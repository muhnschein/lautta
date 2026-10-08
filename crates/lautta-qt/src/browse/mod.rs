// SPDX-License-Identifier: LGPL-2.1-or-later
//! browse area facades (doc/QML-API.md): the Browse page's locations, the
//! `Bridge` singleton, favourites, recents and per-location settings.
//! Logic lives in `lautta_core::app_browse`; this layer maps it to QML.

mod bridge;
mod favourites;
mod folderinfo;
mod locations;
mod prefs;
mod recents;

use cpp::cpp;
use qmetaobject::prelude::*;
use std::collections::HashMap;
use std::sync::OnceLock;
use tokio::sync::watch;

/// A model cell; QML sees plain strings and numbers.
#[derive(Debug, Clone, PartialEq)]
pub(crate) enum Cell {
    Str(String),
    Int(i64),
}

impl Cell {
    pub(crate) fn s(v: &str) -> Cell {
        Cell::Str(v.to_owned())
    }

    fn variant(&self) -> QVariant {
        match self {
            Cell::Str(s) => QVariant::from(QString::from(s.as_str())),
            Cell::Int(i) => QVariant::from(*i),
        }
    }
}

cpp! {{
    #include <QtCore/QCoreApplication>
    #include <QtCore/QEventLoop>
}}

/// Runs the Qt event loop for `ms` milliseconds. For tests that drive a
/// scenario from a synchronous script (the host has no widgets to run an
/// engine with `exec`).
pub fn pump_events(ms: i32) {
    let until = std::time::Instant::now() + std::time::Duration::from_millis(u64::try_from(ms).unwrap_or(0));
    while std::time::Instant::now() < until {
        cpp!(unsafe [] {
            QCoreApplication::processEvents(QEventLoop::AllEvents, 5);
        });
        std::thread::sleep(std::time::Duration::from_millis(2));
    }
}

/// First role id of the models of this area (`Qt::UserRole`).
const FIRST_ROLE: i32 = 0x100;

/// Role ids and names for a column list; the role of column `i` is
/// `FIRST_ROLE + i`.
pub(crate) fn role_names(names: &[&str]) -> HashMap<i32, QByteArray> {
    names
        .iter()
        .enumerate()
        .map(|(i, n)| (FIRST_ROLE + i as i32, QByteArray::from(*n)))
        .collect()
}

/// The value of `role` in row `row` of a table of cells.
pub(crate) fn cell_data(rows: &[Vec<Cell>], row: i32, role: i32) -> QVariant {
    let Ok(row) = usize::try_from(row) else {
        return QVariant::default();
    };
    let Ok(col) = usize::try_from(role - FIRST_ROLE) else {
        return QVariant::default();
    };
    rows.get(row)
        .and_then(|r| r.get(col))
        .map(Cell::variant)
        .unwrap_or_default()
}

/// Ends the background tasks of a facade when it is dropped: they wait on
/// the receiver of [`Guard::subscribe`], which then reports closure.
pub(crate) struct Guard(watch::Sender<()>);

impl Default for Guard {
    fn default() -> Self {
        Guard(watch::channel(()).0)
    }
}

impl Guard {
    pub(crate) fn subscribe(&self) -> watch::Receiver<()> {
        self.0.subscribe()
    }
}

static STORE: OnceLock<watch::Sender<u64>> = OnceLock::new();

fn store_channel() -> &'static watch::Sender<u64> {
    STORE.get_or_init(|| watch::channel(0).0)
}

/// Tells the models of this area that favourites or recents changed
/// (the stores have no change notification of their own).
pub(crate) fn store_changed() {
    store_channel().send_modify(|g| *g += 1);
}

pub(crate) fn store_subscribe() -> watch::Receiver<u64> {
    store_channel().subscribe()
}

/// `{kind, message}` as the failure signals carry it.
pub(crate) fn error_parts(e: &lautta_core::Error) -> (QString, QString) {
    (QString::from(e.kind.name()), QString::from(e.message.as_str()))
}

/// Registers this area's QML types under `Lautta 1.0`.
pub fn register() {
    let uri = crate::qml_uri();
    qmetaobject::qml_register_singleton_type::<bridge::Bridge>(&uri, 1, 0, &crate::cstr("Bridge"));
    qmetaobject::qml_register_type::<locations::LocationsModel>(&uri, 1, 0, &crate::cstr("LocationsModel"));
    qmetaobject::qml_register_type::<favourites::FavouritesModel>(
        &uri,
        1,
        0,
        &crate::cstr("FavouritesModel"),
    );
    qmetaobject::qml_register_type::<recents::RecentsModel>(&uri, 1, 0, &crate::cstr("RecentsModel"));
    qmetaobject::qml_register_type::<folderinfo::FolderInfo>(&uri, 1, 0, &crate::cstr("FolderInfo"));
    qmetaobject::qml_register_type::<prefs::LocationPrefsModel>(
        &uri,
        1,
        0,
        &crate::cstr("LocationPrefsModel"),
    );
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn roles_are_numbered_from_the_user_role() {
        let names = role_names(&["a", "b"]);
        assert_eq!(names[&FIRST_ROLE].to_string(), "a");
        assert_eq!(names[&(FIRST_ROLE + 1)].to_string(), "b");
    }

    #[test]
    fn cells_become_plain_values() {
        let rows = vec![vec![Cell::s("x"), Cell::Int(7)]];
        assert_eq!(cell_data(&rows, 0, FIRST_ROLE).to_qstring().to_string(), "x");
        assert_eq!(cell_data(&rows, 0, FIRST_ROLE + 1).to_int(), 7);
        assert!(!cell_data(&rows, 1, FIRST_ROLE).is_valid());
        assert!(!cell_data(&rows, 0, FIRST_ROLE + 2).is_valid());
        assert!(!cell_data(&rows, -1, FIRST_ROLE).is_valid());
        assert!(!cell_data(&rows, 0, 3).is_valid());
    }

    #[test]
    fn store_changes_are_signalled() {
        let mut rx = store_subscribe();
        store_changed();
        assert!(rx.has_changed().unwrap());
        rx.borrow_and_update();
        assert!(!rx.has_changed().unwrap());
    }
}
