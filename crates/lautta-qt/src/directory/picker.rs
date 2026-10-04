// SPDX-License-Identifier: LGPL-2.1-or-later
//! `PickerRootsModel`: the locations the folder picker starts from (OPS-10).
//! A plain list of `core.locations.locations()`; Browse has its own richer
//! model.

use crate::runtime::core;
use cpp::cpp;
use lautta_core::locations::{Location, LocationKind, LocationStatus};
use lautta_core::Uri;
use qmetaobject::prelude::*;
use qmetaobject::QByteArray;
use std::cell::RefCell;
use std::collections::HashMap;

cpp! {{
    #include <QtCore/QCoreApplication>
    #include <QtCore/QElapsedTimer>
    #include <QtCore/QEventLoop>
    #include <QtCore/QThread>
}}

const ROLE_URI: i32 = 0x0100;
const ROLE_NAME: i32 = 0x0101;
const ROLE_KIND: i32 = 0x0102;
const ROLE_STATUS: i32 = 0x0103;
const ROLE_ATTENTION: i32 = 0x0104;
const ROLE_SELECTABLE: i32 = 0x0105;

#[derive(Clone)]
struct Root {
    uri: String,
    name: String,
    kind: &'static str,
    status: &'static str,
    attention: String,
}

#[derive(QObject, Default)]
pub struct PickerRootsModel {
    base: qt_base_class!(trait QAbstractListModel),
    refresh: qt_method!(fn(&mut self)),
    /// True when a server location exists (Upload/Download actions, UI-8).
    hasRemote: qt_method!(fn(&self) -> bool),
    /// Test aid: runs the Qt event loop for `ms` milliseconds so queued
    /// results of async actions arrive inside a synchronous QML test.
    pump: qt_method!(fn(&self, ms: i32)),
    rows: RefCell<Option<Vec<Root>>>,
}

fn kind_name(kind: &LocationKind) -> &'static str {
    match kind {
        LocationKind::UserFolder => "folder",
        LocationKind::Android => "android",
        LocationKind::Volume => "volume",
        LocationKind::Server { .. } | LocationKind::AdHoc => "server",
        LocationKind::Archive => "archive",
    }
}

fn status_name(s: LocationStatus) -> &'static str {
    match s {
        LocationStatus::Ready => "ready",
        LocationStatus::Connecting => "connecting",
        LocationStatus::Offline => "offline",
        LocationStatus::NeedsAttention => "attention",
    }
}

fn to_root(l: Location) -> Root {
    Root {
        uri: Uri::root(l.id.clone()).to_string(),
        kind: kind_name(&l.kind),
        status: status_name(l.status),
        attention: l.attention.unwrap_or_default(),
        name: l.name,
    }
}

impl PickerRootsModel {
    fn load() -> Vec<Root> {
        core()
            .map(|c| {
                c.picker_locations()
                    .into_iter()
                    .filter(|l| l.kind != LocationKind::Archive)
                    .map(to_root)
                    .collect()
            })
            .unwrap_or_default()
    }

    fn with_rows<T>(&self, f: impl FnOnce(&[Root]) -> T) -> T {
        let mut rows = self.rows.borrow_mut();
        f(rows.get_or_insert_with(Self::load))
    }

    fn refresh(&mut self) {
        self.begin_reset_model();
        *self.rows.borrow_mut() = None;
        self.end_reset_model();
    }

    fn pump(&self, ms: i32) {
        // SAFETY: plain Qt calls on the GUI thread without Rust pointers.
        unsafe {
            cpp!([ms as "int"] {
                QElapsedTimer timer;
                timer.start();
                while (timer.elapsed() < ms) {
                    QCoreApplication::processEvents(QEventLoop::AllEvents, 10);
                    QThread::msleep(2);
                }
            })
        }
    }

    fn hasRemote(&self) -> bool {
        self.with_rows(|r| r.iter().any(|x| x.kind == "server"))
    }
}

impl QAbstractListModel for PickerRootsModel {
    fn row_count(&self) -> i32 {
        self.with_rows(|r| r.len() as i32)
    }

    fn data(&self, index: QModelIndex, role: i32) -> QVariant {
        let row = usize::try_from(index.row()).ok();
        let Some(root) = row.and_then(|i| self.with_rows(|r| r.get(i).cloned())) else {
            return QVariant::default();
        };
        match role {
            ROLE_URI => QString::from(root.uri.as_str()).into(),
            ROLE_NAME => QString::from(root.name.as_str()).into(),
            ROLE_KIND => QString::from(root.kind).into(),
            ROLE_STATUS => QString::from(root.status).into(),
            ROLE_ATTENTION => QString::from(root.attention.as_str()).into(),
            ROLE_SELECTABLE => (root.status == "ready").into(),
            _ => QVariant::default(),
        }
    }

    fn role_names(&self) -> HashMap<i32, QByteArray> {
        HashMap::from([
            (ROLE_URI, "uri".into()),
            (ROLE_NAME, "name".into()),
            (ROLE_KIND, "kind".into()),
            (ROLE_STATUS, "status".into()),
            (ROLE_ATTENTION, "attention".into()),
            (ROLE_SELECTABLE, "selectable".into()),
        ])
    }
}
