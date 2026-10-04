// SPDX-License-Identifier: LGPL-2.1-or-later
//! A zip writer that needs only `Write` (no `Seek`), so the archive can go
//! straight into a pipe. Files use data descriptors (sizes and CRC follow the
//! data), deflate, UTF-8 names, unix modes and extended timestamps; entries
//! switch to zip64 when their size is unknown or ≥ 4 GiB.

use super::{EntryMeta, MemberWriter};
use crate::entry::Kind;
use std::io::{self, Read, Write};
use std::time::{Duration, SystemTime};

const LOCAL_SIG: u32 = 0x0403_4b50;
const DESCRIPTOR_SIG: u32 = 0x0807_4b50;
const CENTRAL_SIG: u32 = 0x0201_4b50;
const EOCD_SIG: u32 = 0x0605_4b50;
const EOCD64_SIG: u32 = 0x0606_4b50;
const LOCATOR64_SIG: u32 = 0x0706_4b50;
const FLAG_DESCRIPTOR: u16 = 0x0008;
const FLAG_UTF8: u16 = 0x0800;
const METHOD_STORED: u16 = 0;
const METHOD_DEFLATE: u16 = 8;
const U32_MAX: u64 = 0xFFFF_FFFF;
const VERSION_MADE_BY: u16 = (3 << 8) | 63;

struct Counting<W: Write> {
    inner: W,
    count: u64,
}

impl<W: Write> Write for Counting<W> {
    fn write(&mut self, buf: &[u8]) -> io::Result<usize> {
        let n = self.inner.write(buf)?;
        self.count += n as u64;
        Ok(n)
    }

    fn flush(&mut self) -> io::Result<()> {
        self.inner.flush()
    }
}

/// What the central directory needs about a finished member.
struct Member {
    name: Vec<u8>,
    flags: u16,
    method: u16,
    dos: (u16, u16),
    unix_mtime: Option<u32>,
    crc: u32,
    compressed: u64,
    size: u64,
    offset: u64,
    external_attrs: u32,
    local_zip64: bool,
}

pub(crate) struct ZipStream<W: Write> {
    out: Counting<W>,
    level: u32,
    members: Vec<Member>,
    /// Test knob: write every file entry as zip64.
    force_zip64: bool,
}

impl<W: Write> ZipStream<W> {
    pub fn new(sink: W, level: u32) -> ZipStream<W> {
        ZipStream {
            out: Counting {
                inner: sink,
                count: 0,
            },
            level,
            members: Vec::new(),
            force_zip64: false,
        }
    }

    #[cfg(test)]
    pub fn with_forced_zip64(mut self) -> ZipStream<W> {
        self.force_zip64 = true;
        self
    }

    fn put16(&mut self, v: u16) -> io::Result<()> {
        self.out.write_all(&v.to_le_bytes())
    }

    fn put32(&mut self, v: u32) -> io::Result<()> {
        self.out.write_all(&v.to_le_bytes())
    }

    fn put64(&mut self, v: u64) -> io::Result<()> {
        self.out.write_all(&v.to_le_bytes())
    }
}

/// Seconds since the epoch to a DOS date/time (UTC, clamped to 1980..2107).
pub(crate) fn dos_datetime(t: SystemTime) -> (u16, u16) {
    let secs = t
        .duration_since(SystemTime::UNIX_EPOCH)
        .map_or(0, |d| d.as_secs());
    let days = i64::try_from(secs / 86_400).unwrap_or(0);
    let rem = secs % 86_400;
    let (y, m, d) = civil_from_days(days);
    if y < 1980 {
        return ((1 << 5) | 1, 0);
    }
    let year = u16::try_from((y - 1980).min(127)).unwrap_or(0);
    let date = (year << 9) | ((m as u16) << 5) | d as u16;
    let time = (((rem / 3600) as u16) << 11) | ((((rem % 3600) / 60) as u16) << 5) | ((rem % 60) / 2) as u16;
    (date, time)
}

/// Days since 1970-01-01 to (year, month, day).
fn civil_from_days(days: i64) -> (i64, u32, u32) {
    let z = days + 719_468;
    let era = z.div_euclid(146_097);
    let doe = z.rem_euclid(146_097);
    let yoe = (doe - doe / 1460 + doe / 36_524 - doe / 146_096) / 365;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = (doy - (153 * mp + 2) / 5 + 1) as u32;
    let m = if mp < 10 { mp + 3 } else { mp - 9 } as u32;
    let y = yoe + era * 400 + i64::from(m <= 2);
    (y, m, d)
}

fn unix_secs(t: Option<SystemTime>) -> Option<u32> {
    let secs = t?.duration_since(SystemTime::UNIX_EPOCH).ok()?.as_secs();
    u32::try_from(secs).ok()
}

fn external_attrs(kind: Kind, mode: Option<u32>) -> u32 {
    let (type_bits, default_mode, dos) = match kind {
        Kind::Dir => (0o040_000, 0o755, 0x10),
        Kind::Symlink => (0o120_000, 0o777, 0),
        _ => (0o100_000, 0o644, 0),
    };
    ((type_bits | (mode.unwrap_or(default_mode) & 0o7777)) << 16) | dos
}

/// Extended timestamp extra field (modification time only).
fn timestamp_extra(mtime: Option<u32>) -> Vec<u8> {
    let Some(secs) = mtime else {
        return Vec::new();
    };
    let mut v = vec![0x55, 0x54, 5, 0, 1];
    v.extend_from_slice(&secs.to_le_bytes());
    v
}

fn io_invalid(msg: &str) -> io::Error {
    io::Error::new(io::ErrorKind::InvalidData, msg.to_owned())
}

impl<W: Write> ZipStream<W> {
    fn local_header(&mut self, m: &Member, descriptor: bool) -> io::Result<()> {
        let mut extra = timestamp_extra(m.unix_mtime);
        if m.local_zip64 {
            extra.extend_from_slice(&[1, 0, 16, 0]);
            extra.extend_from_slice(&[0u8; 16]);
        }
        // With a zip64 extra the fixed size fields must read 0xFFFFFFFF.
        let (crc, csize, usize_) = match (descriptor, m.local_zip64) {
            (false, _) => (m.crc, m.compressed as u32, m.size as u32),
            (true, false) => (0, 0, 0),
            (true, true) => (0, U32_MAX as u32, U32_MAX as u32),
        };
        self.put32(LOCAL_SIG)?;
        self.put16(if m.local_zip64 { 45 } else { 20 })?;
        self.put16(m.flags)?;
        self.put16(m.method)?;
        self.put16(m.dos.1)?;
        self.put16(m.dos.0)?;
        self.put32(crc)?;
        self.put32(csize)?;
        self.put32(usize_)?;
        self.put16(u16::try_from(m.name.len()).map_err(|_| io_invalid("name too long"))?)?;
        self.put16(extra.len() as u16)?;
        self.out.write_all(&m.name)?;
        self.out.write_all(&extra)
    }

    fn new_member(&mut self, meta: &EntryMeta, name: Vec<u8>, method: u16, descriptor: bool) -> Member {
        let mut flags = if std::str::from_utf8(&name).is_ok() {
            FLAG_UTF8
        } else {
            0
        };
        if descriptor {
            flags |= FLAG_DESCRIPTOR;
        }
        let large = meta.size.map_or(true, |s| s >= U32_MAX);
        let mtime = meta
            .mtime
            .unwrap_or(SystemTime::UNIX_EPOCH + Duration::from_secs(0));
        Member {
            name,
            flags,
            method,
            dos: dos_datetime(mtime),
            unix_mtime: unix_secs(meta.mtime),
            crc: 0,
            compressed: 0,
            size: 0,
            offset: self.out.count,
            external_attrs: external_attrs(meta.kind, meta.mode),
            local_zip64: descriptor && (large || self.force_zip64),
        }
    }

    /// Folders and links: no deflate, sizes known up front.
    fn small_member(&mut self, meta: &EntryMeta, name: Vec<u8>, body: &[u8]) -> io::Result<()> {
        let mut m = self.new_member(meta, name, METHOD_STORED, false);
        let mut crc = flate2::Crc::new();
        crc.update(body);
        m.crc = crc.sum();
        m.size = body.len() as u64;
        m.compressed = body.len() as u64;
        self.local_header(&m, false)?;
        self.out.write_all(body)?;
        self.members.push(m);
        Ok(())
    }

    fn file_member(&mut self, meta: &EntryMeta, data: &mut dyn Read) -> io::Result<()> {
        let mut m = self.new_member(meta, meta.path.clone(), METHOD_DEFLATE, true);
        self.local_header(&m, true)?;
        let start = self.out.count;
        let mut enc = flate2::write::DeflateEncoder::new(&mut self.out, flate2::Compression::new(self.level));
        let mut crc = flate2::Crc::new();
        let mut buf = vec![0u8; 64 * 1024];
        loop {
            let n = data.read(&mut buf)?;
            if n == 0 {
                break;
            }
            crc.update(&buf[..n]);
            enc.write_all(&buf[..n])?;
        }
        enc.finish()?;
        m.crc = crc.sum();
        m.size = u64::from(crc.amount());
        m.compressed = self.out.count - start;
        if !m.local_zip64 && (m.size >= U32_MAX || m.compressed >= U32_MAX) {
            return Err(io_invalid("file grew past 4 GiB while being compressed"));
        }
        self.descriptor(&m)?;
        self.members.push(m);
        Ok(())
    }

    fn descriptor(&mut self, m: &Member) -> io::Result<()> {
        self.put32(DESCRIPTOR_SIG)?;
        self.put32(m.crc)?;
        if m.local_zip64 {
            self.put64(m.compressed)?;
            self.put64(m.size)
        } else {
            self.put32(m.compressed as u32)?;
            self.put32(m.size as u32)
        }
    }

    fn central_entry(&mut self, m: &Member) -> io::Result<()> {
        let big = |v: u64| v >= U32_MAX;
        let mut zip64 = Vec::new();
        for v in [m.size, m.compressed, m.offset] {
            if big(v) {
                zip64.extend_from_slice(&v.to_le_bytes());
            }
        }
        let mut extra = timestamp_extra(m.unix_mtime);
        if !zip64.is_empty() {
            extra.extend_from_slice(&[1, 0]);
            extra.extend_from_slice(&(zip64.len() as u16).to_le_bytes());
            extra.extend_from_slice(&zip64);
        }
        let clamp = |v: u64| if big(v) { U32_MAX as u32 } else { v as u32 };
        self.put32(CENTRAL_SIG)?;
        self.put16(VERSION_MADE_BY)?;
        self.put16(if m.local_zip64 || !zip64.is_empty() {
            45
        } else {
            20
        })?;
        self.put16(m.flags)?;
        self.put16(m.method)?;
        self.put16(m.dos.1)?;
        self.put16(m.dos.0)?;
        self.put32(m.crc)?;
        self.put32(clamp(m.compressed))?;
        self.put32(clamp(m.size))?;
        self.put16(m.name.len() as u16)?;
        self.put16(extra.len() as u16)?;
        self.put16(0)?;
        self.put16(0)?;
        self.put16(0)?;
        self.put32(m.external_attrs)?;
        self.put32(clamp(m.offset))?;
        self.out.write_all(&m.name)?;
        self.out.write_all(&extra)
    }

    fn end_of_directory(&mut self, start: u64, size: u64) -> io::Result<()> {
        let count = self.members.len() as u64;
        let needs64 = count >= 0xFFFF || start >= U32_MAX || size >= U32_MAX;
        if needs64 {
            let record_at = self.out.count;
            self.put32(EOCD64_SIG)?;
            self.put64(44)?;
            self.put16(VERSION_MADE_BY)?;
            self.put16(45)?;
            self.put32(0)?;
            self.put32(0)?;
            self.put64(count)?;
            self.put64(count)?;
            self.put64(size)?;
            self.put64(start)?;
            self.put32(LOCATOR64_SIG)?;
            self.put32(0)?;
            self.put64(record_at)?;
            self.put32(1)?;
        }
        self.put32(EOCD_SIG)?;
        self.put16(0)?;
        self.put16(0)?;
        let n16 = if needs64 { 0xFFFF } else { count as u16 };
        self.put16(n16)?;
        self.put16(n16)?;
        self.put32(if needs64 { U32_MAX as u32 } else { size as u32 })?;
        self.put32(if needs64 { U32_MAX as u32 } else { start as u32 })?;
        self.put16(0)
    }
}

impl<W: Write + Send> MemberWriter for ZipStream<W> {
    fn member(&mut self, meta: &EntryMeta, data: &mut dyn Read) -> io::Result<()> {
        match meta.kind {
            Kind::Dir => {
                let mut name = meta.path.clone();
                name.push(b'/');
                self.small_member(meta, name, &[])
            }
            Kind::Symlink => {
                let target = meta.link.clone().unwrap_or_default();
                self.small_member(meta, meta.path.clone(), &target)
            }
            _ => self.file_member(meta, data),
        }
    }

    fn finish(mut self: Box<Self>) -> io::Result<()> {
        let start = self.out.count;
        let members = std::mem::take(&mut self.members);
        for m in &members {
            self.central_entry(m)?;
        }
        let size = self.out.count - start;
        self.members = members;
        self.end_of_directory(start, size)?;
        self.out.flush()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn dos_dates_match_known_instants() {
        let t = SystemTime::UNIX_EPOCH + Duration::from_secs(1_709_642_096); // 2024-03-05 12:34:56 UTC
        let (date, time) = dos_datetime(t);
        assert_eq!((date >> 9) + 1980, 2024);
        assert_eq!((date >> 5) & 0xF, 3);
        assert_eq!(date & 0x1F, 5);
        assert_eq!((time >> 11, (time >> 5) & 0x3F, (time & 0x1F) * 2), (12, 34, 56));
        assert_eq!(
            dos_datetime(SystemTime::UNIX_EPOCH),
            ((1 << 5) | 1, 0),
            "before 1980 clamps"
        );
        let far = SystemTime::UNIX_EPOCH + Duration::from_secs(5_000_000_000);
        assert_eq!(dos_datetime(far).0 >> 9, 127, "after 2107 clamps");
    }

    #[test]
    fn civil_dates() {
        assert_eq!(civil_from_days(0), (1970, 1, 1));
        assert_eq!(civil_from_days(19_787), (2024, 3, 5));
        assert_eq!(civil_from_days(11_016), (2000, 2, 29));
    }

    #[test]
    fn external_attributes_carry_type_and_mode() {
        assert_eq!(external_attrs(Kind::File, Some(0o600)) >> 16, 0o100_600);
        assert_eq!(external_attrs(Kind::Dir, None) >> 16, 0o040_755);
        assert_eq!(external_attrs(Kind::Dir, None) & 0xFF, 0x10);
        assert_eq!(external_attrs(Kind::Symlink, None) >> 16, 0o120_777);
    }
}
