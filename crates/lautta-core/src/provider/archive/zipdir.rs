// SPDX-License-Identifier: LGPL-2.1-or-later

use super::blocks::{from_io, BlockReader, BlockSource};
use super::{Loc, RawEntry};
use crate::entry::Kind;
use crate::error::{Error, ErrorKind, Result};
use crate::password::Password;
use crate::zipaes::{self, AesField, Decryptor};
use crate::zipcrypto;
use chrono::{DateTime, Local, Offset, TimeZone};
use std::io::{self, Read, Seek, SeekFrom};
use std::sync::{Arc, Mutex, PoisonError};
use std::time::{Duration, SystemTime};

const EOCD_SIG: [u8; 4] = [0x50, 0x4b, 0x05, 0x06];
const EOCD_LEN: usize = 22;
const MAX_COMMENT: u64 = 65_535;
const ZIP64_LOCATOR_SIG: u32 = 0x0706_4b50;
const ZIP64_EOCD_SIG: u32 = 0x0606_4b50;
const CENTRAL_SIG: u32 = 0x0201_4b50;
const LOCAL_SIG: u32 = 0x0403_4b50;
const CENTRAL_FIXED: usize = 46;
const MAX_DIRECTORY: u64 = 256 * 1024 * 1024;
const FULL_CHECK_MAX: u64 = 1024 * 1024;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ZipLoc {
    pub method: u16,
    pub flags: u16,
    pub crc: u32,
    pub compressed: u64,
    pub size: u64,
    pub header_offset: u64,
    pub aes: Option<AesField>,
}

const FLAG_ENCRYPTED: u16 = 0x0001;
const FLAG_DESCRIPTOR: u16 = 0x0008;
const FLAG_STRONG: u16 = 0x0040;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Lock {
    Open,
    Aes(AesField),
    ZipCrypto,
    Strong,
}

impl ZipLoc {
    pub fn aes(&self) -> Option<AesField> {
        self.aes
            .filter(|_| self.flags & FLAG_ENCRYPTED != 0 && self.method == zipaes::METHOD)
    }

    pub fn lock(&self) -> Lock {
        if self.flags & FLAG_ENCRYPTED == 0 {
            Lock::Open
        } else if let Some(aes) = self.aes() {
            Lock::Aes(aes)
        } else if self.flags & FLAG_STRONG != 0 {
            Lock::Strong
        } else {
            Lock::ZipCrypto
        }
    }

    fn zipcrypto_check(&self, local_time: u16) -> u8 {
        if self.flags & FLAG_DESCRIPTOR != 0 {
            local_time.to_be_bytes()[0]
        } else {
            self.crc.to_be_bytes()[0]
        }
    }
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

struct Directory {
    offset: u64,
    size: u64,
    count: u64,
}

pub fn read_entries(reader: &mut BlockReader) -> Result<Vec<RawEntry>> {
    let dir = locate_directory(reader)?;
    if dir.size > MAX_DIRECTORY || dir.offset.checked_add(dir.size).is_none_or(|e| e > reader.size()) {
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
    let mut aes = None;
    for (id, data) in extra_fields(extra) {
        match id {
            0x0001 => apply_zip64(data, &mut size, &mut compressed, &mut header_offset),
            0x5455 => mtime = unix_mtime(data).or(mtime),
            zipaes::EXTRA_ID => aes = AesField::parse(data),
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
        aes,
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

pub fn dos_to_time(date: u16, time: u16) -> SystemTime {
    dos_to_time_in(&Local, date, time)
}

fn dos_to_time_in<Tz: TimeZone>(tz: &Tz, date: u16, time: u16) -> SystemTime {
    let year = 1980 + i64::from(date >> 9);
    let month = i64::from((date >> 5) & 0xF).clamp(1, 12);
    let day = i64::from(date & 0x1F).max(1);
    let wall = days_from_civil(year, month, day) * 86_400
        + i64::from(time >> 11) * 3600
        + i64::from((time >> 5) & 0x3F) * 60
        + i64::from(time & 0x1F) * 2;
    let offset = DateTime::from_timestamp(wall, 0).map_or(0, |t| {
        tz.offset_from_utc_datetime(&t.naive_utc())
            .fix()
            .local_minus_utc()
    });
    let secs = wall - i64::from(offset);
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

fn stored_data(src: &Arc<BlockSource>, loc: &ZipLoc) -> Result<(io::Take<BlockReader>, u16)> {
    let mut reader = src.reader();
    let head = reader.read_exact_at(loc.header_offset, 30).map_err(from_io)?;
    if le32(&head, 0) != Some(LOCAL_SIG) {
        return Err(corrupt("bad local header"));
    }
    let local_time = le16(&head, 10).unwrap_or(0);
    let name_len = u64::from(le16(&head, 26).unwrap_or(0));
    let extra_len = u64::from(le16(&head, 28).unwrap_or(0));
    let data_at = loc.header_offset + 30 + name_len + extra_len;
    if data_at.checked_add(loc.compressed).is_none_or(|e| e > src.size()) {
        return Err(corrupt("entry data out of range"));
    }
    reader.seek(SeekFrom::Start(data_at)).map_err(from_io)?;
    Ok((reader.take(loc.compressed), local_time))
}

fn needs_password(password: Option<&Password>) -> Result<&Password> {
    password.ok_or_else(|| Error::kind(ErrorKind::PasswordRequired))
}

pub fn check_password(
    src: &Arc<BlockSource>,
    entries: &[RawEntry],
    password: Option<&Password>,
) -> Result<bool> {
    let zips = || {
        entries.iter().filter_map(|e| match &e.loc {
            Loc::Zip(z) => Some(z),
            _ => None,
        })
    };
    if let Some((loc, aes)) = zips().find_map(|z| z.aes().map(|aes| (z, aes))) {
        Decryptor::new(
            stored_data(src, loc)?.0,
            loc.compressed,
            needs_password(password)?,
            aes.strength,
        )?;
        return Ok(true);
    }
    let Some(loc) = zips()
        .filter(|z| z.lock() == Lock::ZipCrypto)
        .min_by_key(|z| (!decodable(z.method), z.compressed))
    else {
        return Ok(false);
    };
    let (raw, local_time) = stored_data(src, loc)?;
    let dec = zipcrypto::Decryptor::new(raw, needs_password(password)?, loc.zipcrypto_check(local_time))?;
    if loc.compressed > FULL_CHECK_MAX || !decodable(loc.method) {
        return Ok(true);
    }
    let mut entry = Checked::new(decode(loc.method, Box::new(dec))?, loc, None);
    match io::copy(&mut entry, &mut io::sink()) {
        Ok(_) => Ok(true),
        Err(e) if matches!(e.kind(), io::ErrorKind::InvalidData | io::ErrorKind::InvalidInput) => {
            Err(Error::kind(ErrorKind::WrongPassword))
        }
        Err(e) => Err(from_io(e)),
    }
}

fn decodable(method: u16) -> bool {
    matches!(method, 0 | 8)
}

fn decode(method: u16, data: Box<dyn Read + Send>) -> Result<Box<dyn Read + Send>> {
    match method {
        0 => Ok(data),
        8 => Ok(Box::new(flate2::read::DeflateDecoder::new(data))),
        m => Err(Error::new(ErrorKind::Unsupported, format!("zip method {m}"))),
    }
}

struct Shared<R: Read>(Arc<Mutex<Decryptor<R>>>);

impl<R: Read> Read for Shared<R> {
    fn read(&mut self, out: &mut [u8]) -> io::Result<usize> {
        self.0.lock().unwrap_or_else(PoisonError::into_inner).read(out)
    }
}

type Finish = Box<dyn FnMut() -> io::Result<()> + Send>;

pub fn open_entry(
    src: &Arc<BlockSource>,
    loc: &ZipLoc,
    password: Option<&Password>,
) -> Result<Box<dyn Read + Send>> {
    let (raw, local_time) = stored_data(src, loc)?;
    let (method, data, finish): (u16, Box<dyn Read + Send>, Option<Finish>) = match loc.lock() {
        Lock::Aes(aes) => {
            let dec = Decryptor::new(raw, loc.compressed, needs_password(password)?, aes.strength)?;
            let dec = Arc::new(Mutex::new(dec));
            let tail = Arc::clone(&dec);
            let finish: Finish =
                Box::new(move || tail.lock().unwrap_or_else(PoisonError::into_inner).finish());
            (aes.method, Box::new(Shared(dec)), Some(finish))
        }
        Lock::ZipCrypto => {
            let check = loc.zipcrypto_check(local_time);
            let dec = zipcrypto::Decryptor::new(raw, needs_password(password)?, check)?;
            (loc.method, Box::new(dec), None)
        }
        Lock::Strong => {
            return Err(Error::new(
                ErrorKind::Unsupported,
                "zip entry uses PKWARE strong encryption",
            ))
        }
        Lock::Open => (loc.method, Box::new(raw), None),
    };
    Ok(Box::new(Checked::new(decode(method, data)?, loc, finish)))
}

struct Checked {
    inner: io::Take<Box<dyn Read + Send>>,
    crc: flate2::Crc,
    len: u64,
    expect_crc: Option<u32>,
    expect_len: u64,
    finish: Option<Finish>,
}

impl Checked {
    fn new(inner: Box<dyn Read + Send>, loc: &ZipLoc, finish: Option<Finish>) -> Checked {
        Checked {
            inner: inner.take(loc.size),
            crc: flate2::Crc::new(),
            len: 0,
            expect_crc: loc.aes().is_none_or(|a| a.has_crc()).then_some(loc.crc),
            expect_len: loc.size,
            finish,
        }
    }
}

impl Read for Checked {
    fn read(&mut self, out: &mut [u8]) -> io::Result<usize> {
        let n = self.inner.read(out)?;
        if n > 0 {
            self.crc.update(&out[..n]);
            self.len += n as u64;
            return Ok(n);
        }
        let more = self.inner.limit() == 0 && self.inner.get_mut().read(&mut [0u8; 1])? > 0;
        if let Some(finish) = self.finish.as_mut() {
            finish()?;
        }
        let crc_ok = self.expect_crc.is_none_or(|c| c == self.crc.sum());
        if more || self.len != self.expect_len || !crc_ok {
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
    use chrono::FixedOffset;

    #[test]
    fn dos_time_conversion() {
        let date = ((2024 - 1980) << 9) | (3 << 5) | 5;
        let time = (12 << 11) | (34 << 5) | (56 / 2);
        let secs = |tz: &FixedOffset| {
            dos_to_time_in(tz, date as u16, time as u16)
                .duration_since(SystemTime::UNIX_EPOCH)
                .map(|d| d.as_secs())
                .unwrap_or(0)
        };
        assert_eq!(secs(&FixedOffset::east_opt(0).unwrap()), 1_709_642_096);
        assert_eq!(secs(&FixedOffset::east_opt(3600).unwrap()), 1_709_642_096 - 3600);
        assert_eq!(
            secs(&FixedOffset::west_opt(5 * 3600).unwrap()),
            1_709_642_096 + 5 * 3600
        );
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
    fn entries_past_4_gib_pass_the_size_check() {
        const LEN: u64 = (1 << 32) + 3;
        let mib = vec![0u8; 1 << 20];
        let mut crc = crc32fast::Hasher::new();
        let mut chunk = crc32fast::Hasher::new();
        chunk.update(&mib);
        for _ in 0..LEN >> 20 {
            crc.combine(&chunk);
        }
        crc.update(&mib[..3]);
        let loc = ZipLoc {
            method: 0,
            flags: 0,
            crc: crc.finalize(),
            compressed: LEN,
            size: LEN,
            header_offset: 0,
            aes: None,
        };
        let mut r = Checked::new(Box::new(io::repeat(0).take(LEN)), &loc, None);
        let mut buf = vec![0u8; 1 << 20];
        let mut total = 0u64;
        loop {
            match r.read(&mut buf) {
                Ok(0) => break,
                Ok(n) => total += n as u64,
                Err(e) => panic!("after {total} bytes: {e}"),
            }
        }
        assert_eq!(total, LEN);
    }

    #[test]
    fn garbage_directory_is_rejected_not_panicking() {
        assert!(parse_directory(&[1, 2, 3], 1).is_err());
        let mut b = CENTRAL_SIG.to_le_bytes().to_vec();
        b.extend_from_slice(&[0u8; 60]);
        b[28] = 0xFF;
        assert!(parse_directory(&b, 1).is_err());
    }
}
