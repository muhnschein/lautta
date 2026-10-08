// SPDX-License-Identifier: LGPL-2.1-or-later
//! Instant filter (BRW-3): substring match that ignores case and diacritics,
//! and the hidden-files toggle (dot names and the hidden attribute). The
//! folder picker also hides files.

use crate::entry::Entry;
use unicode_normalization::char::is_combining_mark;
use unicode_normalization::UnicodeNormalization;

/// Case- and diacritics-insensitive form used for matching.
pub fn fold_text(s: &str) -> String {
    s.nfd()
        .filter(|c| !is_combining_mark(*c))
        .flat_map(char::to_lowercase)
        .collect()
}

#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct FilterOptions {
    needle: String,
    pub show_hidden: bool,
    /// Only folders pass (the folder picker, OPS-10).
    pub folders_only: bool,
}

impl FilterOptions {
    /// No text, files and folders; only the hidden-files toggle set.
    pub fn with_hidden(show_hidden: bool) -> FilterOptions {
        FilterOptions {
            show_hidden,
            ..FilterOptions::default()
        }
    }

    pub fn set_text(&mut self, text: &str) {
        self.needle = fold_text(text.trim());
    }

    /// The folded search text (empty when no text filter is active).
    pub fn text(&self) -> &str {
        &self.needle
    }

    /// True when nothing but the hidden toggle could exclude an entry.
    pub fn is_inactive(&self) -> bool {
        self.needle.is_empty() && !self.folders_only
    }

    pub fn matches(&self, entry: &Entry) -> bool {
        (self.show_hidden || !entry.is_hidden())
            && (!self.folders_only || entry.is_dir())
            && self.matches_text(entry)
    }

    fn matches_text(&self, entry: &Entry) -> bool {
        self.needle.is_empty() || fold_text(&String::from_utf8_lossy(&entry.name)).contains(&self.needle)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::entry::{EntryFlags, Kind};

    fn file(name: &str) -> Entry {
        Entry::new(name.as_bytes(), Kind::File)
    }

    fn opts(text: &str) -> FilterOptions {
        let mut o = FilterOptions {
            show_hidden: true,
            ..FilterOptions::default()
        };
        o.set_text(text);
        o
    }

    #[test]
    fn substring_ignores_case_and_diacritics() {
        let o = opts("cafe");
        assert!(o.matches(&file("Caf\u{e9}.txt")));
        assert!(o.matches(&file("CAFE")));
        assert!(o.matches(&file("my cafe\u{301} notes")));
        assert!(!o.matches(&file("coffee")));
        let o = opts("CAF\u{c9}");
        assert!(o.matches(&file("cafe")));
        assert!(opts("na\u{ef}ve").matches(&file("NAIVE.md")));
    }

    #[test]
    fn empty_text_matches_everything_and_text_is_trimmed() {
        assert!(opts("").matches(&file("x")));
        assert!(opts("  ").matches(&file("x")));
        assert_eq!(opts("  Ab ").text(), "ab");
        assert!(opts(" b ").matches(&file("abc")));
    }

    #[test]
    fn invalid_utf8_names_are_filterable() {
        let e = Entry::new(b"ab\xffcd", Kind::File);
        assert!(opts("ab").matches(&e));
        assert!(!opts("zz").matches(&e));
    }

    #[test]
    fn hidden_toggle() {
        let mut o = FilterOptions::default();
        assert!(!o.matches(&file(".git")));
        assert!(o.matches(&file("visible")));
        let mut attr = file("plain");
        attr.flags |= EntryFlags::HIDDEN;
        assert!(!o.matches(&attr));
        o.show_hidden = true;
        assert!(o.matches(&file(".git")));
        assert!(o.matches(&attr));
    }

    #[test]
    fn folders_only_hides_files() {
        let mut o = opts("");
        assert!(o.matches(&file("a.png")));
        o.folders_only = true;
        assert!(!o.matches(&file("a.png")));
        assert!(o.matches(&Entry::new(b"dir", Kind::Dir)));
    }

    #[test]
    fn text_and_folders_only_combine() {
        let mut o = opts("holiday");
        assert!(!o.is_inactive());
        o.folders_only = true;
        assert!(o.matches(&Entry::new(b"Holiday", Kind::Dir)));
        assert!(!o.matches(&file("Holiday.jpg")));
        assert!(!o.matches(&Entry::new(b"Work", Kind::Dir)));
        o.set_text("");
        assert!(!o.is_inactive(), "folders only filters");
        assert!(FilterOptions::default().is_inactive());
    }
}
