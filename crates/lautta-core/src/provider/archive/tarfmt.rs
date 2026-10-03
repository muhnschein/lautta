// SPDX-License-Identifier: LGPL-2.1-or-later
//! tar and compressed tar (PRV-10). Plain tar is indexed through the block
//! cache (headers only, data skipped by seeking); compressed tar is read
//! sequentially from the downloaded cache file.

use super::blocks::{from_io, BlockReader};
use super::{Compression, Loc, RawEntry};
use crate::entry::Kind;
use crate::error::{Error, ErrorKind, Result};
use std::fs::File;
use std::io::{self, BufReader, Read, Write};
use std::time::{Duration, SystemTime};

/// Decompressing reader over `input`.
pub fn decoder<'a, R: Read + 'a>(comp: Compression, input: R) -> io::Result<Box<dyn Read + 'a>> {
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

/// Indexes a plain tar through ranged reads: headers only.
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

/// Indexes a (decompressed) tar stream; entries are located by sequence
/// number, which counts every entry the archive yields.
pub fn index_stream(input: impl Read) -> Result<Vec<RawEntry>> {
    let mut archive = tar::Archive::new(input);
    let mut out = Vec::new();
    for (seq, entry) in archive.entries().map_err(tar_err)?.enumerate() {
        let entry = entry.map_err(tar_err)?;
        out.extend(raw_entry(&entry, Loc::TarSeq(seq)));
    }
    Ok(out)
}

/// Copies entry number `seq` of a compressed tar into `out`.
pub fn copy_seq(file: File, comp: Compression, seq: usize, out: &mut dyn Write) -> Result<()> {
    let input = decoder(comp, BufReader::new(file)).map_err(tar_err)?;
    let mut archive = tar::Archive::new(input);
    for (i, entry) in archive.entries().map_err(tar_err)?.enumerate() {
        if i == seq {
            let mut entry = entry.map_err(tar_err)?;
            io::copy(&mut entry, out).map_err(tar_err)?;
            return Ok(());
        }
    }
    Err(Error::new(ErrorKind::NotFound, "entry vanished from archive"))
}

/// Reads up to `limit` decompressed bytes from the start of `head`; used to
/// recognise compressed tar archives by their `ustar` magic.
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
    fn stream_index_has_links_and_sequence() {
        let entries = index_stream(tar_bytes().as_slice()).unwrap();
        assert_eq!(entries.len(), 2);
        assert_eq!(entries[0].kind, Kind::File);
        assert_eq!(entries[0].size, 5);
        assert_eq!(entries[0].mode, Some(0o640));
        assert!(matches!(entries[0].loc, Loc::TarSeq(0)));
        assert_eq!(entries[1].kind, Kind::Symlink);
        assert_eq!(entries[1].link.as_deref(), Some(&b"dir/a.txt"[..]));
        assert!(matches!(entries[1].loc, Loc::TarSeq(1)));
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
}
