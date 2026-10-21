// SPDX-License-Identifier: LGPL-2.1-or-later
//! `RecentsModel` (ORG-2): files opened, previewed or transferred,
//! newest first. Roles: `itemId`, `uri`, `name`, `kind`, `place`, `at` (ms
//! since the epoch), `dateGroup` (the Activity page's date sections, §15.4:
//! `today`, `yesterday`, `last7Days`, `last30Days`, `older`).

use super::{cell_data, error_parts, role_names, store_changed, Cell};
use crate::runtime::{blocking_then, core};
use lautta_core::app::Core;
use lautta_core::app_browse::RecentRow;
use lautta_core::org::RecentsFilter;
use qmetaobject::prelude::*;
use qmetaobject::QPointer;
use std::collections::HashMap;

const COLUMNS: [&str; 7] = ["itemId", "uri", "name", "kind", "place", "at", "dateGroup"];

fn cells(r: RecentRow) -> Vec<Cell> {
    vec![
        Cell::Int(r.id),
        Cell::Str(r.uri),
        Cell::Str(r.name),
        Cell::s(r.kind),
        Cell::Str(r.place),
        Cell::Int(r.at_ms),
        Cell::s(r.date_group.name()),
    ]
}

#[derive(QObject, Default)]
pub struct RecentsModel {
    base: qt_base_class!(trait QAbstractListModel),
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
    fn reload(&mut self) {
        let Some(core) = core() else { return };
        self.generation += 1;
        let generation = self.generation;
        let me = QPointer::from(&*self);
        blocking_then(
            move || {
                (
                    core.recent_rows(&RecentsFilter::default()),
                    core.recents.is_enabled(),
                )
            },
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
    use lautta_core::date_group::DateGroup;

    #[test]
    fn rows_match_the_columns() {
        let row = RecentRow {
            id: 1,
            uri: "lautta://a/b".into(),
            name: "b".into(),
            kind: "opened",
            place: "A".into(),
            at_ms: 5,
            date_group: DateGroup::LastThirtyDays,
        };
        let rows = vec![cells(row)];
        assert_eq!(rows[0].len(), COLUMNS.len());
        let (role, _) = role_names(&COLUMNS)
            .into_iter()
            .find(|(_, name)| name.to_string() == "dateGroup")
            .unwrap();
        assert_eq!(cell_data(&rows, 0, role).to_qstring().to_string(), "last30Days");
    }
}
