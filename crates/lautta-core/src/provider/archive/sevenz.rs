// SPDX-License-Identifier: LGPL-2.1-or-later
//! 7z: indexed and extracted from the downloaded cache file. AES-256 ones open with the password;
//! a wrong one fails at open.

use super::blocks::from_io;
use super::{Loc, RawEntry};
use crate::entry::Kind;
use crate::error::{Error, ErrorKind, Result};
use crate::password::Password;
use sevenz_rust2::{ArchiveReader, EncoderMethod};
use std::fs::File;
use std::io::{self, Write};
use std::time::SystemTime;

/// 7z stores unix mode bits in the high half of the attributes when bit 15
/// (`FILE_ATTRIBUTE_UNIX_EXTENSION`) is set.
const UNIX_EXTENSION: u32 = 0x8000;

fn open(file: File, password: Option<&Password>) -> Result<ArchiveReader<File>> {
    let key = password.map_or_else(sevenz_rust2::Password::empty, |p| p.as_str().into());
    ArchiveReader::new(file, key).map_err(|e| map_err(e, password.is_some()))
}

/// With a password given, data that fails to decode reads as a wrong password: 7z has no check
/// value for it, 7-Zip says the same ("Wrong password?").
fn map_err(err: sevenz_rust2::Error, with_password: bool) -> Error {
    match err {
        sevenz_rust2::Error::PasswordRequired => Error::kind(ErrorKind::PasswordRequired),
        sevenz_rust2::Error::MaybeBadPassword(_) if with_password => Error::kind(ErrorKind::WrongPassword),
        other => Error::new(ErrorKind::ProtocolError, format!("7z: {other}")),
    }
}

fn locked(reader: &ArchiveReader<File>) -> bool {
    let aes = EncoderMethod::AES256_SHA256.id();
    reader
        .archive()
        .blocks
        .iter()
        .any(|b| b.coders.iter().any(|c| c.encoder_method_id() == aes))
}

fn classify(attrs: Option<u32>, is_dir: bool) -> (Kind, Option<u32>) {
    let unix = attrs.filter(|a| a & UNIX_EXTENSION != 0).map(|a| a >> 16);
    match unix {
        Some(mode) => {
            let kind = match mode & 0o170_000 {
                0o120_000 => Kind::Symlink,
                0o040_000 => Kind::Dir,
                0o100_000 | 0 => {
                    if is_dir {
                        Kind::Dir
                    } else {
                        Kind::File
                    }
                }
                _ => Kind::Special,
            };
            (kind, Some(mode & 0o7777))
        }
        None if is_dir => (Kind::Dir, None),
        None => (Kind::File, None),
    }
}

/// Readable names prove no password: decodes the start of the first file.
fn check_password(reader: &mut ArchiveReader<File>) -> Result<()> {
    let mut failed = false;
    let res = reader.for_each_entries(|entry, data| {
        if !entry.has_stream() {
            return Ok(true);
        }
        failed = data.read(&mut [0u8; 4096]).is_err();
        Ok(false)
    });
    if failed || res.is_err() {
        return Err(Error::kind(ErrorKind::WrongPassword));
    }
    Ok(())
}

/// Entries, and whether any data is encrypted. Locked: `PasswordRequired` without `password`,
/// `WrongPassword` with a wrong one.
pub fn index(file: File, password: Option<&Password>) -> Result<(Vec<RawEntry>, bool)> {
    let mut reader = open(file, password)?;
    let locked = locked(&reader);
    if locked {
        if password.is_none() {
            return Err(Error::kind(ErrorKind::PasswordRequired));
        }
        check_password(&mut reader)?;
    }
    let mut out = Vec::new();
    for e in &reader.archive().files {
        if e.is_anti_item() {
            continue;
        }
        let attrs = e.has_windows_attributes.then_some(e.windows_attributes);
        let (kind, mode) = classify(attrs, e.is_directory());
        let mtime = e
            .has_last_modified_date
            .then(|| SystemTime::from(e.last_modified_date()));
        out.push(RawEntry {
            name: e.name().as_bytes().to_vec(),
            kind,
            size: if kind == Kind::Dir { 0 } else { e.size() },
            mtime,
            mode,
            link: None,
            loc: Loc::SevenZ(e.name().to_owned()),
        });
    }
    Ok((out, locked))
}

/// Solid archives decode the preceding entries of the block too: inherent to 7z.
pub fn copy_entry(file: File, name: &str, out: &mut dyn Write, password: Option<&Password>) -> Result<()> {
    let mut reader = open(file, password)?;
    let mut found = false;
    let mut failure: Option<io::Error> = None;
    let res = reader.for_each_entries(|entry, data| {
        if entry.name() != name {
            return Ok(true);
        }
        found = true;
        if let Err(e) = io::copy(data, out) {
            failure = Some(e);
        }
        Ok(false)
    });
    if let Some(e) = failure {
        if password.is_some() && e.kind() == io::ErrorKind::InvalidData {
            return Err(Error::kind(ErrorKind::WrongPassword));
        }
        return Err(from_io(e));
    }
    res.map_err(|e| map_err(e, password.is_some()))?;
    if found {
        Ok(())
    } else {
        Err(Error::new(ErrorKind::NotFound, "entry vanished from archive"))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn classify_uses_unix_bits_when_present() {
        let sym = (0o120_777 << 16) | UNIX_EXTENSION;
        assert_eq!(classify(Some(sym), false).0, Kind::Symlink);
        let reg = (0o100_600 << 16) | UNIX_EXTENSION;
        assert_eq!(classify(Some(reg), false), (Kind::File, Some(0o600)));
        assert_eq!(classify(Some(0x10), true), (Kind::Dir, None));
        assert_eq!(classify(None, false), (Kind::File, None));
    }
}
