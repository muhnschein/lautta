// SPDX-License-Identifier: LGPL-2.1-or-later
//! tar and compressed tar. Plain tar indexed via block cache (headers, data skipped by seek);
//! compressed tar read sequentially from the downloaded cache file; entry data served from
//! [`Spill`], so reading every entry decompresses archive about once, not once per entry.

use super::blocks::{from_io, BlockReader};
use super::{Compression, Loc, RawEntry};
use crate::entry::Kind;
use crate::error::{Error, ErrorKind, Result};
use std::fs::File;
use std::io::{self, BufReader, Read};
use std::os::unix::fs::FileExt;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::{Duration, SystemTime};

/// Decompressed bytes [`Spill`] keeps before starting a new file at the next entry.
pub const SPILL_CAP: u64 = 256 * 1024 * 1024;

pub fn decoder<'a, R: Read + Send + 'a>(
    comp: Compression,
    input: R,
) -> io::Result<Box<dyn Read + Send + 'a>> {
    Ok(match comp {
        Compression::None => Box::new(input),
        Compression::Gzip => Box::new(flate2::read::MultiGzDecoder::new(input)),
        Compression::Bzip2 => Box::new(bzip2::read::BzDecoder::new(input)),
        Compression::Xz => Box::new(xz2::read::XzDecoder::new(input)),
        Compression::Zstd => Box::new(zstd::stream::read::Decoder::new(input)?),
    })
}

fn raw_entry<R: Read>(entry: &tar::Entry<'_, R>, loc: Loc) -> Option<RawEntry> {
    use tar::EntryType as T;
    let header = entry.header();
    let kind = match header.entry_type() {
        T::Regular | T::Continuous => Kind::File,
        T::Directory => Kind::Dir,
        T::Symlink => Kind::Symlink,
        T::Link | T::Char | T::Block | T::Fifo => Kind::Special,
        _ => return None,
    };
    let mtime = header
        .mtime()
        .ok()
        .map(|s| SystemTime::UNIX_EPOCH + Duration::from_secs(s));
    Some(RawEntry {
        name: entry.path_bytes().into_owned(),
        kind,
        size: if kind == Kind::File { entry.size() } else { 0 },
        mtime,
        mode: header.mode().ok().map(|m| m & 0o7777),
        link: entry.link_name_bytes().map(|l| l.into_owned()),
        loc,
    })
}

fn tar_err(err: io::Error) -> Error {
    from_io(err)
}

pub fn index_seek(reader: BlockReader) -> Result<Vec<RawEntry>> {
    let mut archive = tar::Archive::new(reader);
    let mut out = Vec::new();
    for entry in archive.entries_with_seek().map_err(tar_err)? {
        let entry = entry.map_err(tar_err)?;
        let loc = Loc::TarAt(entry.raw_file_position());
        out.extend(raw_entry(&entry, loc));
    }
    Ok(out)
}

/// Entries located by data offset in the decompressed stream.
pub fn index_stream(input: impl Read) -> Result<Vec<RawEntry>> {
    let mut archive = tar::Archive::new(input);
    let mut out = Vec::new();
    for entry in archive.entries().map_err(tar_err)? {
        let entry = entry.map_err(tar_err)?;
        let loc = Loc::TarStream(entry.raw_file_position());
        out.extend(raw_entry(&entry, loc));
    }
    Ok(out)
}

/// Decompressed stream of a compressed tar, read forward once and kept in an unlinked file:
/// entry read in archive order costs only its own bytes. Earlier entry past what file keeps
/// decompresses again from start.
pub struct Spill {
    comp: Compression,
    dir: PathBuf,
    input: Box<dyn Read + Send>,
    /// Stream bytes read from `input`.
    pos: u64,
    file: Arc<File>,
    /// Stream offset of `file`'s first byte; it holds `base..pos`.
    base: u64,
    cap: u64,
}

impl Spill {
    pub fn new(archive: File, comp: Compression, dir: &Path) -> Result<Spill> {
        Ok(Spill {
            comp,
            dir: dir.to_path_buf(),
            input: decoder(comp, BufReader::new(archive)).map_err(tar_err)?,
            pos: 0,
            file: Arc::new(spill_file(dir)?),
            base: 0,
            cap: SPILL_CAP,
        })
    }

    /// File holding stream bytes `offset..offset + len`, and where they start in it.
    pub fn fetch(
        &mut self,
        offset: u64,
        len: u64,
        reopen: impl FnOnce() -> Result<File>,
    ) -> Result<(Arc<File>, u64)> {
        let end = offset
            .checked_add(len)
            .ok_or_else(|| Error::new(ErrorKind::ProtocolError, "tar entry out of range"))?;
        if offset < self.base {
            self.input = decoder(self.comp, BufReader::new(reopen()?)).map_err(tar_err)?;
            self.pos = 0;
            self.skip_to(offset)?;
        } else if offset >= self.pos && end - self.base > self.cap {
            self.skip_to(offset)?;
        }
        let mut buf = vec![0u8; 256 * 1024];
        while self.pos < end {
            let want = usize::try_from(end - self.pos)
                .unwrap_or(usize::MAX)
                .min(buf.len());
            let n = self.input.read(&mut buf[..want]).map_err(tar_err)?;
            if n == 0 {
                break;
            }
            self.file
                .write_all_at(&buf[..n], self.pos - self.base)
                .map_err(tar_err)?;
            self.pos += n as u64;
        }
        Ok((Arc::clone(&self.file), offset - self.base))
    }

    /// Drops kept bytes; next file starts at `offset`.
    fn skip_to(&mut self, offset: u64) -> Result<()> {
        let gap = offset - self.pos;
        let skipped = io::copy(&mut (&mut self.input).take(gap), &mut io::sink()).map_err(tar_err)?;
        self.pos += skipped;
        self.file = Arc::new(spill_file(&self.dir)?);
        self.base = self.pos;
        Ok(())
    }
}

fn spill_file(dir: &Path) -> Result<File> {
    std::fs::create_dir_all(dir)?;
    let path = dir.join(super::unique_name("spill"));
    let file = std::fs::OpenOptions::new()
        .read(true)
        .write(true)
        .create_new(true)
        .open(&path)?;
    // Unlinked at once: data lives as long as the handles.
    std::fs::remove_file(&path)?;
    Ok(file)
}

/// Recognises compressed tar by `ustar` magic.
pub fn decompressed_head(comp: Compression, head: &[u8], limit: usize) -> Vec<u8> {
    let mut out = Vec::new();
    if let Ok(dec) = decoder(comp, head) {
        let mut taken = dec.take(limit as u64);
        // A truncated head ends in an error after the bytes we need.
        let _ = taken.read_to_end(&mut out);
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;
    use std::sync::atomic::{AtomicUsize, Ordering};

    fn tar_bytes() -> Vec<u8> {
        let mut b = tar::Builder::new(Vec::new());
        let mut h = tar::Header::new_gnu();
        h.set_size(5);
        h.set_mode(0o640);
        h.set_mtime(1_700_000_000);
        h.set_cksum();
        b.append_data(&mut h, "dir/a.txt", &b"hello"[..]).unwrap();
        let mut l = tar::Header::new_gnu();
        l.set_entry_type(tar::EntryType::Symlink);
        l.set_size(0);
        b.append_link(&mut l, "ln", "dir/a.txt").unwrap();
        b.into_inner().unwrap()
    }

    #[test]
    fn stream_index_has_links_and_data_offsets() {
        let entries = index_stream(tar_bytes().as_slice()).unwrap();
        assert_eq!(entries.len(), 2);
        assert_eq!(entries[0].kind, Kind::File);
        assert_eq!(entries[0].size, 5);
        assert_eq!(entries[0].mode, Some(0o640));
        assert!(matches!(entries[0].loc, Loc::TarStream(512)));
        assert_eq!(entries[1].kind, Kind::Symlink);
        assert_eq!(entries[1].link.as_deref(), Some(&b"dir/a.txt"[..]));
        assert!(matches!(entries[1].loc, Loc::TarStream(1536)));
    }

    #[test]
    fn decompressed_head_sniffs_gzip_tar() {
        let mut enc = flate2::write::GzEncoder::new(Vec::new(), flate2::Compression::fast());
        enc.write_all(&tar_bytes()).unwrap();
        let gz = enc.finish().unwrap();
        let head = decompressed_head(Compression::Gzip, &gz, 512);
        assert_eq!(&head[257..262], b"ustar");
        assert!(decompressed_head(Compression::Gzip, b"not gzip", 512).is_empty());
    }

    fn gz_of(entries: &[Vec<u8>]) -> (tempfile::TempDir, PathBuf) {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("t.tar.gz");
        let gz = flate2::write::GzEncoder::new(File::create(&path).unwrap(), flate2::Compression::fast());
        let mut b = tar::Builder::new(gz);
        for (i, data) in entries.iter().enumerate() {
            let mut h = tar::Header::new_gnu();
            h.set_size(data.len() as u64);
            h.set_cksum();
            b.append_data(&mut h, format!("e{i}"), data.as_slice()).unwrap();
        }
        b.into_inner().unwrap().finish().unwrap();
        (dir, path)
    }

    fn read(spill: &mut Spill, loc: (u64, u64), path: &Path, reopened: &AtomicUsize) -> Vec<u8> {
        let (file, at) = spill
            .fetch(loc.0, loc.1, || {
                reopened.fetch_add(1, Ordering::SeqCst);
                Ok(File::open(path)?)
            })
            .unwrap();
        let mut out = vec![0u8; usize::try_from(loc.1).unwrap()];
        file.read_exact_at(&mut out, at).unwrap();
        out
    }

    fn spill_of(entries: &[Vec<u8>]) -> (tempfile::TempDir, PathBuf, Vec<(u64, u64)>, Spill) {
        let (dir, path) = gz_of(entries);
        let raw =
            index_stream(decoder(Compression::Gzip, BufReader::new(File::open(&path).unwrap())).unwrap())
                .unwrap();
        let locs = raw
            .iter()
            .map(|e| match e.loc {
                Loc::TarStream(at) => (at, e.size),
                _ => panic!("compressed tar entry not located by stream offset"),
            })
            .collect();
        let spill = Spill::new(File::open(&path).unwrap(), Compression::Gzip, dir.path()).unwrap();
        (dir, path, locs, spill)
    }

    fn entries() -> Vec<Vec<u8>> {
        (0..6u8).map(|i| vec![i; 700 + usize::from(i) * 100]).collect()
    }

    #[test]
    fn spill_reads_entries_in_order_from_one_pass() {
        let data = entries();
        let (_d, path, locs, mut spill) = spill_of(&data);
        let reopened = AtomicUsize::new(0);
        for i in [0, 1, 3, 2, 5, 4] {
            assert_eq!(read(&mut spill, locs[i], &path, &reopened), data[i], "entry {i}");
            if i < 2 {
                assert_eq!(spill.pos, locs[i].0 + locs[i].1, "entry {i}: no read past it");
            }
        }
        assert_eq!(reopened.load(Ordering::SeqCst), 0, "stream decompressed once");
    }

    #[test]
    fn spill_past_cap_keeps_later_entries_only_and_rereads_earlier() {
        let data = entries();
        let (_d, path, locs, mut spill) = spill_of(&data);
        // Entries 4 and 5 fit together, 0 to 4 do not.
        spill.cap = locs[5].0 + locs[5].1 - locs[4].0;
        let reopened = AtomicUsize::new(0);
        assert_eq!(read(&mut spill, locs[0], &path, &reopened), data[0]);
        assert_eq!(read(&mut spill, locs[4], &path, &reopened), data[4]);
        assert_eq!(spill.base, locs[4].0, "file starts at entry past the cap");
        assert_eq!(read(&mut spill, locs[5], &path, &reopened), data[5]);
        assert_eq!(spill.base, locs[4].0, "entry fitting the cap keeps the file");
        assert_eq!(reopened.load(Ordering::SeqCst), 0);
        assert_eq!(read(&mut spill, locs[1], &path, &reopened), data[1]);
        assert_eq!(
            reopened.load(Ordering::SeqCst),
            1,
            "earlier entry decompresses again"
        );
        assert_eq!(read(&mut spill, locs[2], &path, &reopened), data[2]);
        assert_eq!(reopened.load(Ordering::SeqCst), 1);
    }

    #[test]
    fn spill_keeps_256_mib() {
        assert_eq!(SPILL_CAP, 256 << 20);
    }

    #[test]
    fn spill_reports_short_stream() {
        let data = entries();
        let (_d, path, locs, mut spill) = spill_of(&data);
        let reopened = AtomicUsize::new(0);
        let (_, len) = locs[5];
        let (file, at) = spill
            .fetch(locs[5].0, len + 10_000, || Ok(File::open(&path)?))
            .unwrap();
        assert!(
            file.metadata().unwrap().len() < at + len + 10_000,
            "no bytes made up past end"
        );
        assert_eq!(reopened.load(Ordering::SeqCst), 0);
    }
}
