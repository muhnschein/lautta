// SPDX-License-Identifier: LGPL-2.1-or-later
//! `Read + Seek` over a [`ReadHandle`] with a 64 KiB block cache (PRV-10), so a
//! remote zip needs ranged reads only: the end of the file (central
//! directory) first, then the blocks of the entries that are actually read.

use crate::error::{Error, ErrorKind};
use crate::provider::ReadHandle;
use std::io::{self, Read, Seek, SeekFrom};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex};

/// Cache block size.
pub const BLOCK: u64 = 64 * 1024;
/// Blocks kept (2 MiB).
const CACHE_BLOCKS: usize = 32;

type Cache = Vec<(u64, Arc<Vec<u8>>)>;

/// Wraps a lautta error so it survives a trip through `std::io`.
pub fn to_io(err: Error) -> io::Error {
    io::Error::new(io::ErrorKind::Other, err)
}

/// Inverse of [`to_io`]; plain I/O errors are mapped by kind.
pub fn from_io(err: io::Error) -> Error {
    let inner = err
        .get_ref()
        .and_then(<dyn std::error::Error + Send + Sync>::downcast_ref::<Error>)
        .cloned();
    match inner {
        Some(e) => e,
        None if err.kind() == io::ErrorKind::InvalidData || err.kind() == io::ErrorKind::UnexpectedEof => {
            Error::new(ErrorKind::ProtocolError, format!("corrupt archive: {err}"))
        }
        None => Error::from_io(&err),
    }
}

/// Shared block cache over one read handle.
pub struct BlockSource {
    handle: Arc<dyn ReadHandle>,
    rt: tokio::runtime::Handle,
    size: u64,
    cache: Mutex<Cache>,
    reads: AtomicU64,
}

impl BlockSource {
    /// `first_block` seeds block 0 when the caller already read the head.
    pub fn new(
        handle: Arc<dyn ReadHandle>,
        rt: tokio::runtime::Handle,
        size: u64,
        first_block: Option<Vec<u8>>,
    ) -> Arc<BlockSource> {
        let mut cache = Vec::new();
        if let Some(head) = first_block {
            if head.len() as u64 == BLOCK.min(size) {
                cache.push((0, Arc::new(head)));
            }
        }
        Arc::new(BlockSource {
            handle,
            rt,
            size,
            cache: Mutex::new(cache),
            reads: AtomicU64::new(0),
        })
    }

    pub fn size(&self) -> u64 {
        self.size
    }

    /// Ranged reads issued to the underlying handle so far.
    pub fn read_count(&self) -> u64 {
        self.reads.load(Ordering::Relaxed)
    }

    pub fn reader(self: &Arc<Self>) -> BlockReader {
        BlockReader {
            src: Arc::clone(self),
            pos: 0,
        }
    }

    fn lock(&self) -> std::sync::MutexGuard<'_, Cache> {
        match self.cache.lock() {
            Ok(g) => g,
            Err(p) => p.into_inner(),
        }
    }

    fn cached(&self, idx: u64) -> Option<Arc<Vec<u8>>> {
        let mut cache = self.lock();
        let at = cache.iter().position(|(i, _)| *i == idx)?;
        let hit = cache.remove(at);
        let data = Arc::clone(&hit.1);
        cache.push(hit);
        Some(data)
    }

    fn fetch(&self, idx: u64) -> io::Result<Arc<Vec<u8>>> {
        let start = idx * BLOCK;
        let want = BLOCK.min(self.size.saturating_sub(start));
        let mut data: Vec<u8> = Vec::with_capacity(usize::try_from(want).unwrap_or(0));
        while (data.len() as u64) < want {
            self.reads.fetch_add(1, Ordering::Relaxed);
            let missing = usize::try_from(want - data.len() as u64).unwrap_or(usize::MAX);
            let chunk = self
                .rt
                .block_on(self.handle.read_at(start + data.len() as u64, missing))
                .map_err(to_io)?;
            if chunk.is_empty() {
                break;
            }
            data.extend_from_slice(&chunk);
        }
        let data = Arc::new(data);
        let mut cache = self.lock();
        if cache.len() >= CACHE_BLOCKS {
            cache.remove(0);
        }
        cache.push((idx, Arc::clone(&data)));
        Ok(data)
    }

    fn block(&self, idx: u64) -> io::Result<Arc<Vec<u8>>> {
        match self.cached(idx) {
            Some(b) => Ok(b),
            None => self.fetch(idx),
        }
    }
}

/// A cursor over a [`BlockSource`].
pub struct BlockReader {
    src: Arc<BlockSource>,
    pos: u64,
}

impl BlockReader {
    pub fn size(&self) -> u64 {
        self.src.size
    }

    /// Reads exactly `len` bytes at `offset`.
    pub fn read_exact_at(&mut self, offset: u64, len: usize) -> io::Result<Vec<u8>> {
        self.pos = offset;
        let mut buf = vec![0u8; len];
        self.read_exact(&mut buf)?;
        Ok(buf)
    }
}

impl Read for BlockReader {
    fn read(&mut self, out: &mut [u8]) -> io::Result<usize> {
        if out.is_empty() || self.pos >= self.src.size {
            return Ok(0);
        }
        let block = self.src.block(self.pos / BLOCK)?;
        let within = usize::try_from(self.pos % BLOCK).unwrap_or(0);
        let avail = block.get(within..).unwrap_or(&[]);
        if avail.is_empty() {
            return Err(io::Error::from(io::ErrorKind::UnexpectedEof));
        }
        let n = avail.len().min(out.len());
        out[..n].copy_from_slice(&avail[..n]);
        self.pos += n as u64;
        Ok(n)
    }
}

impl Seek for BlockReader {
    fn seek(&mut self, to: SeekFrom) -> io::Result<u64> {
        let next = match to {
            SeekFrom::Start(p) => Some(p),
            SeekFrom::End(d) => self.src.size.checked_add_signed(d),
            SeekFrom::Current(d) => self.pos.checked_add_signed(d),
        };
        match next {
            Some(p) => {
                self.pos = p;
                Ok(p)
            }
            None => Err(io::Error::new(io::ErrorKind::InvalidInput, "seek before start")),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::provider::memory::MemoryProvider;
    use crate::provider::{Lane, Provider};
    use crate::vpath::VPath;

    async fn source(len: usize) -> (Arc<BlockSource>, Vec<u8>) {
        let data: Vec<u8> = (0..len).map(|i| (i % 251) as u8).collect();
        let mem = MemoryProvider::default();
        mem.add_file("f", &data, 0);
        let h = mem
            .open_read(&VPath::parse(b"f").unwrap_or_default(), Lane::Bulk)
            .await
            .unwrap();
        let h: Arc<dyn ReadHandle> = Arc::from(h);
        (
            BlockSource::new(h, tokio::runtime::Handle::current(), len as u64, None),
            data,
        )
    }

    #[tokio::test]
    async fn reads_across_blocks_and_caches() {
        let (src, data) = source(200_000).await;
        let src2 = Arc::clone(&src);
        let out = tokio::task::spawn_blocking(move || {
            let mut r = src2.reader();
            r.seek(SeekFrom::Start(65_000)).unwrap();
            let mut buf = vec![0u8; 2000];
            r.read_exact(&mut buf).unwrap();
            let first = src2.read_count();
            r.seek(SeekFrom::Start(65_100)).unwrap();
            let mut again = vec![0u8; 100];
            r.read_exact(&mut again).unwrap();
            (buf, first, src2.read_count())
        })
        .await
        .unwrap();
        assert_eq!(out.0, data[65_000..67_000]);
        assert_eq!(out.1, 2, "two blocks fetched");
        assert_eq!(out.2, 2, "second read served from the cache");
    }

    #[tokio::test]
    async fn seek_from_end_and_eof() {
        let (src, data) = source(70_000).await;
        let out = tokio::task::spawn_blocking(move || {
            let mut r = src.reader();
            let end = r.seek(SeekFrom::End(-10)).unwrap();
            let mut tail = Vec::new();
            r.read_to_end(&mut tail).unwrap();
            let bad = r.seek(SeekFrom::Current(-1_000_000));
            (end, tail, bad.is_err(), r.size())
        })
        .await
        .unwrap();
        assert_eq!(out.0, 69_990);
        assert_eq!(out.1, data[69_990..]);
        assert!(out.2);
        assert_eq!(out.3, 70_000);
    }

    #[test]
    fn io_error_round_trip_keeps_kind() {
        let e = Error::new(ErrorKind::ConnectionLost, "gone");
        assert_eq!(from_io(to_io(e)).kind, ErrorKind::ConnectionLost);
        let corrupt = from_io(io::Error::from(io::ErrorKind::UnexpectedEof));
        assert_eq!(corrupt.kind, ErrorKind::ProtocolError);
    }
}
