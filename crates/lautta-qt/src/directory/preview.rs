// SPDX-License-Identifier: LGPL-2.1-or-later
//! The sample folder in the preview of the view options page (BRW-4),
//! behind `App.previewRows` (doc/QML-API.md). QML gives the entries and the
//! view settings; the core's `ListingState` orders and filters them, so the
//! preview shows exactly what a real folder would.

use crate::json::from_json;
use lautta_core::entry::{ms_to_system_time, Entry, Kind};
use lautta_core::listing::ListingState;
use lautta_core::mime;
use lautta_core::sort::SortKey;
use lautta_core::viewprefs::ViewPrefs;
use qmetaobject::{QString, QVariant, QVariantList, QVariantMap};
use serde::Deserialize;

/// The fields of a row, in the order of `role_value`'s numbers.
const FIELDS: [&str; 6] = ["name", "isDir", "size", "modified", "hidden", "category"];

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

/// The view settings the preview is asked for; an unknown sort key sorts
/// by name, as a folder does.
pub(crate) fn prefs(sort_key: &str, descending: bool, folders_first: bool, show_hidden: bool) -> ViewPrefs {
    ViewPrefs {
        sort: SortKey::parse(sort_key).unwrap_or_default(),
        descending,
        folders_first,
        show_hidden,
        ..ViewPrefs::default()
    }
}

/// The entries of `entries_json` as a folder shows them with `prefs`: one
/// map of `FIELDS` per shown entry, in order.
pub(crate) fn rows(entries_json: &str, prefs: &ViewPrefs) -> QVariantList {
    let mut out = QVariantList::default();
    for e in listing(&parse_entries(entries_json), prefs).visible_entries() {
        out.push(QVariant::from(row(e)));
    }
    out
}

fn row(e: &Entry) -> QVariantMap {
    let mut map = QVariantMap::default();
    for (i, field) in FIELDS.iter().enumerate() {
        map.insert(QString::from(*field), role_value(e, i));
    }
    map
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

    #[test]
    fn rows_are_maps_of_the_fields_in_order() {
        let key = |k: &str| QString::from(k);
        let wanted = prefs("size", true, true, false);
        assert_eq!(wanted.sort, SortKey::Size);
        assert!(wanted.descending && wanted.folders_first && !wanted.show_hidden);
        assert_eq!(prefs("nonsense", false, false, true).sort, SortKey::Name);
        assert_eq!(rows(SAMPLES, &wanted).len(), 4, "no hidden file");
        assert_eq!(rows(SAMPLES, &prefs("name", false, true, true)).len(), 5);
        assert_eq!(rows("not json", &wanted).len(), 0);
        let state = listing(&parse_entries(SAMPLES), &wanted);
        let image = row(state.entry_at(2).unwrap());
        assert_eq!(image.len(), FIELDS.len());
        let field = |k: &str| image.value(key(k), QVariant::default());
        assert_eq!(field("name").to_qstring().to_string(), "photo.jpg");
        assert!(!field("isDir").to_bool() && !field("hidden").to_bool());
        assert_eq!(field("size"), QVariant::from(2_400_000.0));
        assert_eq!(field("modified"), QVariant::from(3000.0));
        assert_eq!(field("category").to_qstring().to_string(), "image");
    }
}
