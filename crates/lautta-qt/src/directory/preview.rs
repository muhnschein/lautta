// SPDX-License-Identifier: LGPL-2.1-or-later
//! `ViewPreviewModel` (doc/QML-API.md): the sample folder in the preview of
//! the view options page (BRW-4). QML gives the entries and the view
//! settings; the core's `ListingState` orders and filters them, so the
//! preview shows exactly what a real folder would.

use crate::json::from_json;
use lautta_core::entry::{ms_to_system_time, Entry, Kind};
use lautta_core::listing::ListingState;
use lautta_core::mime;
use lautta_core::sort::SortKey;
use lautta_core::viewprefs::ViewPrefs;
use qmetaobject::prelude::*;
use qmetaobject::{QByteArray, QVariantList};
use serde::Deserialize;
use std::collections::HashMap;

const ROLE_BASE: i32 = 0x0100;
const ROLES: [&str; 6] = ["name", "isDir", "size", "modified", "hidden", "category"];

/// One sample entry as QML writes it.
#[derive(Deserialize)]
struct Sample {
    name: String,
    #[serde(default)]
    dir: bool,
    /// Bytes; folders have none.
    size: Option<u64>,
    /// Milliseconds since the epoch.
    modified: Option<i64>,
}

fn to_entry(s: Sample) -> Entry {
    let kind = if s.dir { Kind::Dir } else { Kind::File };
    let mut e = Entry::new(s.name.as_bytes(), kind);
    e.size = s.size.filter(|_| !s.dir);
    e.modified = s.modified.map(ms_to_system_time);
    e
}

/// The entries of `json`, `[{name, dir, size, modified}]`; none when it is
/// not such a list.
fn parse_entries(json: &str) -> Vec<Entry> {
    from_json::<Vec<Sample>>(json)
        .unwrap_or_default()
        .into_iter()
        .map(to_entry)
        .collect()
}

/// `entries` as a folder shows them with the view settings `prefs`.
fn listing(entries: &[Entry], prefs: &ViewPrefs) -> ListingState {
    let mut state = ListingState::from_prefs(prefs);
    state.apply_batch(entries.to_vec());
    state.finish();
    state
}

#[derive(QObject, Default)]
pub struct ViewPreviewModel {
    base: qt_base_class!(trait QAbstractListModel),

    /// JSON list of the sample entries (see `parse_entries`).
    entries: qt_property!(QString; NOTIFY changed WRITE set_entries),
    sortKey: qt_property!(QString; NOTIFY changed WRITE set_sort_key),
    descending: qt_property!(bool; NOTIFY changed WRITE set_descending),
    foldersFirst: qt_property!(bool; NOTIFY changed WRITE set_folders_first),
    showHidden: qt_property!(bool; NOTIFY changed WRITE set_show_hidden),
    /// The names in the order shown.
    names: qt_property!(QVariantList; NOTIFY changed),
    count: qt_property!(i32; NOTIFY changed),
    changed: qt_signal!(),

    samples: Vec<Entry>,
    state: ListingState,
}

impl ViewPreviewModel {
    fn set_entries(&mut self, value: QString) {
        self.samples = parse_entries(&value.to_string());
        self.entries = value;
        self.update();
    }

    fn set_sort_key(&mut self, value: QString) {
        self.sortKey = value;
        self.update();
    }

    fn set_descending(&mut self, value: bool) {
        self.descending = value;
        self.update();
    }

    fn set_folders_first(&mut self, value: bool) {
        self.foldersFirst = value;
        self.update();
    }

    fn set_show_hidden(&mut self, value: bool) {
        self.showHidden = value;
        self.update();
    }

    fn prefs(&self) -> ViewPrefs {
        ViewPrefs {
            sort: SortKey::parse(&self.sortKey.to_string()).unwrap_or_default(),
            descending: self.descending,
            folders_first: self.foldersFirst,
            show_hidden: self.showHidden,
            ..ViewPrefs::default()
        }
    }

    fn update(&mut self) {
        self.begin_reset_model();
        self.state = listing(&self.samples, &self.prefs());
        self.end_reset_model();
        let mut names = QVariantList::default();
        for e in self.state.visible_entries() {
            names.push(QVariant::from(QString::from(e.display_name().as_str())));
        }
        self.names = names;
        self.count = self.state.len() as i32;
        self.changed();
    }
}

fn role_value(e: &Entry, role: usize) -> QVariant {
    match role {
        0 => QString::from(e.display_name().as_str()).into(),
        1 => e.is_dir().into(),
        2 => QVariant::from(e.size.map_or(-1.0, |s| s as f64)),
        3 => QVariant::from(e.modified_ms().unwrap_or(-1) as f64),
        4 => e.is_hidden().into(),
        5 => QString::from(mime::category_of(e).icon_name()).into(),
        _ => QVariant::default(),
    }
}

impl QAbstractListModel for ViewPreviewModel {
    fn row_count(&self) -> i32 {
        self.state.len() as i32
    }

    fn data(&self, index: QModelIndex, role: i32) -> QVariant {
        let (Ok(row), Ok(r)) = (usize::try_from(index.row()), usize::try_from(role - ROLE_BASE)) else {
            return QVariant::default();
        };
        self.state
            .entry_at(row)
            .map(|e| role_value(e, r))
            .unwrap_or_default()
    }

    fn role_names(&self) -> HashMap<i32, QByteArray> {
        ROLES
            .iter()
            .enumerate()
            .map(|(i, n)| (ROLE_BASE + i as i32, QByteArray::from(*n)))
            .collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const SAMPLES: &str = r#"[
        {"name": "Work", "dir": true, "modified": 5000},
        {"name": "photo.jpg", "size": 2400000, "modified": 3000},
        {"name": "notes.txt", "size": 3000, "modified": 4000},
        {"name": ".backup", "size": 12000, "modified": 1000},
        {"name": "Photos", "dir": true, "size": 99, "modified": 2000}
    ]"#;

    fn order(prefs: ViewPrefs) -> Vec<String> {
        listing(&parse_entries(SAMPLES), &prefs)
            .visible_entries()
            .map(Entry::display_name)
            .collect()
    }

    #[test]
    fn samples_are_parsed() {
        let entries = parse_entries(SAMPLES);
        assert_eq!(entries.len(), 5);
        let photos = &entries[4];
        assert!(photos.is_dir() && photos.size.is_none(), "folders have no size");
        assert_eq!(
            (entries[1].size, entries[1].modified_ms()),
            (Some(2_400_000), Some(3000))
        );
        assert!(entries[3].is_hidden());
        assert!(parse_entries("not json").is_empty());
        assert!(parse_entries(r#"{"name": "x"}"#).is_empty());
    }

    #[test]
    fn samples_are_ordered_like_a_folder() {
        let base = ViewPrefs::default();
        assert_eq!(order(base), ["Photos", "Work", "notes.txt", "photo.jpg"]);
        let by_size = ViewPrefs {
            sort: SortKey::Size,
            descending: true,
            ..base
        };
        // Descending turns the sizes round; folders, which have none, stay
        // first in name order.
        assert_eq!(order(by_size), ["Photos", "Work", "photo.jpg", "notes.txt"]);
        let mixed = ViewPrefs {
            sort: SortKey::Modified,
            folders_first: false,
            show_hidden: true,
            ..base
        };
        assert_eq!(
            order(mixed),
            [".backup", "Photos", "photo.jpg", "notes.txt", "Work"]
        );
    }

    #[test]
    fn roles_carry_the_raw_values() {
        let state = listing(&parse_entries(SAMPLES), &ViewPrefs::default());
        let folder = state.entry_at(0).unwrap();
        let image = state.entry_at(3).unwrap();
        assert_eq!(role_value(folder, 0).to_qstring().to_string(), "Photos");
        assert!(role_value(folder, 1).to_bool());
        assert_eq!(
            role_value(folder, 2),
            QVariant::from(-1.0),
            "folders have no size"
        );
        assert_eq!(role_value(image, 2), QVariant::from(2_400_000.0));
        assert_eq!(role_value(image, 3), QVariant::from(3000.0));
        assert!(!role_value(image, 4).to_bool());
        assert_eq!(role_value(image, 5).to_qstring().to_string(), "image");
        assert!(!role_value(image, 6).is_valid());
    }
}
