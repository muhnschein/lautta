// SPDX-License-Identifier: LGPL-2.1-or-later
//! View settings (BRW-4): one global set for every folder, kept in the
//! settings (SPEC §18). Grid suggestion for media folders (SPEC 15.3).

use crate::entry::Entry;
use crate::mime::{self, FileCategory};
use crate::sort::{SortKey, SortOptions};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
pub enum ViewMode {
    #[default]
    List,
    Grid,
}

impl ViewMode {
    pub fn as_str(self) -> &'static str {
        match self {
            ViewMode::List => "list",
            ViewMode::Grid => "grid",
        }
    }

    pub fn parse(s: &str) -> Option<ViewMode> {
        match s {
            "list" => Some(ViewMode::List),
            "grid" => Some(ViewMode::Grid),
            _ => None,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ViewPrefs {
    pub sort: SortKey,
    pub descending: bool,
    pub folders_first: bool,
    pub show_hidden: bool,
    pub view_mode: ViewMode,
    pub thumbnails: bool,
}

impl Default for ViewPrefs {
    fn default() -> ViewPrefs {
        ViewPrefs {
            sort: SortKey::Name,
            descending: false,
            folders_first: true,
            show_hidden: false,
            view_mode: ViewMode::List,
            thumbnails: true,
        }
    }
}

impl ViewPrefs {
    pub fn sort_options(&self) -> SortOptions {
        SortOptions {
            key: self.sort,
            descending: self.descending,
            folders_first: self.folders_first,
        }
    }
}

/// Grid is suggested when more than 60 % of the entries are images or videos
/// (SPEC 15.3).
pub fn suggest_grid(entries: &[Entry]) -> bool {
    let media = entries
        .iter()
        .filter(|e| matches!(mime::category_of(e), FileCategory::Image | FileCategory::Video))
        .count();
    media * 10 > entries.len() * 6
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::entry::Kind;

    #[test]
    fn sort_options_follow_prefs() {
        let o = ViewPrefs {
            sort: SortKey::Modified,
            descending: true,
            folders_first: false,
            ..ViewPrefs::default()
        }
        .sort_options();
        assert_eq!(o.key, SortKey::Modified);
        assert!(o.descending);
        assert!(!o.folders_first);
    }

    #[test]
    fn view_mode_round_trip() {
        for m in [ViewMode::List, ViewMode::Grid] {
            assert_eq!(ViewMode::parse(m.as_str()), Some(m));
        }
        assert_eq!(ViewMode::parse("tiles"), None);
    }

    fn media(n_media: usize, n_other: usize) -> Vec<Entry> {
        let mut v: Vec<Entry> = (0..n_media)
            .map(|i| Entry::new(format!("p{i}.jpg").as_bytes(), Kind::File))
            .collect();
        v.extend((0..n_other).map(|i| Entry::new(format!("t{i}.txt").as_bytes(), Kind::File)));
        v
    }

    #[test]
    fn grid_suggestion_threshold_is_over_sixty_percent() {
        assert!(!suggest_grid(&[]));
        assert!(!suggest_grid(&media(6, 4)), "exactly 60 % is not more than 60 %");
        assert!(suggest_grid(&media(7, 4)));
        assert!(suggest_grid(&media(10, 0)));
        assert!(!suggest_grid(&media(0, 5)));
        let mut v = media(5, 0);
        v.push(Entry::new(b"clip.mp4", Kind::File));
        v.push(Entry::new(b"dir", Kind::Dir));
        assert!(suggest_grid(&v), "videos count as media, folders do not");
    }
}
