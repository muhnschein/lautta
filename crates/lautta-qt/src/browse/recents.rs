// SPDX-License-Identifier: LGPL-2.1-or-later
//! `RecentsModel` (ORG-2): files opened, previewed or transferred,
//! newest first, filtered by kind and name. Roles: `id`, `uri`, `name`,
//! `kind`, `place`, `at` (ms since the epoch), `day` (`today`, `yesterday`,
//! `week`, `earlier`).

use super::{cell_data, error_parts, role_names, store_changed, Cell};
use crate::runtime::{blocking_then, core};
use lautta_core::app::Core;
use lautta_core::app_browse::RecentRow;
use lautta_core::org::{RecentKind, RecentsFilter};
use qmetaobject::prelude::*;
use qmetaobject::QPointer;
use std::collections::HashMap;

const COLUMNS: [&str; 7] = ["itemId", "uri", "name", "kind", "place", "at", "day"];

/// The core filter for QML's `kindFilter` (empty or unknown: everything)
/// and `text`.
fn filter_of(kind: &str, text: &str) -> RecentsFilter {
    RecentsFilter {
        kinds: RecentKind::parse(kind).into_iter().collect(),
        text: Some(text.trim().to_owned()).filter(|t| !t.is_empty()),
        limit: None,
    }
}

fn cells(r: RecentRow) -> Vec<Cell> {
    vec![
        Cell::Int(r.id),
        Cell::Str(r.uri),
        Cell::Str(r.name),
        Cell::s(r.kind),
        Cell::Str(r.place),
        Cell::Int(r.at_ms),
        Cell::s(r.day),
    ]
}

#[derive(QObject, Default)]
pub struct RecentsModel {
    base: qt_base_class!(trait QAbstractListModel),
    /// `""` (everything), `opened`, `previewed` or `transferred`.
    kindFilter: qt_property!(QString; NOTIFY filterChanged WRITE set_kind_filter),
    text: qt_property!(QString; NOTIFY filterChanged WRITE set_text),
    filterChanged: qt_signal!(),
    count: qt_property!(i32; NOTIFY countChanged),
    /// The setting `recents_enabled` as of the last reload.
    enabled: qt_property!(bool; NOTIFY countChanged),
    loaded: qt_property!(bool; NOTIFY countChanged),
    countChanged: qt_signal!(),

    reload: qt_method!(fn(&mut self)),
    clear: qt_method!(fn(&mut self)),
    remove: qt_method!(fn(&mut self, id: i32)),
    failed: qt_signal!(kind: QString, message: QString),

    rows: Vec<Vec<Cell>>,
    generation: u64,
}

impl RecentsModel {
    fn set_kind_filter(&mut self, kind: QString) {
        if self.kindFilter != kind {
            self.kindFilter = kind;
            self.filterChanged();
            self.reload();
        }
    }

    fn set_text(&mut self, text: QString) {
        if self.text != text {
            self.text = text;
            self.filterChanged();
            self.reload();
        }
    }

    fn reload(&mut self) {
        let Some(core) = core() else { return };
        self.generation += 1;
        let generation = self.generation;
        let filter = filter_of(&self.kindFilter.to_string(), &self.text.to_string());
        let me = QPointer::from(&*self);
        blocking_then(
            move || (core.recent_rows(&filter), core.recents.is_enabled()),
            move |(res, enabled)| {
                let Some(model) = me.as_pinned() else { return };
                let mut model = model.borrow_mut();
                if model.generation != generation {
                    return;
                }
                match res {
                    Ok(rows) => {
                        model.begin_reset_model();
                        model.rows = rows.into_iter().map(cells).collect();
                        model.end_reset_model();
                        model.count = model.rows.len() as i32;
                    }
                    Err(e) => {
                        let (kind, message) = error_parts(&e);
                        model.failed(kind, message);
                    }
                }
                model.enabled = enabled;
                model.loaded = true;
                model.countChanged();
            },
        );
    }

    fn change(&mut self, work: impl FnOnce(&Core) -> lautta_core::Result<()> + Send + 'static) {
        let Some(core) = core() else { return };
        let me = QPointer::from(&*self);
        blocking_then(
            move || work(&core),
            move |res| {
                let Some(model) = me.as_pinned() else { return };
                let mut model = model.borrow_mut();
                if let Err(e) = res {
                    let (kind, message) = error_parts(&e);
                    model.failed(kind, message);
                }
                store_changed();
                model.reload();
            },
        );
    }

    fn clear(&mut self) {
        self.change(|core| core.recents.clear().map(|_| ()));
    }

    fn remove(&mut self, id: i32) {
        self.change(move |core| core.recents.remove(i64::from(id)).map(|_| ()));
    }
}

impl QAbstractListModel for RecentsModel {
    fn row_count(&self) -> i32 {
        self.rows.len() as i32
    }

    fn data(&self, index: QModelIndex, role: i32) -> QVariant {
        cell_data(&self.rows, index.row(), role)
    }

    fn role_names(&self) -> HashMap<i32, QByteArray> {
        role_names(&COLUMNS)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn filters_read_kind_and_text() {
        let f = filter_of("previewed", "  Notes ");
        assert_eq!(f.kinds, vec![RecentKind::Previewed]);
        assert_eq!(f.text.as_deref(), Some("Notes"));
        let all = filter_of("", "");
        assert!(all.kinds.is_empty());
        assert_eq!(all.text, None);
        assert!(filter_of("bogus", "").kinds.is_empty());
    }

    #[test]
    fn rows_match_the_columns() {
        let row = RecentRow {
            id: 1,
            uri: "lautta://a/b".into(),
            name: "b".into(),
            kind: "opened",
            place: "A".into(),
            at_ms: 5,
            day: "today",
        };
        assert_eq!(cells(row).len(), COLUMNS.len());
    }
}
