// SPDX-License-Identifier: LGPL-2.1-or-later
//! Per-folder view settings (BRW-4): stored per URI in `view_prefs`; a lookup
//! inherits folder, then the location root, then the global defaults (File
//! Browser parity). Grid suggestion for media folders (SPEC 15.3).

use crate::db::Db;
use crate::entry::Entry;
use crate::error::Result;
use crate::mime::{self, FileCategory};
use crate::sort::{SortKey, SortOptions};
use crate::uri::Uri;
use rusqlite::{params, OptionalExtension};
use std::sync::Mutex;

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

/// Stored settings with the global defaults beneath them. Columns that are
/// NULL or unreadable fall back to the defaults field by field.
pub struct ViewPrefsStore {
    db: Db,
    defaults: Mutex<ViewPrefs>,
}

impl ViewPrefsStore {
    pub fn new(db: Db, defaults: ViewPrefs) -> ViewPrefsStore {
        ViewPrefsStore {
            db,
            defaults: Mutex::new(defaults),
        }
    }

    pub fn defaults(&self) -> ViewPrefs {
        match self.defaults.lock() {
            Ok(g) => *g,
            Err(p) => *p.into_inner(),
        }
    }

    /// Replaces the global defaults (Settings page, §18).
    pub fn set_defaults(&self, defaults: ViewPrefs) {
        match self.defaults.lock() {
            Ok(mut g) => *g = defaults,
            Err(p) => *p.into_inner() = defaults,
        }
    }

    /// Effective settings for a folder: its own row, else the location root's,
    /// else the global defaults.
    pub fn get(&self, uri: &Uri) -> Result<ViewPrefs> {
        if let Some(p) = self.get_exact(uri)? {
            return Ok(p);
        }
        if !uri.path.is_root() {
            if let Some(p) = self.get_exact(&Uri::root(uri.location.clone()))? {
                return Ok(p);
            }
        }
        Ok(self.defaults())
    }

    /// Settings stored for exactly this folder, without inheritance.
    pub fn get_exact(&self, uri: &Uri) -> Result<Option<ViewPrefs>> {
        let defaults = self.defaults();
        let row = self
            .db
            .lock()
            .query_row(
                "SELECT sort_key, sort_desc, folders_first, show_hidden, view_mode, thumbnails \
                 FROM view_prefs WHERE uri = ?1",
                params![uri.to_string()],
                |r| {
                    Ok(RawRow {
                        sort_key: r.get(0)?,
                        sort_desc: r.get(1)?,
                        folders_first: r.get(2)?,
                        show_hidden: r.get(3)?,
                        view_mode: r.get(4)?,
                        thumbnails: r.get(5)?,
                    })
                },
            )
            .optional()?;
        Ok(row.map(|r| r.resolve(&defaults)))
    }

    pub fn set(&self, uri: &Uri, prefs: &ViewPrefs) -> Result<()> {
        self.db.lock().execute(
            "INSERT INTO view_prefs(uri, sort_key, sort_desc, folders_first, show_hidden, view_mode, thumbnails) \
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7) \
             ON CONFLICT(uri) DO UPDATE SET sort_key = excluded.sort_key, sort_desc = excluded.sort_desc, \
             folders_first = excluded.folders_first, show_hidden = excluded.show_hidden, \
             view_mode = excluded.view_mode, thumbnails = excluded.thumbnails",
            params![
                uri.to_string(),
                prefs.sort.as_str(),
                prefs.descending,
                prefs.folders_first,
                prefs.show_hidden,
                prefs.view_mode.as_str(),
                prefs.thumbnails
            ],
        )?;
        Ok(())
    }

    /// Forgets a folder's own settings so it inherits again.
    pub fn clear(&self, uri: &Uri) -> Result<()> {
        self.db
            .lock()
            .execute("DELETE FROM view_prefs WHERE uri = ?1", params![uri.to_string()])?;
        Ok(())
    }
}

struct RawRow {
    sort_key: Option<String>,
    sort_desc: Option<bool>,
    folders_first: Option<bool>,
    show_hidden: Option<bool>,
    view_mode: Option<String>,
    thumbnails: Option<bool>,
}

impl RawRow {
    fn resolve(self, d: &ViewPrefs) -> ViewPrefs {
        ViewPrefs {
            sort: self
                .sort_key
                .as_deref()
                .and_then(SortKey::parse)
                .unwrap_or(d.sort),
            descending: self.sort_desc.unwrap_or(d.descending),
            folders_first: self.folders_first.unwrap_or(d.folders_first),
            show_hidden: self.show_hidden.unwrap_or(d.show_hidden),
            view_mode: self
                .view_mode
                .as_deref()
                .and_then(ViewMode::parse)
                .unwrap_or(d.view_mode),
            thumbnails: self.thumbnails.unwrap_or(d.thumbnails),
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

    fn store() -> ViewPrefsStore {
        ViewPrefsStore::new(Db::open_in_memory().unwrap(), ViewPrefs::default())
    }

    fn uri(s: &str) -> Uri {
        Uri::parse(s).unwrap()
    }

    fn custom() -> ViewPrefs {
        ViewPrefs {
            sort: SortKey::Modified,
            descending: true,
            folders_first: false,
            show_hidden: true,
            view_mode: ViewMode::Grid,
            thumbnails: false,
        }
    }

    #[test]
    fn falls_back_to_global_defaults() {
        let s = store();
        assert_eq!(
            s.get(&uri("lautta://user-documents/a")).unwrap(),
            ViewPrefs::default()
        );
        let d = custom();
        s.set_defaults(d);
        assert_eq!(s.defaults(), d);
        assert_eq!(s.get(&uri("lautta://user-documents/a")).unwrap(), d);
    }

    #[test]
    fn folder_then_root_then_defaults() {
        let s = store();
        let root = uri("lautta://user-documents/");
        let folder = uri("lautta://user-documents/Photos");
        let other = uri("lautta://user-documents/Other");
        let root_prefs = ViewPrefs {
            sort: SortKey::Size,
            ..ViewPrefs::default()
        };
        s.set(&root, &root_prefs).unwrap();
        assert_eq!(s.get(&folder).unwrap(), root_prefs);
        s.set(&folder, &custom()).unwrap();
        assert_eq!(s.get(&folder).unwrap(), custom());
        assert_eq!(s.get(&other).unwrap(), root_prefs);
        // Another location is unaffected.
        assert_eq!(
            s.get(&uri("lautta://user-music/x")).unwrap(),
            ViewPrefs::default()
        );
        s.clear(&folder).unwrap();
        assert_eq!(s.get(&folder).unwrap(), root_prefs);
        assert_eq!(s.get_exact(&folder).unwrap(), None);
        s.clear(&root).unwrap();
        assert_eq!(s.get(&folder).unwrap(), ViewPrefs::default());
    }

    #[test]
    fn set_overwrites_and_odd_paths_round_trip() {
        let s = store();
        let u = Uri::new("nv-acc:1", crate::vpath::VPath::parse(b"a b/caf\xe9").unwrap());
        s.set(&u, &ViewPrefs::default()).unwrap();
        s.set(&u, &custom()).unwrap();
        assert_eq!(s.get_exact(&u).unwrap(), Some(custom()));
    }

    #[test]
    fn null_or_garbage_columns_use_defaults() {
        let s = store();
        let u = uri("lautta://user-documents/x");
        s.db.lock()
            .execute(
                "INSERT INTO view_prefs(uri, sort_key, view_mode, show_hidden) VALUES (?1, 'bogus', 'grid', 1)",
                params![u.to_string()],
            )
            .unwrap();
        let p = s.get(&u).unwrap();
        assert_eq!(p.sort, SortKey::Name);
        assert_eq!(p.view_mode, ViewMode::Grid);
        assert!(p.show_hidden);
        assert!(p.folders_first);
        assert!(p.thumbnails);
        assert!(!p.descending);
    }

    #[test]
    fn root_lookup_does_not_recurse() {
        let s = store();
        let root = uri("lautta://user-documents/");
        s.set(&root, &custom()).unwrap();
        assert_eq!(s.get(&root).unwrap(), custom());
    }

    #[test]
    fn sort_options_follow_prefs() {
        let o = custom().sort_options();
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
