// SPDX-License-Identifier: LGPL-2.1-or-later
//! Facades over `lautta_core::app_browse`.

mod bridge;
mod favourites;
mod folderinfo;
mod layout;
mod locations;
mod recents;

use crate::runtime::handle;
use cpp::cpp;
use lautta_core::app::Core;
use qmetaobject::prelude::*;
use qmetaobject::{queued_callback, QPointer};
use std::collections::HashMap;
use std::sync::{Arc, OnceLock};
use std::time::Duration;
use tokio::sync::watch;

#[derive(Debug, Clone, PartialEq)]
pub(crate) enum Cell {
    Str(String),
    Int(i64),
    Bool(bool),
}

impl Cell {
    pub(crate) fn s(v: &str) -> Cell {
        Cell::Str(v.to_owned())
    }

    fn variant(&self) -> QVariant {
        match self {
            Cell::Str(s) => QVariant::from(QString::from(s.as_str())),
            Cell::Int(i) => QVariant::from(*i),
            Cell::Bool(b) => QVariant::from(*b),
        }
    }
}

cpp! {{
    #include <QtCore/QCoreApplication>
    #include <QtCore/QEvent>
    #include <QtCore/QEventLoop>
}}

/// Runs the Qt event loop for `ms`. For tests driving a scenario from a synchronous script (host
/// has no widgets to `exec` an engine).
pub fn pump_events(ms: i32) {
    let until = std::time::Instant::now() + std::time::Duration::from_millis(u64::try_from(ms).unwrap_or(0));
    while std::time::Instant::now() < until {
        cpp!(unsafe [] {
            QCoreApplication::processEvents(QEventLoop::AllEvents, 5);
            QCoreApplication::sendPostedEvents(nullptr, QEvent::DeferredDelete);
        });
        std::thread::sleep(std::time::Duration::from_millis(2));
    }
}

const FIRST_ROLE: i32 = 0x100;

pub(crate) fn role_names(names: &[&str]) -> HashMap<i32, QByteArray> {
    names
        .iter()
        .enumerate()
        .map(|(i, n)| (FIRST_ROLE + i as i32, QByteArray::from(*n)))
        .collect()
}

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

/// Ends a facade's background tasks on drop: they wait on [`Guard::subscribe`]'s receiver.
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

/// Stores have no change notification of their own.
pub(crate) fn store_changed() {
    store_channel().send_modify(|g| *g += 1);
}

pub(crate) fn store_subscribe() -> watch::Receiver<u64> {
    store_channel().subscribe()
}

const SETTLE: Duration = Duration::from_millis(80);

/// Ends with `guard` or when a source is gone.
pub(crate) fn watch_sources<T: QObject + 'static>(
    me: QPointer<T>,
    core: &Arc<Core>,
    guard: &Guard,
    on_change: fn(&mut T),
) {
    let notify = queued_callback(move |()| {
        if let Some(model) = me.as_pinned() {
            on_change(&mut model.borrow_mut());
        }
    });
    let core = core.clone();
    let mut stop = guard.subscribe();
    handle().spawn(async move {
        let mut locations = core.locations.subscribe();
        let mut store = store_subscribe();
        let mut status = core.bridge.as_ref().map(|b| b.watch_status());
        let mut nearby = core.bridge.as_ref().map(|b| b.watch_nearby());
        let mut drives = core.bridge.as_ref().map(|b| b.watch_drives());
        loop {
            tokio::select! {
                r = locations.changed() => if r.is_err() { return },
                r = store.changed() => if r.is_err() { return },
                ok = changed(&mut status) => if !ok { return },
                ok = changed(&mut nearby) => if !ok { return },
                ok = changed(&mut drives) => if !ok { return },
                _ = stop.changed() => return,
            }
            tokio::time::sleep(SETTLE).await;
            locations.borrow_and_update();
            store.borrow_and_update();
            notify(());
        }
    });
}

/// False when the bridge is gone. Never completes without a bridge.
async fn changed<T>(rx: &mut Option<watch::Receiver<T>>) -> bool {
    match rx {
        Some(rx) => rx.changed().await.is_ok(),
        None => std::future::pending().await,
    }
}

pub(crate) fn error_parts(e: &lautta_core::Error) -> (QString, QString) {
    (QString::from(e.kind.name()), QString::from(e.message.as_str()))
}

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
    qmetaobject::qml_register_type::<layout::BrowseLayoutModel>(
        &uri,
        1,
        0,
        &crate::cstr("BrowseLayoutModel"),
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
        let rows = vec![vec![Cell::s("x"), Cell::Int(7), Cell::Bool(true)]];
        assert_eq!(cell_data(&rows, 0, FIRST_ROLE).to_qstring().to_string(), "x");
        assert_eq!(cell_data(&rows, 0, FIRST_ROLE + 1).to_int(), 7);
        assert!(cell_data(&rows, 0, FIRST_ROLE + 2).to_bool());
        assert!(!cell_data(&rows, 1, FIRST_ROLE).is_valid());
        assert!(!cell_data(&rows, 0, FIRST_ROLE + 3).is_valid());
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
