// SPDX-License-Identifier: LGPL-2.1-or-later
//! Name handling for operations: "keep both" names (OPS-2), names that are
//! invalid on the destination and their safe replacements (OPS-7), and
//! case-only rename detection (OPS-3).

use crate::entry::{cap, Capabilities};
use crate::vpath::validate_name;

/// Longest name most filesystems accept, in bytes.
pub const DEFAULT_MAX_NAME_BYTES: usize = 255;

const RESERVED_CHARS: &[u8] = b"<>:\"/\\|?*";

/// Which naming rules a destination enforces.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct NameRules {
    /// vfat/exFAT/SMB rules: reserved characters, trailing dots and spaces,
    /// device names, UTF-8 only.
    pub restricted: bool,
    pub max_bytes: usize,
    pub case_insensitive: bool,
}

impl NameRules {
    /// POSIX-like: any byte but `/` and NUL, 255 bytes.
    pub fn permissive() -> NameRules {
        NameRules {
            restricted: false,
            max_bytes: DEFAULT_MAX_NAME_BYTES,
            case_insensitive: false,
        }
    }

    pub fn from_capabilities(caps: &Capabilities) -> NameRules {
        let max_bytes = caps
            .max_name_bytes
            .and_then(|m| usize::try_from(m).ok())
            .filter(|m| *m > 0)
            .unwrap_or(DEFAULT_MAX_NAME_BYTES);
        NameRules {
            restricted: caps.has(cap::RESTRICTED_NAMES),
            max_bytes,
            case_insensitive: caps.has(cap::CASE_INSENSITIVE),
        }
    }
}

/// Splits `name` into stem and extension (the extension keeps its dot).
/// Dotfiles have no extension; `.tar.gz` style double extensions stay together;
/// folders never split.
pub fn split_extension(name: &[u8], is_dir: bool) -> (&[u8], &[u8]) {
    if is_dir {
        return (name, &[]);
    }
    let Some(last) = last_extension_start(name) else {
        return (name, &[]);
    };
    let stem = &name[..last];
    if let Some(inner) = last_extension_start(stem) {
        if stem[inner..].eq_ignore_ascii_case(b".tar") {
            return (&name[..inner], &name[inner..]);
        }
    }
    (stem, &name[last..])
}

/// Index of the dot that starts the extension, if the name has one. The
/// extension must be short and free of spaces so "Mr. Smith" is not split.
fn last_extension_start(name: &[u8]) -> Option<usize> {
    let dot = name.iter().rposition(|b| *b == b'.')?;
    let ext = &name[dot + 1..];
    if dot == 0 || ext.is_empty() || ext.len() > 16 || ext.contains(&b' ') {
        return None;
    }
    Some(dot)
}

/// `name 2.ext`, `name 3.ext`, … for a file; the first candidate not `taken`.
pub fn keep_both_name(name: &[u8], taken: &dyn Fn(&[u8]) -> bool) -> Vec<u8> {
    numbered_name(name, false, taken)
}

/// Same for folders: the number is appended to the whole name.
pub fn keep_both_dir_name(name: &[u8], taken: &dyn Fn(&[u8]) -> bool) -> Vec<u8> {
    numbered_name(name, true, taken)
}

/// Picks the file or folder variant of [`keep_both_name`].
pub fn keep_both_for(name: &[u8], is_dir: bool, taken: &dyn Fn(&[u8]) -> bool) -> Vec<u8> {
    numbered_name(name, is_dir, taken)
}

/// Candidates tried before giving up and using a number nothing probably has.
const KEEP_BOTH_TRIES: u32 = 100_000;

fn numbered_name(name: &[u8], is_dir: bool, taken: &dyn Fn(&[u8]) -> bool) -> Vec<u8> {
    let (stem, ext) = split_extension(name, is_dir);
    let candidate = |n: u32| {
        let mut c = stem.to_vec();
        c.extend_from_slice(format!(" {n}").as_bytes());
        c.extend_from_slice(ext);
        c
    };
    (2..=KEEP_BOTH_TRIES)
        .map(candidate)
        .find(|c| !taken(c))
        .unwrap_or_else(|| candidate(KEEP_BOTH_TRIES + 1))
}

/// True when `a` and `b` differ only in letter case (OPS-3).
pub fn is_case_only_rename(a: &[u8], b: &[u8]) -> bool {
    if a == b {
        return false;
    }
    match (std::str::from_utf8(a), std::str::from_utf8(b)) {
        (Ok(x), Ok(y)) => x.to_lowercase() == y.to_lowercase(),
        _ => a.eq_ignore_ascii_case(b),
    }
}

fn is_device_name(stem: &str) -> bool {
    let upper = stem.to_ascii_uppercase();
    if matches!(upper.as_str(), "CON" | "PRN" | "AUX" | "NUL") {
        return true;
    }
    let digit = upper.strip_prefix("COM").or_else(|| upper.strip_prefix("LPT"));
    matches!(digit, Some(d) if d.len() == 1 && matches!(d.as_bytes()[0], b'1'..=b'9'))
}

/// The part before the first dot decides whether a name is a device name.
fn device_prefix_len(name: &str) -> Option<usize> {
    let end = name.find('.').unwrap_or(name.len());
    is_device_name(&name[..end]).then_some(end)
}

fn restricted_ok(name: &[u8]) -> bool {
    let Ok(s) = std::str::from_utf8(name) else {
        return false;
    };
    if s.bytes()
        .any(|b| b < 0x20 || b == 0x7f || RESERVED_CHARS.contains(&b))
    {
        return false;
    }
    if s.ends_with('.') || s.ends_with(' ') {
        return false;
    }
    device_prefix_len(s).is_none()
}

/// Whether `name` can be created as-is on a destination with `rules` (OPS-7).
pub fn is_valid_name(name: &[u8], rules: NameRules) -> bool {
    if validate_name(name).is_err() || name.len() > rules.max_bytes {
        return false;
    }
    !rules.restricted || restricted_ok(name)
}

/// A name that is valid under `rules`, as close to `name` as possible.
pub fn safe_name(name: &[u8], rules: NameRules) -> Vec<u8> {
    if is_valid_name(name, rules) {
        return name.to_vec();
    }
    let cleaned = if rules.restricted {
        clean_restricted(name)
    } else {
        name.iter()
            .map(|b| if *b == b'/' || *b == 0 { b'_' } else { *b })
            .collect()
    };
    let cleaned = if cleaned == b"." || cleaned == b".." {
        b"_".to_vec()
    } else {
        cleaned
    };
    let mut out = truncate_keeping_extension(&cleaned, rules.max_bytes);
    if rules.restricted {
        out = trim_trailing_dots_spaces(out);
    }
    if out.is_empty() {
        out.push(b'_');
    }
    out
}

fn clean_restricted(name: &[u8]) -> Vec<u8> {
    let lossy = String::from_utf8_lossy(name);
    let mut s: String = lossy
        .chars()
        .map(|c| {
            let reserved =
                c.is_control() || c == '\u{FFFD}' || (c.is_ascii() && RESERVED_CHARS.contains(&(c as u8)));
            if reserved {
                '_'
            } else {
                c
            }
        })
        .collect();
    while s.ends_with('.') || s.ends_with(' ') {
        s.pop();
    }
    if let Some(end) = device_prefix_len(&s) {
        s.insert(end, '_');
    }
    s.into_bytes()
}

fn trim_trailing_dots_spaces(mut v: Vec<u8>) -> Vec<u8> {
    while matches!(v.last(), Some(b'.' | b' ')) {
        v.pop();
    }
    v
}

fn truncate_utf8(bytes: &[u8], max: usize) -> &[u8] {
    if bytes.len() <= max {
        return bytes;
    }
    let mut end = max;
    // A UTF-8 continuation byte has the bit pattern 10xxxxxx.
    while end > 0 && (bytes[end] & 0xC0) == 0x80 {
        end -= 1;
    }
    &bytes[..end]
}

fn truncate_keeping_extension(name: &[u8], max: usize) -> Vec<u8> {
    if name.len() <= max {
        return name.to_vec();
    }
    let (stem, ext) = split_extension(name, false);
    if ext.len() >= max / 2 {
        return truncate_utf8(name, max).to_vec();
    }
    let mut out = truncate_utf8(stem, max - ext.len()).to_vec();
    out.extend_from_slice(ext);
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    fn never(_: &[u8]) -> bool {
        false
    }

    #[test]
    fn splits_extensions() {
        assert_eq!(split_extension(b"a.txt", false), (&b"a"[..], &b".txt"[..]));
        assert_eq!(split_extension(b"a.tar.gz", false), (&b"a"[..], &b".tar.gz"[..]));
        assert_eq!(split_extension(b"a.TAR.xz", false), (&b"a"[..], &b".TAR.xz"[..]));
        assert_eq!(split_extension(b".bashrc", false), (&b".bashrc"[..], &b""[..]));
        assert_eq!(split_extension(b"noext", false), (&b"noext"[..], &b""[..]));
        assert_eq!(split_extension(b"dot.", false), (&b"dot."[..], &b""[..]));
        assert_eq!(split_extension(b"a.b.c", false), (&b"a.b"[..], &b".c"[..]));
        assert_eq!(
            split_extension(b"Mr. Smith", false),
            (&b"Mr. Smith"[..], &b""[..])
        );
        assert_eq!(split_extension(b"v1.2", true), (&b"v1.2"[..], &b""[..]));
        assert_eq!(split_extension(b".tar.gz", false), (&b".tar"[..], &b".gz"[..]));
        let long = b"a.abcdefghijklmnopq";
        assert_eq!(split_extension(long, false).1, b"");
        let max = b"a.abcdefghijklmnop";
        assert_eq!(split_extension(max, false).1, b".abcdefghijklmnop");
    }

    #[test]
    fn keep_both_numbers_skip_taken() {
        assert_eq!(keep_both_name(b"a.txt", &never), b"a 2.txt");
        let taken = |n: &[u8]| n == b"a 2.txt" || n == b"a 3.txt";
        assert_eq!(keep_both_name(b"a.txt", &taken), b"a 4.txt");
        assert_eq!(keep_both_name(b"a.tar.gz", &never), b"a 2.tar.gz");
        assert_eq!(keep_both_name(b".rc", &never), b".rc 2");
        assert_eq!(keep_both_name(b"README", &never), b"README 2");
        assert_eq!(keep_both_dir_name(b"v1.2", &never), b"v1.2 2");
        assert_eq!(keep_both_for(b"x.d", true, &never), b"x.d 2");
        assert_eq!(keep_both_for(b"x.d", false, &never), b"x 2.d");
    }

    #[test]
    fn keep_both_gives_up_when_everything_is_taken() {
        let all = |_: &[u8]| true;
        assert_eq!(keep_both_name(b"a.b", &all), b"a 100001.b");
        let last = |n: &[u8]| n != b"a 100000";
        assert_eq!(keep_both_name(b"a", &last), b"a 100000");
    }

    #[test]
    fn case_only() {
        assert!(is_case_only_rename(b"a.TXT", b"a.txt"));
        assert!(is_case_only_rename("É".as_bytes(), "é".as_bytes()));
        assert!(!is_case_only_rename(b"a", b"a"));
        assert!(!is_case_only_rename(b"a", b"b"));
        assert!(is_case_only_rename(b"A\xff", b"a\xff"));
        assert!(!is_case_only_rename(b"A\xff", b"a\xfe"));
    }

    #[test]
    fn rules_from_capabilities() {
        let r = NameRules::from_capabilities(&Capabilities::with(&[
            cap::RESTRICTED_NAMES,
            cap::CASE_INSENSITIVE,
        ]));
        assert!(r.restricted && r.case_insensitive);
        assert_eq!(r.max_bytes, 255);
        let mut c = Capabilities {
            max_name_bytes: Some(100),
            ..Capabilities::default()
        };
        let r = NameRules::from_capabilities(&c);
        assert!(!r.restricted && !r.case_insensitive);
        assert_eq!(r.max_bytes, 100);
        c.max_name_bytes = Some(0);
        assert_eq!(NameRules::from_capabilities(&c).max_bytes, 255);
    }

    fn restricted() -> NameRules {
        NameRules {
            restricted: true,
            ..NameRules::permissive()
        }
    }

    #[test]
    fn validity() {
        let p = NameRules::permissive();
        let r = restricted();
        assert!(is_valid_name(b"a:b", p));
        assert!(!is_valid_name(b"a:b", r));
        assert!(!is_valid_name(b"", p));
        assert!(!is_valid_name(b"..", p));
        assert!(!is_valid_name(b"a/b", p));
        assert!(is_valid_name(&[b'a'; 255], p));
        assert!(!is_valid_name(&[b'a'; 256], p));
        for bad in [
            &b"a<"[..],
            b"a>",
            b"a\"",
            b"a\\",
            b"a|",
            b"a?",
            b"a*",
            b"a\x01",
            b"a\x7f",
        ] {
            assert!(!is_valid_name(bad, r), "{bad:?}");
        }
        assert!(!is_valid_name(b"a.", r));
        assert!(!is_valid_name(b"a ", r));
        assert!(is_valid_name(b"a.b", r));
        assert!(!is_valid_name(b"caf\xe9", r));
        assert!(is_valid_name(b"caf\xe9", p));
        assert!(is_valid_name("café".as_bytes(), r));
    }

    #[test]
    fn device_names() {
        let r = restricted();
        for bad in ["CON", "con", "Prn.txt", "AUX", "nul.tar.gz", "COM1", "lpt9.x"] {
            assert!(!is_valid_name(bad.as_bytes(), r), "{bad}");
        }
        for ok in ["COM0", "COM10", "CONSOLE", "LPT", "xCON", "COM"] {
            assert!(is_valid_name(ok.as_bytes(), r), "{ok}");
        }
        assert!(is_valid_name(b"CON", NameRules::permissive()));
    }

    #[test]
    fn safe_names_restricted() {
        let r = restricted();
        assert_eq!(safe_name(b"ok.txt", r), b"ok.txt");
        assert_eq!(safe_name(b"a:b*c?.txt", r), b"a_b_c_.txt");
        assert_eq!(safe_name(b"name. . ", r), b"name");
        assert_eq!(safe_name(b"CON", r), b"CON_");
        assert_eq!(safe_name(b"com1.txt", r), b"com1_.txt");
        assert_eq!(safe_name(b"caf\xe9.txt", r), b"caf_.txt");
        assert_eq!(safe_name(b"a\x01b", r), b"a_b");
        assert_eq!(safe_name(b"...", r), b"_");
        assert_eq!(safe_name(b"a/b", r), b"a_b");
        assert_eq!(safe_name(b"..", r), b"_");
        assert_eq!(safe_name(b"?", r), b"?".iter().map(|_| b'_').collect::<Vec<u8>>());
        for name in [&b"a:b."[..], b"CON.", b"x\\y", b"nul"] {
            assert!(is_valid_name(&safe_name(name, r), r), "{name:?}");
        }
    }

    #[test]
    fn safe_names_permissive() {
        let p = NameRules::permissive();
        assert_eq!(safe_name(b"a:b", p), b"a:b");
        assert_eq!(safe_name(b"a/b\0c", p), b"a_b_c");
        assert_eq!(safe_name(b"..", p), b"_");
        assert_eq!(safe_name(b".", p), b"_");
        assert_eq!(safe_name(b"", p), b"_");
        assert_eq!(safe_name(b"trail. ", p), b"trail. ");
    }

    #[test]
    fn truncation_keeps_extension_and_utf8() {
        let p = NameRules {
            max_bytes: 10,
            ..NameRules::permissive()
        };
        assert_eq!(safe_name(b"abcdefghijklmnop.txt", p), b"abcdef.txt");
        assert_eq!(safe_name(b"abcdefghijklmnop.tar.gz", p), b"abcdefghij");
        let wide = NameRules { max_bytes: 20, ..p };
        assert_eq!(
            safe_name(b"abcdefghijklmnopqrstuvwxyz.tar.gz", wide),
            b"abcdefghijklm.tar.gz"
        );
        // 2-byte chars: the cut must not split one.
        let out = safe_name("ééééééé.md".as_bytes(), p);
        assert_eq!(out, "ééé.md".as_bytes());
        assert!(std::str::from_utf8(&out).is_ok());
        // extension too long: truncate the whole name
        assert_eq!(safe_name(b"a.abcdefghijklmn", p), b"a.abcdefgh");
        let tiny = NameRules { max_bytes: 4, ..p };
        let out = safe_name("日本語日本語日本語.txt".as_bytes(), tiny);
        assert_eq!(out, "日".as_bytes());
        assert!(is_valid_name(&out, tiny));
    }

    #[test]
    fn truncation_restricted_retrims() {
        let r = NameRules {
            max_bytes: 5,
            restricted: true,
            case_insensitive: false,
        };
        assert_eq!(safe_name(b"abc.def.ghi", r), b"abc.d");
        assert_eq!(safe_name(b"abcd.efgh", r), b"abcd");
    }
}
