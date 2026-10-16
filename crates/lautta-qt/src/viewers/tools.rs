// SPDX-License-Identifier: LGPL-2.1-or-later
//! `ViewerTools`: small helpers the viewer pages need (Recents notes, ORG-2)
//! and one for the host QML tests.

use super::parse_uri;
use crate::runtime::core;
use cpp::cpp;
use lautta_core::org::recents::RecentKind;
use qmetaobject::prelude::*;
use qmetaobject::qml_register_type;
use std::path::PathBuf;

cpp! {{
    #include <QtCore/QEventLoop>
    #include <QtCore/QTimer>
}}

pub fn register() {
    qml_register_type::<ViewerTools>(&crate::qml_uri(), 1, 0, &crate::cstr("ViewerTools"));
}

#[derive(QObject, Default)]
pub struct ViewerTools {
    base: qt_base_class!(trait QObject),
    /// Notes a file the user looked at in Recents (previewed).
    noteViewed: qt_method!(fn(&self, uri: QString)),
    /// Notes a file handed to another app in Recents (opened).
    noteOpened: qt_method!(fn(&self, uri: QString)),
    /// Test support: runs the event loop for `ms` milliseconds so async
    /// results of the viewer types arrive while a test is running.
    pump: qt_method!(fn(&self, ms: i32)),
    /// Test support: copies the local file `sourceUrl` (a `file://` URL) to
    /// the local location path of `destUri`; true when it worked.
    installFixture: qt_method!(fn(&self, source_url: QString, dest_uri: QString) -> bool),
}

impl ViewerTools {
    fn note(&self, uri: QString, kind: RecentKind) {
        if let (Some(core), Some(u)) = (core(), parse_uri(&uri)) {
            core.note_viewed(&u, kind);
        }
    }

    fn noteViewed(&self, uri: QString) {
        self.note(uri, RecentKind::Previewed);
    }

    fn noteOpened(&self, uri: QString) {
        self.note(uri, RecentKind::Opened);
    }

    fn pump(&self, ms: i32) {
        cpp!(unsafe [ms as "int"] {
            QEventLoop loop;
            QTimer::singleShot(ms, &loop, &QEventLoop::quit);
            loop.exec();
        });
    }

    fn installFixture(&self, source_url: QString, dest_uri: QString) -> bool {
        let (Some(core), Some(dest)) = (core(), parse_uri(&dest_uri)) else {
            return false;
        };
        let source = source_url.to_string();
        let Some(source) = source.strip_prefix("file://").map(PathBuf::from) else {
            return false;
        };
        match core.locations.to_local_path(&dest) {
            Some(target) => std::fs::copy(source, target).is_ok(),
            None => false,
        }
    }
}
