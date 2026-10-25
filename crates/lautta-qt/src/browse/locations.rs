// SPDX-License-Identifier: LGPL-2.1-or-later
//! `LocationsModel`: the rows of the Browse page (SPEC §15.2).
//!
//! Roles: `section`, `uri`, `name`, `kind`, `icon`, `status`, `attention`,
//! `colour`, `itemId`, `place`, `provider`, `host`, `free`, `total`, `fs`,
//! `connection`, `connectionError`, `isFile`, `drive` (see
//! `lautta_core::app_browse::Row` for the values).

use super::{cell_data, role_names, watch_sources, Cell, Guard};
use crate::runtime::{core, spawn_then};
use lautta_core::app_browse::Row;
use qmetaobject::prelude::*;
use qmetaobject::QPointer;
use std::collections::HashMap;

const COLUMNS: [&str; 19] = [
    "section",
    "uri",
    "name",
    "kind",
    "icon",
    "status",
    "attention",
    "colour",
    "itemId",
    "place",
    "provider",
    "host",
    "free",
    "total",
    "fs",
    "connection",
    "connectionError",
    "isFile",
    "drive",
];

fn cells(r: &Row) -> Vec<Cell> {
    vec![
        Cell::s(r.section),
        Cell::s(&r.uri),
        Cell::s(&r.name),
        Cell::s(r.kind),
        Cell::s(&r.icon),
        Cell::s(r.status),
        Cell::s(&r.attention),
        Cell::s(&r.colour),
        Cell::s(&r.item_id),
        Cell::s(&r.place),
        Cell::s(&r.provider),
        Cell::s(&r.host),
        Cell::Int(r.free),
        Cell::Int(r.total),
        Cell::s(&r.fs),
        Cell::s(r.connection),
        Cell::s(&r.connection_error),
        Cell::Bool(r.is_file),
        Cell::s(&r.drive),
    ]
}

fn key_of(r: &Row) -> String {
    let (s, k, u, i) = r.key();
    format!("{s}\u{1}{k}\u{1}{u}\u{1}{i}")
}

#[derive(QObject, Default)]
pub struct LocationsModel {
    base: qt_base_class!(trait QAbstractListModel),
    count: qt_property!(i32; NOTIFY countChanged),
    /// Recent ad-hoc servers and nearby servers, for the connect form.
    recentCount: qt_property!(i32; NOTIFY countChanged),
    nearbyCount: qt_property!(i32; NOTIFY countChanged),
    countChanged: qt_signal!(),
    refresh: qt_method!(fn(&mut self)),
    sectionCount: qt_method!(fn(&self, section: QString) -> i32),

    rows: Vec<Vec<Cell>>,
    keys: Vec<String>,
    sections: Vec<String>,
    watching: bool,
    generation: u64,
    guard: Guard,
}

impl LocationsModel {
    /// Reads the rows again; the first call also starts following changes of
    /// the locations, the bridge and the stores.
    fn refresh(&mut self) {
        let Some(core) = core() else { return };
        if !self.watching {
            self.watching = true;
            watch_sources(QPointer::from(&*self), &core, &self.guard, Self::refresh);
        }
        self.generation += 1;
        let generation = self.generation;
        let me = QPointer::from(&*self);
        spawn_then(async move { core.browse_rows().await }, move |rows| {
            if let Some(model) = me.as_pinned() {
                let mut model = model.borrow_mut();
                if model.generation == generation {
                    model.apply(rows);
                }
            }
        });
    }

    fn sectionCount(&self, section: QString) -> i32 {
        let section = section.to_string();
        self.sections.iter().filter(|s| **s == section).count() as i32
    }

    /// Shows `rows`. Rows that keep their identity stay (an open context
    /// menu or a row remorse survives); only the rows in between the
    /// unchanged start and end of the list are removed and inserted.
    fn apply(&mut self, rows: Vec<Row>) {
        let keys: Vec<String> = rows.iter().map(key_of).collect();
        let mut cells: Vec<Vec<Cell>> = rows.iter().map(cells).collect();
        self.sections = rows.iter().map(|r| r.section.to_owned()).collect();
        self.recentCount = rows.iter().filter(|r| r.kind == "adhocRecent").count() as i32;
        self.nearbyCount = rows.iter().filter(|r| r.kind == "nearby").count() as i32;
        let (start, removed, inserted) = splice(&self.keys, &keys);
        if removed > 0 {
            self.begin_remove_rows(start as i32, (start + removed - 1) as i32);
            self.rows.drain(start..start + removed);
            self.end_remove_rows();
        }
        if inserted > 0 {
            self.begin_insert_rows(start as i32, (start + inserted - 1) as i32);
            let new: Vec<Vec<Cell>> = cells.drain(start..start + inserted).collect();
            self.rows.splice(start..start, new);
            self.end_insert_rows();
        }
        self.keys = keys;
        // What is left in `cells` are the rows that stayed, in order.
        let kept: Vec<usize> = (0..self.rows.len())
            .filter(|i| *i < start || *i >= start + inserted)
            .collect();
        for (i, row) in kept.into_iter().zip(cells) {
            if self.rows[i] != row {
                self.rows[i] = row;
                let index = self.row_index(i as i32);
                self.data_changed(index, index);
            }
        }
        self.count = self.rows.len() as i32;
        self.countChanged();
    }
}

/// How `old` turns into `new`: from `start`, `removed` rows go and
/// `inserted` rows come; the rows before and after keep their identity.
fn splice(old: &[String], new: &[String]) -> (usize, usize, usize) {
    let start = old.iter().zip(new).take_while(|(a, b)| a == b).count();
    let end = old[start..]
        .iter()
        .rev()
        .zip(new[start..].iter().rev())
        .take_while(|(a, b)| a == b)
        .count();
    (start, old.len() - start - end, new.len() - start - end)
}

impl QAbstractListModel for LocationsModel {
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
    fn cells_match_the_columns() {
        let cells = cells(&Row::default());
        assert_eq!(cells.len(), COLUMNS.len());
    }

    #[test]
    fn no_row_shows_an_item_count() {
        assert!(!COLUMNS.contains(&"count"), "Browse shows no counts (SPEC §15.2)");
    }

    #[test]
    fn a_server_row_tells_its_connection() {
        let row = Row {
            kind: "account",
            connection: "failed",
            connection_error: "Timeout".into(),
            ..Row::default()
        };
        let cells = cells(&row);
        let at = |name: &str| &cells[COLUMNS.iter().position(|c| *c == name).unwrap()];
        assert_eq!(at("connection"), &Cell::s("failed"));
        assert_eq!(at("connectionError"), &Cell::s("Timeout"));
        assert_eq!(at("kind"), &Cell::s("account"));
    }

    #[test]
    fn a_volume_row_names_its_drive() {
        let row = Row {
            kind: "volume",
            status: "unmounted",
            drive: "mmcblk1p1".into(),
            ..Row::default()
        };
        let cells = cells(&row);
        let at = |name: &str| &cells[COLUMNS.iter().position(|c| *c == name).unwrap()];
        assert_eq!(at("drive"), &Cell::s("mmcblk1p1"));
        assert_eq!(at("status"), &Cell::s("unmounted"));
    }

    fn keys(names: &[&str]) -> Vec<String> {
        names.iter().map(|n| (*n).to_owned()).collect()
    }

    #[test]
    fn splice_keeps_the_rows_around_a_change() {
        let old = keys(&["fav1", "fav2", "docs", "nas"]);
        assert_eq!(splice(&old, &old), (4, 0, 0));
        assert_eq!(splice(&old, &keys(&["fav2", "docs", "nas"])), (0, 1, 0));
        assert_eq!(
            splice(&old, &keys(&["fav1", "fav2", "fav3", "docs", "nas"])),
            (2, 0, 1)
        );
        assert_eq!(
            splice(&old, &keys(&["fav1", "fav2", "docs", "nas", "near0"])),
            (4, 0, 1)
        );
        assert_eq!(splice(&old, &keys(&["fav1", "x", "docs", "nas"])), (1, 1, 1));
        assert_eq!(splice(&[], &keys(&["a", "b"])), (0, 0, 2));
        assert_eq!(splice(&keys(&["a", "b"]), &[]), (0, 2, 0));
        // A repeated key is counted once, from the start.
        assert_eq!(splice(&keys(&["a", "a"]), &keys(&["a"])), (1, 1, 0));
    }

    #[test]
    fn keys_tell_rows_apart() {
        let a = Row {
            kind: "favourite",
            item_id: "1".into(),
            ..Row::default()
        };
        let b = Row {
            item_id: "2".into(),
            ..a.clone()
        };
        assert_ne!(key_of(&a), key_of(&b));
        assert_eq!(key_of(&a), key_of(&a.clone()));
    }
}
