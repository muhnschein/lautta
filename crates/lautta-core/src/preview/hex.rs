// SPDX-License-Identifier: LGPL-2.1-or-later
//! Hex viewer model (SPEC PRV-4): rows of 16 bytes fetched with ranged reads
//! through a small page cache, so a multi-gigabyte remote file can be
//! scrolled without downloading it.

use crate::error::{Error, ErrorKind, Result};
use crate::provider::ReadHandle;
use std::collections::HashMap;
use std::sync::Arc;

/// Bytes per row.
pub const ROW_BYTES: usize = 16;
/// Rows per cached page (4 KiB).
pub const PAGE_ROWS: u64 = 256;
const PAGE_BYTES: u64 = PAGE_ROWS * ROW_BYTES as u64;
/// Pages kept (128 KiB).
const CACHE_PAGES: usize = 32;
/// Rows a single `rows` call may return.
pub const MAX_ROWS_PER_CALL: usize = 512;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct HexRow {
    pub offset: u64,
    /// `00000010`
    pub offset_text: String,
    /// `48 65 6c 6c 6f 20 57 6f  72 6c 64` padded to a fixed width.
    pub hex: String,
    /// Printable ASCII, `.` otherwise (one character per byte).
    pub ascii: String,
    /// Bytes in the row (16 except possibly the last row).
    pub len: usize,
}

/// Formats one row (pure; the unit under fuzzing).
pub fn format_row(offset: u64, bytes: &[u8]) -> HexRow {
    let bytes = &bytes[..bytes.len().min(ROW_BYTES)];
    let mut hex = String::with_capacity(ROW_BYTES * 3 + 1);
    let mut ascii = String::with_capacity(ROW_BYTES);
    for i in 0..ROW_BYTES {
        if i == ROW_BYTES / 2 {
            hex.push(' ');
        }
        match bytes.get(i) {
            Some(b) => {
                hex.push_str(&format!("{b:02x}"));
                ascii.push(if (0x20..0x7f).contains(b) {
                    char::from(*b)
                } else {
                    '.'
                });
            }
            None => hex.push_str("  "),
        }
        if i + 1 < ROW_BYTES {
            hex.push(' ');
        }
    }
    HexRow {
        offset,
        offset_text: format!("{offset:08x}"),
        hex,
        ascii,
        len: bytes.len(),
    }
}

/// Paged view over a read handle.
pub struct HexView {
    handle: Arc<dyn ReadHandle>,
    size: u64,
    /// Page index to bytes, least recently used first in `order`.
    pages: HashMap<u64, Arc<Vec<u8>>>,
    order: Vec<u64>,
    reads: u64,
}

impl HexView {
    /// The size must be known (it is for files; streams cannot be viewed).
    pub fn new(handle: Arc<dyn ReadHandle>) -> Result<HexView> {
        let size = handle
            .size()
            .ok_or_else(|| Error::new(ErrorKind::Unsupported, "size unknown"))?;
        Ok(HexView {
            handle,
            size,
            pages: HashMap::new(),
            order: Vec::new(),
            reads: 0,
        })
    }

    pub fn size(&self) -> u64 {
        self.size
    }

    pub fn row_count(&self) -> u64 {
        self.size.div_ceil(ROW_BYTES as u64)
    }

    /// Ranged reads issued so far.
    pub fn read_count(&self) -> u64 {
        self.reads
    }

    /// The row that shows `offset`; errors when it is outside the file.
    pub fn goto_offset(&self, offset: u64) -> Result<u64> {
        if offset >= self.size {
            return Err(Error::new(
                ErrorKind::InvalidArgument,
                "offset beyond end of file",
            ));
        }
        Ok(offset / ROW_BYTES as u64)
    }

    /// Byte offset of the first byte of `row`.
    pub fn offset_of_row(row: u64) -> u64 {
        row.saturating_mul(ROW_BYTES as u64)
    }

    /// Rows `first_row..first_row + count` (fewer at the end of the file).
    pub async fn rows(&mut self, first_row: u64, count: usize) -> Result<Vec<HexRow>> {
        let count = count.min(MAX_ROWS_PER_CALL) as u64;
        let last = first_row.saturating_add(count).min(self.row_count());
        let mut out = Vec::new();
        for row in first_row..last {
            let offset = Self::offset_of_row(row);
            let page = self.page(offset / PAGE_BYTES).await?;
            let within = usize::try_from(offset % PAGE_BYTES).unwrap_or(0);
            let end = (within + ROW_BYTES).min(page.len());
            let bytes = page.get(within..end).unwrap_or(&[]);
            if bytes.is_empty() {
                break;
            }
            out.push(format_row(offset, bytes));
        }
        Ok(out)
    }

    async fn page(&mut self, index: u64) -> Result<Arc<Vec<u8>>> {
        if let Some(p) = self.pages.get(&index) {
            let p = Arc::clone(p);
            self.order.retain(|i| *i != index);
            self.order.push(index);
            return Ok(p);
        }
        let start = index * PAGE_BYTES;
        let want = PAGE_BYTES.min(self.size.saturating_sub(start));
        let mut data: Vec<u8> = Vec::new();
        while (data.len() as u64) < want {
            self.reads += 1;
            let missing = usize::try_from(want - data.len() as u64).unwrap_or(usize::MAX);
            let chunk = self.handle.read_at(start + data.len() as u64, missing).await?;
            if chunk.is_empty() {
                break;
            }
            data.extend_from_slice(&chunk);
        }
        let page = Arc::new(data);
        if self.order.len() >= CACHE_PAGES {
            let evicted = self.order.remove(0);
            self.pages.remove(&evicted);
        }
        self.pages.insert(index, Arc::clone(&page));
        self.order.push(index);
        Ok(page)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::provider::memory::MemoryProvider;
    use crate::provider::{Lane, Provider};
    use crate::vpath::VPath;

    async fn view(data: &[u8]) -> HexView {
        let mem = MemoryProvider::default();
        mem.add_file("f", data, 0);
        let h = mem
            .open_read(&VPath::parse(b"f").unwrap_or_default(), Lane::Interactive)
            .await
            .unwrap();
        HexView::new(Arc::from(h)).unwrap()
    }

    #[test]
    fn full_row_format() {
        let row = format_row(0x10, b"Hello World!\x00\x01\xff~");
        assert_eq!(row.offset_text, "00000010");
        assert_eq!(row.hex, "48 65 6c 6c 6f 20 57 6f  72 6c 64 21 00 01 ff 7e");
        assert_eq!(row.ascii, "Hello World!...~");
        assert_eq!(row.len, 16);
    }

    #[test]
    fn short_row_is_padded_to_the_same_width() {
        let full = format_row(0, &[0u8; 16]);
        let short = format_row(0, b"ab");
        assert_eq!(short.hex.len(), full.hex.len());
        assert!(short.hex.starts_with("61 62   "));
        assert_eq!(short.ascii, "ab");
        assert_eq!(short.len, 2);
        let empty = format_row(0, b"");
        assert_eq!((empty.hex.trim(), empty.len), ("", 0));
    }

    #[test]
    fn oversized_input_is_cut_to_one_row() {
        assert_eq!(format_row(0, &[65u8; 40]).ascii, "A".repeat(16));
    }

    #[tokio::test]
    async fn rows_page_through_the_file() {
        let data: Vec<u8> = (0..=255u8).cycle().take(40).collect();
        let mut v = view(&data).await;
        assert_eq!((v.size(), v.row_count()), (40, 3));
        let rows = v.rows(0, 10).await.unwrap();
        assert_eq!(rows.len(), 3);
        assert_eq!(rows[1].offset, 16);
        assert_eq!(rows[1].hex.split_whitespace().next(), Some("10"));
        assert_eq!(rows[2].len, 8);
        let tail = v.rows(2, 1).await.unwrap();
        assert_eq!(tail, rows[2..]);
        assert!(v.rows(3, 5).await.unwrap().is_empty());
        assert!(v.rows(u64::MAX, 5).await.unwrap().is_empty());
    }

    #[tokio::test]
    async fn pages_are_cached_and_evicted() {
        let data = vec![7u8; (PAGE_BYTES as usize) * (CACHE_PAGES + 3)];
        let mut v = view(&data).await;
        v.rows(0, 4).await.unwrap();
        v.rows(1, 4).await.unwrap();
        assert_eq!(v.read_count(), 1, "same page, one ranged read");
        v.rows(PAGE_ROWS, 1).await.unwrap();
        assert_eq!(v.read_count(), 2);
        for page in 2..(CACHE_PAGES as u64 + 3) {
            v.rows(page * PAGE_ROWS, 1).await.unwrap();
        }
        let before = v.read_count();
        v.rows(0, 1).await.unwrap();
        assert_eq!(v.read_count(), before + 1, "oldest page was evicted");
        v.rows(0, 1).await.unwrap();
        assert_eq!(v.read_count(), before + 1);
    }

    #[tokio::test]
    async fn a_row_never_straddles_pages_wrongly() {
        let data: Vec<u8> = (0..(PAGE_BYTES * 2 + 5)).map(|i| (i % 251) as u8).collect();
        let mut v = view(&data).await;
        let rows = v.rows(PAGE_ROWS - 1, 3).await.unwrap();
        for r in &rows {
            let start = r.offset as usize;
            let expect = format_row(r.offset, &data[start..(start + 16).min(data.len())]);
            assert_eq!(*r, expect);
        }
        assert_eq!(rows.len(), 3);
    }

    #[tokio::test]
    async fn goto_offset_maps_to_rows_and_rejects_the_end() {
        let v = view(&[0u8; 100]).await;
        assert_eq!(v.goto_offset(0).unwrap(), 0);
        assert_eq!(v.goto_offset(15).unwrap(), 0);
        assert_eq!(v.goto_offset(16).unwrap(), 1);
        assert_eq!(v.goto_offset(99).unwrap(), 6);
        assert_eq!(
            v.goto_offset(100).err().map(|e| e.kind),
            Some(ErrorKind::InvalidArgument)
        );
        assert_eq!(HexView::offset_of_row(6), 96);
        let empty = view(b"").await;
        assert!(empty.goto_offset(0).is_err());
        assert_eq!(empty.row_count(), 0);
    }

    #[tokio::test]
    async fn per_call_row_limit_applies() {
        let mut v = view(&vec![1u8; 16 * 1000]).await;
        assert_eq!(v.rows(0, 5000).await.unwrap().len(), MAX_ROWS_PER_CALL);
    }

    struct NoSize;

    #[async_trait::async_trait]
    impl ReadHandle for NoSize {
        fn size(&self) -> Option<u64> {
            None
        }
        async fn read_at(&self, _offset: u64, _max: usize) -> Result<Vec<u8>> {
            Ok(Vec::new())
        }
    }

    #[test]
    fn unknown_size_is_unsupported() {
        let err = HexView::new(Arc::new(NoSize)).err();
        assert_eq!(err.map(|e| e.kind), Some(ErrorKind::Unsupported));
    }
}
