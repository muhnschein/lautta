// SPDX-License-Identifier: LGPL-2.1-or-later
//! Zip central directory parser and entry reader (PRV-10).
//!
//! The `zip` crate reads every entry's local header when it opens an entry,
//! which would touch the whole file just to list it. This reader parses the
//! end-of-central-directory record and the central directory (two or three
//! ranged reads), and reads an entry's local header only when it is opened.

use super::blocks::{from_io, BlockReader, BlockSource};
use super::{Loc, RawEntry};
use crate::entry::Kind;
use crate::error::{Error, ErrorKind, Result};
use std::io::{self, Read, Seek, SeekFrom};
use std::sync::Arc;
use std::time::{Duration, SystemTime};

const EOCD_SIG: [u8; 4] = [0x50, 0x4b, 0x05, 0x06];
const EOCD_LEN: usize = 22;
const MAX_COMMENT: u64 = 65_535;
const ZIP64_LOCATOR_SIG: u32 = 0x0706_4b50;
const ZIP64_EOCD_SIG: u32 = 0x0606_4b50;
const CENTRAL_SIG: u32 = 0x0201_4b50;
const LOCAL_SIG: u32 = 0x0403_4b50;
const CENTRAL_FIXED: usize = 46;
/// A central directory larger than this is refused (memory bound).
const MAX_DIRECTORY: u64 = 256 * 1024 * 1024;

/// Where and how an entry's bytes are stored.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ZipLoc {
    pub method: u16,
    pub flags: u16,
    pub crc: u32,
    pub compressed: u64,
    pub size: u64,
    pub header_offset: u64,
}

fn corrupt(what: &str) -> Error {
    Error::new(ErrorKind::ProtocolError, format!("corrupt zip: {what}"))
}

fn le16(b: &[u8], at: usize) -> Option<u16> {
    let s = b.get(at..at.checked_add(2)?)?;
    Some(u16::from_le_bytes([s[0], s[1]]))
}

fn le32(b: &[u8], at: usize) -> Option<u32> {
    let s = b.get(at..at.checked_add(4)?)?;
    Some(u32::from_le_bytes([s[0], s[1], s[2], s[3]]))
}

fn le64(b: &[u8], at: usize) -> Option<u64> {
    let s = b.get(at..at.checked_add(8)?)?;
    let mut a = [0u8; 8];
    a.copy_from_slice(s);
    Some(u64::from_le_bytes(a))
}

/// Location of the central directory: (offset, size, entry count).
struct Directory {
    offset: u64,
    size: u64,
    count: u64,
}

/// Reads the whole archive's entries (ranged: tail, then the directory).
pub fn read_entries(reader: &mut BlockReader) -> Result<Vec<RawEntry>> {
    let dir = locate_directory(reader)?;
    if dir.size > MAX_DIRECTORY
        || dir
            .offset
            .checked_add(dir.size)
            .map_or(true, |e| e > reader.size())
    {
        return Err(corrupt("central directory out of range"));
    }
    let len = usize::try_from(dir.size).map_err(|_| corrupt("central directory too large"))?;
    let bytes = reader.read_exact_at(dir.offset, len).map_err(from_io)?;
    parse_directory(&bytes, dir.count)
}

fn locate_directory(reader: &mut BlockReader) -> Result<Directory> {
    let size = reader.size();
    if size < EOCD_LEN as u64 {
        return Err(corrupt("too small"));
    }
    let tail_len = size.min(EOCD_LEN as u64 + MAX_COMMENT);
    let tail_start = size - tail_len;
    let tail = reader
        .read_exact_at(tail_start, usize::try_from(tail_len).unwrap_or(0))
        .map_err(from_io)?;
    let at = tail
        .windows(4)
        .rposition(|w| w == EOCD_SIG)
        .filter(|p| p + EOCD_LEN <= tail.len())
        .ok_or_else(|| corrupt("no end of central directory"))?;
    let eocd = &tail[at..];
    let count = u64::from(le16(eocd, 10).unwrap_or(0));
    let dir_size = u64::from(le32(eocd, 12).unwrap_or(0));
    let dir_offset = u64::from(le32(eocd, 16).unwrap_or(0));
    let needs64 = count == 0xFFFF || dir_size == 0xFFFF_FFFF || dir_offset == 0xFFFF_FFFF;
    if !needs64 {
        return Ok(Directory {
            offset: dir_offset,
            size: dir_size,
            count,
        });
    }
    locate_zip64(reader, tail_start + at as u64)
}

fn locate_zip64(reader: &mut BlockReader, eocd_pos: u64) -> Result<Directory> {
    let loc_pos = eocd_pos
        .checked_sub(20)
        .ok_or_else(|| corrupt("no zip64 locator"))?;
    let loc = reader.read_exact_at(loc_pos, 20).map_err(from_io)?;
    if le32(&loc, 0) != Some(ZIP64_LOCATOR_SIG) {
        return Err(corrupt("no zip64 locator"));
    }
    let rec_pos = le64(&loc, 8).ok_or_else(|| corrupt("zip64 locator"))?;
    let rec = reader.read_exact_at(rec_pos, 56).map_err(from_io)?;
    if le32(&rec, 0) != Some(ZIP64_EOCD_SIG) {
        return Err(corrupt("bad zip64 end record"));
    }
    match (le64(&rec, 32), le64(&rec, 40), le64(&rec, 48)) {
        (Some(count), Some(size), Some(offset)) => Ok(Directory { offset, size, count }),
        _ => Err(corrupt("zip64 end record")),
    }
}

fn parse_directory(bytes: &[u8], count: u64) -> Result<Vec<RawEntry>> {
    let mut out = Vec::new();
    let mut at = 0usize;
    for _ in 0..count {
        let (entry, next) = parse_central_entry(bytes, at)?;
        out.push(entry);
        at = next;
    }
    Ok(out)
}

fn parse_central_entry(b: &[u8], at: usize) -> Result<(RawEntry, usize)> {
    let bad = || corrupt("central directory entry");
    if le32(b, at) != Some(CENTRAL_SIG) {
        return Err(bad());
    }
    let made_by = le16(b, at + 4).ok_or_else(bad)?;
    let flags = le16(b, at + 8).ok_or_else(bad)?;
    let method = le16(b, at + 10).ok_or_else(bad)?;
    let dos_time = le16(b, at + 12).ok_or_else(bad)?;
    let dos_date = le16(b, at + 14).ok_or_else(bad)?;
    let crc = le32(b, at + 16).ok_or_else(bad)?;
    let mut compressed = u64::from(le32(b, at + 20).ok_or_else(bad)?);
    let mut size = u64::from(le32(b, at + 24).ok_or_else(bad)?);
    let name_len = usize::from(le16(b, at + 28).ok_or_else(bad)?);
    let extra_len = usize::from(le16(b, at + 30).ok_or_else(bad)?);
    let comment_len = usize::from(le16(b, at + 32).ok_or_else(bad)?);
    let attrs = le32(b, at + 38).ok_or_else(bad)?;
    let mut header_offset = u64::from(le32(b, at + 42).ok_or_else(bad)?);
    let name_at = at + CENTRAL_FIXED;
    let name = b.get(name_at..name_at + name_len).ok_or_else(bad)?;
    let extra = b
        .get(name_at + name_len..name_at + name_len + extra_len)
        .ok_or_else(bad)?;
    let next = name_at + name_len + extra_len + comment_len;
    if next > b.len() {
        return Err(bad());
    }
    let mut mtime = Some(dos_to_time(dos_date, dos_time));
    for (id, data) in extra_fields(extra) {
        match id {
            0x0001 => apply_zip64(data, &mut size, &mut compressed, &mut header_offset),
            0x5455 => mtime = unix_mtime(data).or(mtime),
            _ => {}
        }
    }
    let (kind, mode) = classify(made_by, attrs, name);
    let loc = ZipLoc {
        method,
        flags,
        crc,
        compressed,
        size,
        header_offset,
    };
    let entry = RawEntry {
        name: name.to_vec(),
        kind,
        size: if kind == Kind::File || kind == Kind::Symlink {
            size
        } else {
            0
        },
        mtime,
        mode,
        link: None,
        loc: Loc::Zip(loc),
    };
    Ok((entry, next))
}

fn extra_fields(mut extra: &[u8]) -> Vec<(u16, &[u8])> {
    let mut out = Vec::new();
    while let (Some(id), Some(len)) = (le16(extra, 0), le16(extra, 2)) {
        let end = 4 + usize::from(len);
        match extra.get(4..end) {
            Some(data) => out.push((id, data)),
            None => break,
        }
        extra = &extra[end..];
    }
    out
}

/// The zip64 field carries, in order, only the values that were `0xFFFFFFFF`
/// in the fixed header; whichever ones the header marks are replaced here.
fn apply_zip64(data: &[u8], size: &mut u64, compressed: &mut u64, offset: &mut u64) {
    let mut at = 0;
    for slot in [size, compressed, offset] {
        if *slot == 0xFFFF_FFFF {
            match le64(data, at) {
                Some(v) => *slot = v,
                None => return,
            }
            at += 8;
        }
    }
}

fn unix_mtime(data: &[u8]) -> Option<SystemTime> {
    let flags = *data.first()?;
    if flags & 1 == 0 {
        return None;
    }
    let secs = le32(data, 1)?;
    Some(SystemTime::UNIX_EPOCH + Duration::from_secs(u64::from(secs)))
}

fn classify(made_by: u16, attrs: u32, name: &[u8]) -> (Kind, Option<u32>) {
    let host_unix = made_by >> 8 == 3;
    let unix_mode = attrs >> 16;
    if host_unix && unix_mode != 0 {
        let kind = match unix_mode & 0o170_000 {
            0o120_000 => Kind::Symlink,
            0o040_000 => Kind::Dir,
            0o100_000 | 0 => Kind::File,
            _ => Kind::Special,
        };
        return (kind, Some(unix_mode & 0o7777));
    }
    if name.ends_with(b"/") || attrs & 0x10 != 0 {
        (Kind::Dir, None)
    } else {
        (Kind::File, None)
    }
}

/// DOS date/time (local time, treated as UTC) to `SystemTime`.
pub fn dos_to_time(date: u16, time: u16) -> SystemTime {
    let year = 1980 + i64::from(date >> 9);
    let month = i64::from((date >> 5) & 0xF).clamp(1, 12);
    let day = i64::from(date & 0x1F).max(1);
    let secs = days_from_civil(year, month, day) * 86_400
        + i64::from(time >> 11) * 3600
        + i64::from((time >> 5) & 0x3F) * 60
        + i64::from(time & 0x1F) * 2;
    SystemTime::UNIX_EPOCH + Duration::from_secs(u64::try_from(secs).unwrap_or(0))
}

fn days_from_civil(year: i64, month: i64, day: i64) -> i64 {
    let y = if month <= 2 { year - 1 } else { year };
    let era = y.div_euclid(400);
    let yoe = y - era * 400;
    let mp = if month > 2 { month - 3 } else { month + 9 };
    let doy = (153 * mp + 2) / 5 + day - 1;
    let doe = yoe * 365 + yoe / 4 - yoe / 100 + doy;
    era * 146_097 + doe - 719_468
}

/// Opens the decompressed bytes of one entry (reads its local header).
pub fn open_entry(src: &Arc<BlockSource>, loc: &ZipLoc) -> Result<Box<dyn Read + Send>> {
    if loc.flags & 1 != 0 {
        return Err(Error::new(ErrorKind::Unsupported, "encrypted zip entry"));
    }
    let mut reader = src.reader();
    let head = reader.read_exact_at(loc.header_offset, 30).map_err(from_io)?;
    if le32(&head, 0) != Some(LOCAL_SIG) {
        return Err(corrupt("bad local header"));
    }
    let name_len = u64::from(le16(&head, 26).unwrap_or(0));
    let extra_len = u64::from(le16(&head, 28).unwrap_or(0));
    let data_at = loc.header_offset + 30 + name_len + extra_len;
    if data_at
        .checked_add(loc.compressed)
        .map_or(true, |e| e > src.size())
    {
        return Err(corrupt("entry data out of range"));
    }
    reader.seek(SeekFrom::Start(data_at)).map_err(from_io)?;
    let raw = reader.take(loc.compressed);
    let stream: Box<dyn Read + Send> = match loc.method {
        0 => Box::new(raw),
        8 => Box::new(flate2::read::DeflateDecoder::new(raw)),
        m => return Err(Error::new(ErrorKind::Unsupported, format!("zip method {m}"))),
    };
    Ok(Box::new(Checked::new(stream, loc)))
}

/// Bounds the output to the declared size and verifies CRC-32 at the end, so
/// a decompression bomb or a corrupt entry cannot pass silently.
struct Checked {
    inner: io::Take<Box<dyn Read + Send>>,
    crc: flate2::Crc,
    expect_crc: u32,
    expect_len: u64,
}

impl Checked {
    fn new(inner: Box<dyn Read + Send>, loc: &ZipLoc) -> Checked {
        Checked {
            inner: inner.take(loc.size),
            crc: flate2::Crc::new(),
            expect_crc: loc.crc,
            expect_len: loc.size,
        }
    }
}

impl Read for Checked {
    fn read(&mut self, out: &mut [u8]) -> io::Result<usize> {
        let n = self.inner.read(out)?;
        if n > 0 {
            self.crc.update(&out[..n]);
            return Ok(n);
        }
        if self.crc.amount() as u64 != self.expect_len || self.crc.sum() != self.expect_crc {
            return Err(io::Error::new(
                io::ErrorKind::InvalidData,
                "zip entry checksum mismatch",
            ));
        }
        Ok(0)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn dos_time_conversion() {
        // 2024-03-05 12:34:56
        let date = ((2024 - 1980) << 9) | (3 << 5) | 5;
        let time = (12 << 11) | (34 << 5) | (56 / 2);
        let t = dos_to_time(date as u16, time as u16);
        let secs = t
            .duration_since(SystemTime::UNIX_EPOCH)
            .map(|d| d.as_secs())
            .unwrap_or(0);
        assert_eq!(secs, 1_709_642_096);
    }

    #[test]
    fn classify_by_unix_mode_and_name() {
        assert_eq!(classify(3 << 8, 0o120_777 << 16, b"l").0, Kind::Symlink);
        assert_eq!(classify(3 << 8, 0o040_755 << 16, b"d").0, Kind::Dir);
        assert_eq!(classify(3 << 8, 0o100_644 << 16, b"f"), (Kind::File, Some(0o644)));
        assert_eq!(classify(0, 0, b"dir/").0, Kind::Dir);
        assert_eq!(classify(0, 0x10, b"dir").0, Kind::Dir);
        assert_eq!(classify(0, 0, b"file").0, Kind::File);
    }

    #[test]
    fn zip64_field_replaces_only_marked_values() {
        let mut data = Vec::new();
        data.extend_from_slice(&5_000_000_000u64.to_le_bytes());
        data.extend_from_slice(&7u64.to_le_bytes());
        let (mut size, mut comp, mut off) = (0xFFFF_FFFF, 10, 0xFFFF_FFFF);
        apply_zip64(&data, &mut size, &mut comp, &mut off);
        assert_eq!((size, comp, off), (5_000_000_000, 10, 7));
    }

    #[test]
    fn extra_field_walker_stops_on_truncation() {
        let extra = [1, 0, 2, 0, 9, 9, 7, 0, 5, 0, 1];
        let fields = extra_fields(&extra);
        assert_eq!(fields.len(), 1);
        assert_eq!(fields[0].0, 1);
    }

    #[test]
    fn garbage_directory_is_rejected_not_panicking() {
        assert!(parse_directory(&[1, 2, 3], 1).is_err());
        let mut b = CENTRAL_SIG.to_le_bytes().to_vec();
        b.extend_from_slice(&[0u8; 60]);
        b[28] = 0xFF; // name longer than the buffer
        assert!(parse_directory(&b, 1).is_err());
    }
}
