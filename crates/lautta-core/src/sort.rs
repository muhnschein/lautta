// SPDX-License-Identifier: LGPL-2.1-or-later
//! Listing order (BRW-2): natural, case-insensitive name order over lowercased
//! NFC text with a byte-order tiebreak; size, modified and type orders;
//! folders-first toggle; stable. Runs in core, off the GUI thread (ARC-8).

use crate::entry::Entry;
use crate::mime::{self, FileCategory};
use std::cmp::Ordering;
use unicode_normalization::UnicodeNormalization;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
pub enum SortKey {
    #[default]
    Name,
    Size,
    Modified,
    Type,
}

impl SortKey {
    /// Stable identifier used in the database and settings.
    pub fn as_str(self) -> &'static str {
        match self {
            SortKey::Name => "name",
            SortKey::Size => "size",
            SortKey::Modified => "modified",
            SortKey::Type => "type",
        }
    }

    pub fn parse(s: &str) -> Option<SortKey> {
        match s {
            "name" => Some(SortKey::Name),
            "size" => Some(SortKey::Size),
            "modified" => Some(SortKey::Modified),
            "type" => Some(SortKey::Type),
            _ => None,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct SortOptions {
    pub key: SortKey,
    pub descending: bool,
    pub folders_first: bool,
}

impl Default for SortOptions {
    fn default() -> SortOptions {
        SortOptions {
            key: SortKey::Name,
            descending: false,
            folders_first: true,
        }
    }
}

/// Lowercased NFC form of a name; invalid UTF-8 is decoded lossily (the raw
/// bytes still break ties).
pub fn fold_name(name: &[u8]) -> String {
    String::from_utf8_lossy(name)
        .nfc()
        .collect::<String>()
        .to_lowercase()
}

/// Natural order: runs of ASCII digits compare by numeric value, everything
/// else bytewise (UTF-8 byte order equals code point order).
pub fn natural_cmp(a: &str, b: &str) -> Ordering {
    let (a, b) = (a.as_bytes(), b.as_bytes());
    let (mut i, mut j) = (0, 0);
    while i < a.len() && j < b.len() {
        let ord = if a[i].is_ascii_digit() && b[j].is_ascii_digit() {
            let (ra, ni) = digit_run(a, i);
            let (rb, nj) = digit_run(b, j);
            i = ni;
            j = nj;
            cmp_digit_runs(ra, rb)
        } else {
            let ord = a[i].cmp(&b[j]);
            i += 1;
            j += 1;
            ord
        };
        if ord != Ordering::Equal {
            return ord;
        }
    }
    (a.len() - i).cmp(&(b.len() - j))
}

fn digit_run(s: &[u8], start: usize) -> (&[u8], usize) {
    let len = s[start..].iter().take_while(|b| b.is_ascii_digit()).count();
    (&s[start..start + len], start + len)
}

fn cmp_digit_runs(a: &[u8], b: &[u8]) -> Ordering {
    let strip = |s: &[u8]| {
        let zeros = s.iter().take_while(|b| **b == b'0').count();
        s[zeros..].to_vec()
    };
    let (a, b) = (strip(a), strip(b));
    a.len().cmp(&b.len()).then_with(|| a.cmp(&b))
}

/// Per-entry data derived once and reused across re-sorts (keeps 20 000-entry
/// sorts allocation-free in the comparator).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SortRecord {
    pub name_key: String,
    pub category: FileCategory,
    pub extension: String,
}

impl SortRecord {
    pub fn for_entry(e: &Entry) -> SortRecord {
        SortRecord {
            name_key: fold_name(&e.name),
            category: mime::category_of(e),
            extension: mime::extension_of(&e.name).unwrap_or_default(),
        }
    }
}

fn name_order(ea: &Entry, ra: &SortRecord, eb: &Entry, rb: &SortRecord) -> Ordering {
    natural_cmp(&ra.name_key, &rb.name_key).then_with(|| ea.name.cmp(&eb.name))
}

/// Total order of two entries under `opts`. Descending reverses the key order
/// only; folders stay first and ties fall back to ascending name order.
pub fn compare_entries(
    ea: &Entry,
    ra: &SortRecord,
    eb: &Entry,
    rb: &SortRecord,
    opts: &SortOptions,
) -> Ordering {
    if opts.folders_first {
        let (da, db) = (ea.is_dir(), eb.is_dir());
        if da != db {
            return db.cmp(&da);
        }
    }
    let primary = match opts.key {
        SortKey::Name => name_order(ea, ra, eb, rb),
        SortKey::Size => ea.size.cmp(&eb.size),
        SortKey::Modified => ea.modified.cmp(&eb.modified),
        SortKey::Type => ra
            .category
            .cmp(&rb.category)
            .then_with(|| ra.extension.cmp(&rb.extension)),
    };
    let primary = if opts.descending {
        primary.reverse()
    } else {
        primary
    };
    primary.then_with(|| name_order(ea, ra, eb, rb))
}

/// Sorts `indices` (positions into `entries`/`records`) in place, stably.
pub fn sort_indices(indices: &mut [u32], entries: &[Entry], records: &[SortRecord], opts: &SortOptions) {
    indices.sort_by(|&a, &b| {
        let (a, b) = (a as usize, b as usize);
        compare_entries(&entries[a], &records[a], &entries[b], &records[b], opts)
    });
}

/// Index permutation that lists `entries` in the requested order.
pub fn sort_permutation(entries: &[Entry], opts: &SortOptions) -> Vec<u32> {
    let records: Vec<SortRecord> = entries.iter().map(SortRecord::for_entry).collect();
    let mut idx: Vec<u32> = (0..entries.len() as u32).collect();
    sort_indices(&mut idx, entries, &records, opts);
    idx
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::entry::{ms_to_system_time, Kind};

    fn names(entries: &[Entry], perm: &[u32]) -> Vec<String> {
        perm.iter().map(|&i| entries[i as usize].display_name()).collect()
    }

    fn files(list: &[&str]) -> Vec<Entry> {
        list.iter()
            .map(|n| Entry::new(n.as_bytes(), Kind::File))
            .collect()
    }

    #[test]
    fn natural_numbers() {
        let e = files(&["file10", "file2", "File1", "file02", "file1"]);
        let p = sort_permutation(&e, &SortOptions::default());
        // Equal folded keys (File1/file1, file02/file2) fall back to byte order.
        assert_eq!(names(&e, &p), ["File1", "file1", "file02", "file2", "file10"]);
    }

    #[test]
    fn natural_cmp_edge_cases() {
        assert_eq!(natural_cmp("a", "a"), Ordering::Equal);
        assert_eq!(natural_cmp("a1", "a1b"), Ordering::Less);
        assert_eq!(natural_cmp("a1b", "a1"), Ordering::Greater);
        assert_eq!(natural_cmp("a9", "a10"), Ordering::Less);
        assert_eq!(natural_cmp("a007", "a7"), Ordering::Equal);
        assert_eq!(natural_cmp("a0", "a00"), Ordering::Equal);
        assert_eq!(
            natural_cmp("a99999999999999999999", "a100000000000000000000"),
            Ordering::Less
        );
        assert_eq!(natural_cmp("1a", "a1"), Ordering::Less);
        assert_eq!(natural_cmp("", "a"), Ordering::Less);
    }

    #[test]
    fn case_and_nfc_insensitive() {
        let e = files(&["banana", "Apple", "cherry", "apple2"]);
        let p = sort_permutation(&e, &SortOptions::default());
        assert_eq!(names(&e, &p), ["Apple", "apple2", "banana", "cherry"]);
        // Decomposed and precomposed e-acute fold to the same key.
        assert_eq!(fold_name("e\u{301}".as_bytes()), fold_name("\u{e9}".as_bytes()));
        assert_eq!(fold_name(b"ABC"), "abc");
        let e = files(&["e\u{301}b", "\u{e9}a"]);
        let p = sort_permutation(&e, &SortOptions::default());
        assert_eq!(p, [1, 0]);
    }

    #[test]
    fn byte_order_breaks_ties_and_handles_invalid_utf8() {
        let mut e = files(&["a"]);
        e.push(Entry::new(b"a\xff", Kind::File));
        e.push(Entry::new(b"a\xfe", Kind::File));
        let p = sort_permutation(&e, &SortOptions::default());
        // Both invalid names fold to "a\u{fffd}"; raw bytes decide.
        assert_eq!(p, [0, 2, 1]);
    }

    #[test]
    fn folders_first_toggle_and_descending() {
        let mut e = files(&["b.txt", "a.txt"]);
        e.push(Entry::new(b"z", Kind::Dir));
        e.push(Entry::new(b"y", Kind::Dir));
        let mut o = SortOptions::default();
        assert_eq!(names(&e, &sort_permutation(&e, &o)), ["y", "z", "a.txt", "b.txt"]);
        o.descending = true;
        // Folders stay first when descending.
        assert_eq!(names(&e, &sort_permutation(&e, &o)), ["z", "y", "b.txt", "a.txt"]);
        o.folders_first = false;
        assert_eq!(names(&e, &sort_permutation(&e, &o)), ["z", "y", "b.txt", "a.txt"]);
        o.descending = false;
        assert_eq!(names(&e, &sort_permutation(&e, &o)), ["a.txt", "b.txt", "y", "z"]);
    }

    #[test]
    fn symlink_to_folder_sorts_with_folders() {
        let mut e = files(&["a"]);
        let mut l = Entry::new(b"l", Kind::Symlink);
        l.target_kind = Kind::Dir;
        e.push(l);
        assert_eq!(
            names(&e, &sort_permutation(&e, &SortOptions::default())),
            ["l", "a"]
        );
    }

    #[test]
    fn size_and_modified() {
        let mut e = files(&["a", "b", "c", "d"]);
        e[0].size = Some(30);
        e[1].size = Some(10);
        e[2].size = Some(10);
        e[3].size = None;
        e[0].modified = Some(ms_to_system_time(5));
        e[1].modified = Some(ms_to_system_time(9));
        e[2].modified = Some(ms_to_system_time(1));
        let mut o = SortOptions {
            key: SortKey::Size,
            ..SortOptions::default()
        };
        assert_eq!(names(&e, &sort_permutation(&e, &o)), ["d", "b", "c", "a"]);
        o.descending = true;
        // Ties (b, c) keep ascending name order even when descending.
        assert_eq!(names(&e, &sort_permutation(&e, &o)), ["a", "b", "c", "d"]);
        o.key = SortKey::Modified;
        o.descending = false;
        assert_eq!(names(&e, &sort_permutation(&e, &o)), ["d", "c", "a", "b"]);
        o.descending = true;
        assert_eq!(names(&e, &sort_permutation(&e, &o)), ["b", "a", "c", "d"]);
    }

    #[test]
    fn type_sorts_by_category_then_extension_then_name() {
        let e = files(&["z.txt", "a.mp3", "b.png", "c.jpg", "a.txt", "d.zip", "noext"]);
        let o = SortOptions {
            key: SortKey::Type,
            ..SortOptions::default()
        };
        // Image (jpg < png), Audio, Text, Archive, Other.
        assert_eq!(
            names(&e, &sort_permutation(&e, &o)),
            ["c.jpg", "b.png", "a.mp3", "a.txt", "z.txt", "d.zip", "noext"]
        );
    }

    #[test]
    fn sort_is_stable_for_identical_entries() {
        let mut e = files(&["same", "same", "same"]);
        e[0].size = Some(1);
        e[1].size = Some(2);
        e[2].size = Some(3);
        // Identical names: input order is preserved.
        let o = SortOptions::default();
        assert_eq!(sort_permutation(&e, &o), [0, 1, 2]);
    }

    #[test]
    fn sort_key_round_trip() {
        for k in [SortKey::Name, SortKey::Size, SortKey::Modified, SortKey::Type] {
            assert_eq!(SortKey::parse(k.as_str()), Some(k));
        }
        assert_eq!(SortKey::parse("bogus"), None);
    }

    #[test]
    fn empty_input() {
        assert!(sort_permutation(&[], &SortOptions::default()).is_empty());
    }
}
