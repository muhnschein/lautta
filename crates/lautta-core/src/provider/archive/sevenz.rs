// SPDX-License-Identifier: LGPL-2.1-or-later
//! 7z (PRV-10): indexed and extracted from the downloaded cache file.

use super::blocks::from_io;
use super::{Loc, RawEntry};
use crate::entry::Kind;
use crate::error::{Error, ErrorKind, Result};
use sevenz_rust::{Password, SevenZReader};
use std::fs::File;
use std::io::{self, Write};
use std::time::SystemTime;

/// 7z stores unix mode bits in the high half of the attributes when bit 15
/// (`FILE_ATTRIBUTE_UNIX_EXTENSION`) is set.
const UNIX_EXTENSION: u32 = 0x8000;

fn open(file: File) -> Result<SevenZReader<File>> {
    let len = file.metadata().map_err(from_io)?.len();
    SevenZReader::new(file, len, Password::empty()).map_err(map_err)
}

fn map_err(err: sevenz_rust::Error) -> Error {
    Error::new(ErrorKind::ProtocolError, format!("7z: {err}"))
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

/// Lists the archive.
pub fn index(file: File) -> Result<Vec<RawEntry>> {
    let reader = open(file)?;
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
    Ok(out)
}

/// Streams the entry called `name` into `out`. Solid archives decode the
/// preceding entries of the block as well; that is inherent to 7z.
pub fn copy_entry(file: File, name: &str, out: &mut dyn Write) -> Result<()> {
    let mut reader = open(file)?;
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
        return Err(from_io(e));
    }
    res.map_err(map_err)?;
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
