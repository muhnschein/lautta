// SPDX-License-Identifier: LGPL-2.1-or-later

use crate::error::Result;
use crate::provider::ReadHandle;

const MAX_BOX_DEPTH: usize = 64;
const MAX_BOXES: usize = 16 * 1024;
const WINDOW: usize = 64 * 1024;

const FIRST_BOXES: [&[u8; 4]; 11] = [
    b"ftyp", b"moov", b"mdat", b"free", b"skip", b"wide", b"pnot", b"moof", b"styp", b"sidx", b"uuid",
];

const CONTAINERS: [&[u8; 4]; 24] = [
    b"moov", b"trak", b"edts", b"mdia", b"minf", b"dinf", b"stbl", b"tref", b"udta", b"mvex", b"moof",
    b"traf", b"mfra", b"gmhd", b"clip", b"matt", b"rmra", b"rmda", b"ilst", b"wave", b"sinf", b"schi",
    b"tapt", b"meta",
];

struct Header {
    kind: [u8; 4],
    body: u64,
    end: u64,
}

#[derive(Default)]
struct Window {
    start: u64,
    data: Vec<u8>,
}

impl Window {
    async fn bytes(&mut self, file: &dyn ReadHandle, at: u64, need: usize) -> Result<&[u8]> {
        let held = at
            .checked_sub(self.start)
            .and_then(|skip| usize::try_from(skip).ok())
            .filter(|&skip| skip + need <= self.data.len());
        if held.is_none() {
            self.start = at;
            self.data = file.read_at(at, WINDOW).await?;
            while self.data.len() < need {
                let more = file.read_at(at + self.data.len() as u64, WINDOW).await?;
                if more.is_empty() {
                    break;
                }
                self.data.extend_from_slice(&more);
            }
        }
        let skip = (at - self.start) as usize;
        Ok(&self.data[skip.min(self.data.len())..(skip + need).min(self.data.len())])
    }

    async fn header(&mut self, file: &dyn ReadHandle, at: u64, limit: u64) -> Result<Option<Header>> {
        if limit.saturating_sub(at) < 8 {
            return Ok(None);
        }
        let head = self.bytes(file, at, 16).await?;
        if head.len() < 8 {
            return Ok(None);
        }
        let short = u64::from(u32::from_be_bytes([head[0], head[1], head[2], head[3]]));
        let kind = [head[4], head[5], head[6], head[7]];
        let (size, header) = match short {
            0 => (limit - at, 8),
            1 if head.len() == 16 => {
                let mut long = [0u8; 8];
                long.copy_from_slice(&head[8..16]);
                (u64::from_be_bytes(long), 16)
            }
            1 => return Ok(None),
            n => (n, 8),
        };
        let Some(end) = at.checked_add(size).filter(|&end| size >= header && end <= limit) else {
            return Ok(None);
        };
        let body = at + header + if &kind == b"meta" { 4 } else { 0 };
        Ok(Some(Header { kind, body, end }))
    }
}

pub async fn nested_too_deeply(file: &dyn ReadHandle) -> Result<bool> {
    let Some(size) = file.size() else {
        return Ok(false);
    };
    let mut window = Window::default();
    let starts_as_boxes = window
        .header(file, 0, size)
        .await?
        .is_some_and(|first| FIRST_BOXES.contains(&&first.kind));
    if !starts_as_boxes {
        return Ok(false);
    }
    let mut ends: Vec<u64> = Vec::new();
    let mut at = 0;
    for _ in 0..MAX_BOXES {
        while ends.last().is_some_and(|&end| at >= end) {
            ends.pop();
        }
        let limit = ends.last().copied().unwrap_or(size);
        match window.header(file, at, limit).await? {
            Some(found) if CONTAINERS.contains(&&found.kind) => {
                ends.push(found.end);
                if ends.len() > MAX_BOX_DEPTH {
                    return Ok(true);
                }
                at = found.body;
            }
            Some(found) => at = found.end,
            None => match ends.pop() {
                Some(end) => at = end,
                None => return Ok(false),
            },
        }
    }
    Ok(false)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::provider::memory::MemoryProvider;
    use crate::provider::{Lane, Provider};
    use crate::vpath::VPath;
    use std::sync::atomic::{AtomicUsize, Ordering};

    const FTYP: &[u8] = b"\x00\x00\x00\x18ftypisom\x00\x00\x02\x00isomiso2";

    fn boxed(kind: &[u8; 4], body: &[u8]) -> Vec<u8> {
        let mut out = u32::try_from(body.len() + 8).unwrap().to_be_bytes().to_vec();
        out.extend_from_slice(kind);
        out.extend_from_slice(body);
        out
    }

    fn nested(kind: &[u8; 4], depth: usize, inner: &[u8]) -> Vec<u8> {
        (0..depth).fold(inner.to_vec(), |body, _| boxed(kind, &body))
    }

    async fn check(bytes: &[u8]) -> bool {
        let mem = MemoryProvider::default();
        mem.add_file("f.mp4", bytes, 0);
        let path = VPath::parse(b"f.mp4").unwrap();
        let file = mem.open_read(&path, Lane::Interactive).await.unwrap();
        nested_too_deeply(file.as_ref()).await.unwrap()
    }

    fn movie(depth: usize) -> Vec<u8> {
        [FTYP.to_vec(), nested(b"moov", depth, &boxed(b"mvhd", &[0; 100]))].concat()
    }

    #[tokio::test]
    async fn boxes_nested_past_the_limit_are_refused_and_at_it_are_not() {
        assert!(!check(&movie(MAX_BOX_DEPTH)).await, "at the limit");
        assert!(check(&movie(MAX_BOX_DEPTH + 1)).await, "one past it");
        let corpus: Vec<u8> = [
            FTYP.to_vec(),
            (0..10_000u32)
                .flat_map(|i| [(8 * (10_000 - i)).to_be_bytes().to_vec(), b"moov".to_vec()].concat())
                .collect(),
        ]
        .concat();
        assert!(check(&corpus).await, "the corpus file of #295");
    }

    #[tokio::test]
    async fn a_usual_movie_passes() {
        let stbl = boxed(
            b"stbl",
            &[
                boxed(b"stsd", &[0; 40]),
                boxed(b"stts", &[0; 16]),
                boxed(b"stco", &[0; 16]),
            ]
            .concat(),
        );
        let trak = boxed(
            b"trak",
            &[
                boxed(b"tkhd", &[0; 84]),
                boxed(
                    b"mdia",
                    &[boxed(b"mdhd", &[0; 24]), boxed(b"minf", &stbl)].concat(),
                ),
            ]
            .concat(),
        );
        let ilst = boxed(
            b"ilst",
            &boxed(b"\xa9nam", &boxed(b"data", b"\0\0\0\x01\0\0\0\0t")),
        );
        let meta = boxed(b"meta", &[vec![0; 4], boxed(b"hdlr", &[0; 25]), ilst].concat());
        let moov = boxed(
            b"moov",
            &[
                boxed(b"mvhd", &[0; 100]),
                trak.clone(),
                trak,
                boxed(b"udta", &meta),
            ]
            .concat(),
        );
        let file = [FTYP.to_vec(), boxed(b"mdat", &[7; 5000]), moov].concat();
        assert!(!check(&file).await);
    }

    #[tokio::test]
    async fn nesting_counts_in_every_kind_of_container_and_after_earlier_boxes() {
        let deep_udta = nested(b"udta", MAX_BOX_DEPTH + 1, &[]);
        let file = [
            FTYP.to_vec(),
            boxed(b"free", &[0; 3]),
            boxed(b"mdat", &[1; 70_000]),
            deep_udta,
        ]
        .concat();
        assert!(check(&file).await, "deep udta after a large mdat");
        let siblings: Vec<u8> = (0..MAX_BOX_DEPTH * 2).flat_map(|_| boxed(b"trak", &[])).collect();
        let wide = [FTYP.to_vec(), boxed(b"moov", &siblings)].concat();
        assert!(!check(&wide).await, "many siblings are not depth");
        let mut meta = nested(b"meta", MAX_BOX_DEPTH + 1, &[]);
        assert!(
            !check(&[FTYP.to_vec(), meta.clone()].concat()).await,
            "meta needs its version to count"
        );
        meta = (0..=MAX_BOX_DEPTH).fold(Vec::new(), |body, _| boxed(b"meta", &[vec![0; 4], body].concat()));
        assert!(
            check(&[FTYP.to_vec(), meta].concat()).await,
            "full meta boxes nest"
        );
    }

    #[tokio::test]
    async fn sizes_of_zero_one_and_lies_are_read_as_the_format_says() {
        let mut to_end = nested(b"moov", MAX_BOX_DEPTH + 1, &[]);
        to_end[..4].copy_from_slice(&0u32.to_be_bytes());
        assert!(
            check(&[FTYP.to_vec(), to_end].concat()).await,
            "size 0 runs to the end"
        );
        let inner = nested(b"moov", MAX_BOX_DEPTH, &[]);
        let mut long = 1u32.to_be_bytes().to_vec();
        long.extend_from_slice(b"moov");
        long.extend_from_slice(&(inner.len() as u64 + 16).to_be_bytes());
        long.extend_from_slice(&inner);
        assert!(
            check(&[FTYP.to_vec(), long].concat()).await,
            "size 1 takes the 64-bit size"
        );
        let mut lie = movie(MAX_BOX_DEPTH + 1);
        lie[FTYP.len()..FTYP.len() + 4].copy_from_slice(&0x7fff_ffffu32.to_be_bytes());
        assert!(
            !check(&lie).await,
            "a box larger than the file is not walked into"
        );
        let mut short = movie(MAX_BOX_DEPTH + 1);
        short[FTYP.len()..FTYP.len() + 4].copy_from_slice(&4u32.to_be_bytes());
        assert!(
            !check(&short).await,
            "a box smaller than its header ends the walk"
        );
        let loops = [FTYP.to_vec(), b"\x00\x00\x00\x08moov".repeat(100_000)].concat();
        assert!(!check(&loops).await, "a flood of empty boxes stays flat");
    }

    #[tokio::test]
    async fn other_files_are_not_walked() {
        let not_boxes = [boxed(b"RIFF", &[]), nested(b"moov", MAX_BOX_DEPTH + 1, &[])].concat();
        assert!(!check(&not_boxes).await);
        assert!(!check(b"").await);
        assert!(!check(b"\x00\x00\x00\x08moo").await);
        assert!(
            check(&nested(b"moov", MAX_BOX_DEPTH + 1, &[])).await,
            "a movie may start with moov"
        );
    }

    struct Counting {
        bytes: Vec<u8>,
        reads: AtomicUsize,
        piece: usize,
    }

    #[async_trait::async_trait]
    impl ReadHandle for Counting {
        fn size(&self) -> Option<u64> {
            Some(self.bytes.len() as u64)
        }

        async fn read_at(&self, offset: u64, max: usize) -> Result<Vec<u8>> {
            self.reads.fetch_add(1, Ordering::Relaxed);
            let start = (offset as usize).min(self.bytes.len());
            let end = (start + max.min(self.piece)).min(self.bytes.len());
            Ok(self.bytes[start..end].to_vec())
        }
    }

    #[tokio::test]
    async fn headers_are_read_in_windows_and_short_reads_are_filled() {
        let file = Counting {
            bytes: movie(MAX_BOX_DEPTH + 1),
            reads: AtomicUsize::new(0),
            piece: usize::MAX,
        };
        assert!(nested_too_deeply(&file).await.unwrap());
        assert_eq!(
            file.reads.load(Ordering::Relaxed),
            1,
            "one window holds all headers"
        );
        let dribble = Counting {
            bytes: movie(MAX_BOX_DEPTH + 1),
            reads: AtomicUsize::new(0),
            piece: 3,
        };
        assert!(
            nested_too_deeply(&dribble).await.unwrap(),
            "reads of 3 bytes still make headers"
        );
    }

    #[tokio::test]
    async fn a_walk_stops_after_its_box_budget() {
        let filler: Vec<u8> = (0..MAX_BOXES).flat_map(|_| boxed(b"free", &[])).collect();
        let late = [FTYP.to_vec(), filler, nested(b"moov", MAX_BOX_DEPTH + 1, &[])].concat();
        assert!(!check(&late).await);
        let filler: Vec<u8> = (0..MAX_BOXES - MAX_BOX_DEPTH - 3)
            .flat_map(|_| boxed(b"free", &[]))
            .collect();
        let early = [FTYP.to_vec(), filler, nested(b"moov", MAX_BOX_DEPTH + 1, &[])].concat();
        assert!(check(&early).await);
    }
}
