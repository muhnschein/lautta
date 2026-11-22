// SPDX-License-Identifier: LGPL-2.1-or-later
//! Checksum lists: GNU `sha256sum` lines, BSD tags (`SHA256 (name) = hex`), one-hash files
//! (`x.iso.sha256`), SFV, PGP clear-signed lists (signature not checked). Content is untrusted:
//! size and entry caps, names stay bytes, unreadable lines are skipped.

use super::Algorithm;
use crate::error::{Error, ErrorKind, Result};

/// Larger files are not read as lists.
pub const MAX_BYTES: u64 = 1 << 20;
pub const MAX_ENTRIES: usize = 10_000;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ListEntry {
    /// As written: may hold `/`, `..` or be absolute; callers resolve it safely.
    pub name: Vec<u8>,
    pub algorithm: Algorithm,
    pub digest: Vec<u8>,
}

/// Lowercase extensions and `*sums` stems that name an algorithm.
const HINTS: [(&str, Algorithm); 24] = [
    ("md5", Algorithm::Md5),
    ("md5sum", Algorithm::Md5),
    ("md5sums", Algorithm::Md5),
    ("sha1", Algorithm::Sha1),
    ("sha1sum", Algorithm::Sha1),
    ("sha1sums", Algorithm::Sha1),
    ("sha224", Algorithm::Sha224),
    ("sha224sums", Algorithm::Sha224),
    ("sha256", Algorithm::Sha256),
    ("sha256sum", Algorithm::Sha256),
    ("sha256sums", Algorithm::Sha256),
    ("sha384", Algorithm::Sha384),
    ("sha384sums", Algorithm::Sha384),
    ("sha512", Algorithm::Sha512),
    ("sha512sum", Algorithm::Sha512),
    ("sha512sums", Algorithm::Sha512),
    ("b2", Algorithm::Blake2b),
    ("b2sums", Algorithm::Blake2b),
    ("b3", Algorithm::Blake3),
    ("b3sums", Algorithm::Blake3),
    ("xxh64", Algorithm::Xxh64),
    ("xxh128", Algorithm::Xxh128),
    ("sfv", Algorithm::Crc32),
    ("crc32", Algorithm::Crc32),
];

/// Names that are lists without naming an algorithm.
const GENERIC: [&str; 3] = ["checksums", "checksum", "sums"];

/// What a file name says about a list: `None` = not a list; `Some(None)` = list, algorithm from
/// its lines.
pub fn list_kind(file_name: &[u8]) -> Option<Option<Algorithm>> {
    let lower = String::from_utf8_lossy(file_name).to_lowercase();
    let mut name = lower.as_str();
    for wrapper in [".asc", ".txt"] {
        name = name.strip_suffix(wrapper).unwrap_or(name);
    }
    let (stem, ext) = name.rsplit_once('.').unwrap_or((name, ""));
    let hint = |key: &str| HINTS.iter().find(|(k, _)| *k == key).map(|(_, a)| *a);
    if let Some(a) = hint(ext).or_else(|| hint(stem).filter(|_| ext.is_empty() && stem.ends_with("sums"))) {
        return Some(Some(a));
    }
    let generic = |s: &str| GENERIC.iter().any(|g| s == *g || s.ends_with(&format!("-{g}")));
    generic(name).then_some(None)
}

/// Parses `data` read from list `file_name`. Fails on more than [`MAX_BYTES`] or
/// [`MAX_ENTRIES`].
pub fn parse(file_name: &[u8], data: &[u8]) -> Result<Vec<ListEntry>> {
    if data.len() as u64 > MAX_BYTES {
        return Err(Error::new(ErrorKind::Unsupported, "checksum list too large"));
    }
    let hint = list_kind(file_name).flatten();
    let sfv = hint == Some(Algorithm::Crc32) && file_name.to_ascii_lowercase().ends_with(b".sfv");
    let mut out = Vec::new();
    for line in body_lines(data) {
        let parsed = if sfv {
            parse_sfv(line)
        } else {
            parse_tag(line).or_else(|| parse_gnu(line, hint))
        };
        if let Some(entry) = parsed {
            if out.len() == MAX_ENTRIES {
                return Err(Error::new(ErrorKind::Unsupported, "checksum list too long"));
            }
            out.push(entry);
        }
    }
    if out.is_empty() {
        if let Some(entry) = single_hash(file_name, data, hint) {
            out.push(entry);
        }
    }
    Ok(out)
}

/// Lines of the list; of a PGP clear-signed one only the signed text, dash-escapes undone.
fn body_lines(data: &[u8]) -> Vec<&[u8]> {
    let lines = data
        .split(|b| *b == b'\n')
        .map(|l| l.strip_suffix(b"\r").unwrap_or(l));
    let mut out = Vec::new();
    let mut state = 0u8; // 0 plain, 1 armour header, 2 signed text
    for line in lines {
        match state {
            0 if line == b"-----BEGIN PGP SIGNED MESSAGE-----" => state = 1,
            1 if line.is_empty() => state = 2,
            1 => {}
            2 if line.starts_with(b"-----BEGIN PGP SIGNATURE-----") => break,
            2 => out.push(line.strip_prefix(b"- ").unwrap_or(line)),
            _ => out.push(line),
        }
    }
    out
}

/// Empty text gives an empty digest, which no algorithm has.
fn hex_digest(text: &[u8]) -> Option<Vec<u8>> {
    hex::decode(text).ok()
}

/// Algorithm of an unlabelled digest: the name's hint if lengths agree, else the most used one
/// of that length.
fn by_length(len: usize, hint: Option<Algorithm>) -> Option<Algorithm> {
    if let Some(a) = hint.filter(|a| a.digest_len() == len) {
        return Some(a);
    }
    Algorithm::all().find(|a| a.digest_len() == len)
}

/// `SHA256 (name) = hex`, as `sha256sum --tag`, BSD `sha256`, `b2sum --tag`, `xxhsum --tag`.
fn parse_tag(line: &[u8]) -> Option<ListEntry> {
    let open = line.iter().position(|b| *b == b'(')?;
    let close = line.windows(4).rposition(|w| w == b") = ")?;
    let tag = std::str::from_utf8(line[..open].strip_suffix(b" ")?).ok()?;
    let algorithm = match tag {
        "BLAKE2b" | "BLAKE2b-512" => Algorithm::Blake2b,
        "XXH3_128" => Algorithm::Xxh128,
        t => Algorithm::parse(t)?,
    };
    let digest = hex_digest(line[close + 4..].trim_ascii())?;
    let name = line.get(open + 1..close).filter(|n| !n.is_empty())?;
    (digest.len() == algorithm.digest_len()).then(|| ListEntry {
        name: name.to_vec(),
        algorithm,
        digest,
    })
}

/// `hex  name`, `hex *name` (binary mode), `\hex  esc\\aped\nname`.
fn parse_gnu(line: &[u8], hint: Option<Algorithm>) -> Option<ListEntry> {
    let (escaped, line) = match line.strip_prefix(b"\\") {
        Some(rest) => (true, rest),
        None => (false, line),
    };
    let space = line.iter().position(|b| *b == b' ')?;
    let digest = hex_digest(&line[..space])?;
    let rest = &line[space + 1..];
    let raw = rest.strip_prefix(b" ").or_else(|| rest.strip_prefix(b"*"))?;
    let name = if escaped { unescape(raw)? } else { raw.to_vec() };
    if name.is_empty() {
        return None;
    }
    let algorithm = by_length(digest.len(), hint)?;
    Some(ListEntry {
        name,
        algorithm,
        digest,
    })
}

fn unescape(raw: &[u8]) -> Option<Vec<u8>> {
    let mut out = Vec::with_capacity(raw.len());
    let mut it = raw.iter();
    while let Some(b) = it.next() {
        if *b != b'\\' {
            out.push(*b);
            continue;
        }
        out.push(match it.next()? {
            b'\\' => b'\\',
            b'n' => b'\n',
            b'r' => b'\r',
            _ => return None,
        });
    }
    Some(out)
}

/// `name crc32` per line; `;` starts a comment.
fn parse_sfv(line: &[u8]) -> Option<ListEntry> {
    if line.first() == Some(&b';') {
        return None;
    }
    let line = line.trim_ascii_end();
    let space = line.iter().rposition(|b| *b == b' ')?;
    let digest = hex_digest(&line[space + 1..])?;
    let name = line[..space].trim_ascii_end();
    (digest.len() == Algorithm::Crc32.digest_len() && !name.is_empty()).then(|| ListEntry {
        name: name.to_vec(),
        algorithm: Algorithm::Crc32,
        digest,
    })
}

/// `x.iso.sha256` holding only the digest: names `x.iso`.
fn single_hash(file_name: &[u8], data: &[u8], hint: Option<Algorithm>) -> Option<ListEntry> {
    let text = body_lines(data)
        .into_iter()
        .map(<[u8]>::trim_ascii)
        .find(|l| !l.is_empty())?;
    let digest = hex_digest(text)?;
    let dot = file_name.iter().rposition(|b| *b == b'.')?;
    let name = &file_name[..dot];
    let algorithm = by_length(digest.len(), hint)?;
    (!name.is_empty()).then(|| ListEntry {
        name: name.to_vec(),
        algorithm,
        digest,
    })
}
