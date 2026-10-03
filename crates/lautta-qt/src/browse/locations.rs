// SPDX-License-Identifier: LGPL-2.1-or-later
//! `LocationsModel`: the rows of the Browse page (SPEC §15.2).
//!
//! Roles: `section`, `uri`, `name`, `kind`, `icon`, `status`, `attention`,
//! `colour`, `count`, `itemId`, `place`, `provider`, `host`, `free`,
//! `total`, `fs`, `leftUri`, `rightUri`, `mode` (see
//! `lautta_core::app_browse::Row` for the values).

use super::{cell_data, role_names, store_subscribe, Cell, Guard};
use crate::runtime::{core, handle, spawn_then};
use lautta_core::app::Core;
use lautta_core::app_browse::Row;
use qmetaobject::prelude::*;
use qmetaobject::{queued_callback, QPointer};
use std::collections::HashMap;
use std::sync::Arc;
use std::time::Duration;

const COLUMNS: [&str; 19] = [
    "section",
    "uri",
    "name",
    "kind",
    "icon",
    "status",
    "attention",
    "colour",
    "count",
    "itemId",
    "place",
    "provider",
    "host",
    "free",
    "total",
    "fs",
    "leftUri",
    "rightUri",
    "mode",
];

/// Changes that arrive together are applied together.
const SETTLE: Duration = Duration::from_millis(80);

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
        Cell::Int(r.count),
        Cell::s(&r.item_id),
        Cell::s(&r.place),
        Cell::s(&r.provider),
        Cell::s(&r.host),
        Cell::Int(r.free),
        Cell::Int(r.total),
        Cell::s(&r.fs),
        Cell::s(&r.left_uri),
        Cell::s(&r.right_uri),
        Cell::s(&r.mode),
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
            self.watch(core.clone());
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

    fn watch(&mut self, core: Arc<Core>) {
        let me = QPointer::from(&*self);
        let notify = queued_callback(move |()| {
            if let Some(model) = me.as_pinned() {
                model.borrow_mut().refresh();
            }
        });
        let mut stop = self.guard.subscribe();
        handle().spawn(async move {
            let mut locations = core.locations.subscribe();
            let mut store = store_subscribe();
            let mut status = core.bridge.as_ref().map(|b| b.watch_status());
            let mut nearby = core.bridge.as_ref().map(|b| b.watch_nearby());
            loop {
                tokio::select! {
                    r = locations.changed() => if r.is_err() { return },
                    r = store.changed() => if r.is_err() { return },
                    ok = changed(&mut status) => if !ok { return },
                    ok = changed_nearby(&mut nearby) => if !ok { return },
                    _ = stop.changed() => return,
                }
                tokio::time::sleep(SETTLE).await;
                locations.borrow_and_update();
                store.borrow_and_update();
                notify(());
            }
        });
    }

    /// Shows `rows`: rows with the same identity update in place (an open
    /// context menu survives a status change), anything else resets.
    fn apply(&mut self, rows: Vec<Row>) {
        let keys: Vec<String> = rows.iter().map(key_of).collect();
        let cells: Vec<Vec<Cell>> = rows.iter().map(cells).collect();
        self.sections = rows.iter().map(|r| r.section.to_owned()).collect();
        self.recentCount = rows.iter().filter(|r| r.kind == "adhocRecent").count() as i32;
        self.nearbyCount = rows.iter().filter(|r| r.kind == "nearby").count() as i32;
        if keys == self.keys {
            let changed: Vec<usize> = (0..cells.len()).filter(|i| cells[*i] != self.rows[*i]).collect();
            self.rows = cells;
            self.countChanged();
            for i in changed {
                let index = self.row_index(i as i32);
                self.data_changed(index, index);
            }
            return;
        }
        self.begin_reset_model();
        self.keys = keys;
        self.rows = cells;
        self.end_reset_model();
        self.count = self.rows.len() as i32;
        self.countChanged();
    }
}

/// Waits for the next bridge status change; false when the bridge is gone.
/// Never completes without a bridge.
async fn changed(rx: &mut Option<tokio::sync::watch::Receiver<lautta_core::bridge::BridgeStatus>>) -> bool {
    match rx {
        Some(rx) => rx.changed().await.is_ok(),
        None => std::future::pending().await,
    }
}

async fn changed_nearby(
    rx: &mut Option<tokio::sync::watch::Receiver<Arc<Vec<lautta_core::bridge::NearbyServer>>>>,
) -> bool {
    match rx {
        Some(rx) => rx.changed().await.is_ok(),
        None => std::future::pending().await,
    }
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
    fn keys_tell_rows_apart() {
        let a = Row {
            kind: "tag",
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
