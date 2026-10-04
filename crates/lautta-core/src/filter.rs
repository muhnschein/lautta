// SPDX-License-Identifier: LGPL-2.1-or-later
//! Instant filter (BRW-3): substring match that ignores case and diacritics,
//! hidden-files toggle (dot names and the hidden attribute) and type chips.

use crate::entry::Entry;
use crate::mime::{self, FileCategory};
use std::collections::BTreeSet;
use unicode_normalization::char::is_combining_mark;
use unicode_normalization::UnicodeNormalization;

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum TypeChip {
    Images,
    Videos,
    Audio,
    Documents,
    Archives,
    Folders,
}

impl TypeChip {
    pub fn matches(self, category: FileCategory) -> bool {
        use FileCategory as C;
        match self {
            TypeChip::Images => category == C::Image,
            TypeChip::Videos => category == C::Video,
            TypeChip::Audio => category == C::Audio,
            TypeChip::Archives => category == C::Archive,
            TypeChip::Folders => category == C::Folder,
            TypeChip::Documents => matches!(
                category,
                C::Text | C::Code | C::Markdown | C::Pdf | C::Document | C::Spreadsheet | C::Presentation
            ),
        }
    }
}

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
    /// Empty means every type; otherwise an entry must match one of the
    /// chips (folders included only with [`TypeChip::Folders`]).
    pub chips: BTreeSet<TypeChip>,
}

impl FilterOptions {
    /// No text, no chips; only the hidden-files toggle set.
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
        self.needle.is_empty() && self.chips.is_empty()
    }

    pub fn matches(&self, entry: &Entry) -> bool {
        (self.show_hidden || !entry.is_hidden()) && self.matches_text(entry) && self.matches_chips(entry)
    }

    fn matches_text(&self, entry: &Entry) -> bool {
        self.needle.is_empty() || fold_text(&String::from_utf8_lossy(&entry.name)).contains(&self.needle)
    }

    fn matches_chips(&self, entry: &Entry) -> bool {
        if self.chips.is_empty() {
            return true;
        }
        let category = mime::category_of(entry);
        self.chips.iter().any(|c| c.matches(category))
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
    fn chips_select_categories() {
        let mut o = opts("");
        o.chips.insert(TypeChip::Images);
        assert!(o.matches(&file("a.png")));
        assert!(!o.matches(&file("a.mp4")));
        assert!(!o.matches(&Entry::new(b"dir", Kind::Dir)));
        o.chips.insert(TypeChip::Folders);
        assert!(o.matches(&Entry::new(b"dir", Kind::Dir)));
        o.chips.clear();
        o.chips.insert(TypeChip::Documents);
        for n in ["a.txt", "a.rs", "a.md", "a.pdf", "a.docx", "a.xlsx", "a.pptx"] {
            assert!(o.matches(&file(n)), "{n}");
        }
        assert!(!o.matches(&file("a.zip")));
        o.chips = [TypeChip::Videos, TypeChip::Audio, TypeChip::Archives]
            .into_iter()
            .collect();
        assert!(o.matches(&file("a.mkv")));
        assert!(o.matches(&file("a.flac")));
        assert!(o.matches(&file("a.7z")));
        assert!(!o.matches(&file("a.png")));
    }

    #[test]
    fn text_and_chips_combine() {
        let mut o = opts("holiday");
        o.chips.insert(TypeChip::Images);
        assert!(o.matches(&file("Holiday.jpg")));
        assert!(!o.matches(&file("Holiday.txt")));
        assert!(!o.matches(&file("work.jpg")));
        assert!(!o.is_inactive());
        assert!(FilterOptions::default().is_inactive());
    }
}
