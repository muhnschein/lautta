// SPDX-License-Identifier: LGPL-2.1-or-later
//! Settings → Storage and Locations: cache sizes and clearing (SEC-5,
//! DAT-3) and the list of locations with preferences. Per-location settings
//! (§18) are the browse area's `LocationPrefsModel`.

use crate::json::to_json;
use crate::runtime::{blocking_then, core};
use lautta_core::locations::LocationKind;
use qmetaobject::prelude::*;
use qmetaobject::QPointer;

#[derive(QObject, Default)]
pub struct CacheInfo {
    base: qt_base_class!(trait QObject),
    /// All caches together, in bytes; 0 until the first `refresh()` answers.
    cacheBytes: qt_property!(i64; NOTIFY changed),
    thumbnailBytes: qt_property!(i64; NOTIFY changed),
    listingBytes: qt_property!(i64; NOTIFY changed),
    archiveBytes: qt_property!(i64; NOTIFY changed),
    busy: qt_property!(bool; NOTIFY changed),
    changed: qt_signal!(),

    refresh: qt_method!(fn(&mut self)),
    clearCache: qt_method!(fn(&mut self)),
    clearAppData: qt_method!(fn(&mut self)),
    /// Settings → Recents → Clear recents (ORG-2).
    clearRecents: qt_method!(fn(&mut self)),
    /// Servers, ad-hoc servers and removable volumes that can have their own
    /// preferences: `[{ id, name, kind }]`.
    locationsJson: qt_method!(fn(&self) -> QString),

    cacheCleared: qt_signal!(freedBytes: i64),
    appDataCleared: qt_signal!(),
    recentsCleared: qt_signal!(),
    /// `kind` is the `ErrorKind` name (`Locked` while transfers are unfinished).
    failed: qt_signal!(kind: QString, message: QString),
}

fn err(e: lautta_core::Error) -> (String, String) {
    (e.kind.name().to_owned(), e.message)
}

impl CacheInfo {
    fn refresh(&mut self) {
        let Some(core) = core() else { return };
        let me = QPointer::from(&*self);
        blocking_then(
            move || core.cache_sizes(),
            move |sizes| {
                if let Some(model) = me.as_pinned() {
                    let mut m = model.borrow_mut();
                    m.thumbnailBytes = sizes.thumbnails as i64;
                    m.listingBytes = sizes.listings as i64;
                    m.archiveBytes = sizes.archives as i64;
                    m.cacheBytes = sizes.total() as i64;
                    m.changed();
                }
            },
        );
    }

    fn clearCache(&mut self) {
        let Some(core) = core() else { return };
        self.set_busy(true);
        let me = QPointer::from(&*self);
        blocking_then(
            move || core.clear_cache().map_err(err),
            move |res| {
                let Some(model) = me.as_pinned() else { return };
                let mut m = model.borrow_mut();
                m.set_busy(false);
                match res {
                    Ok(freed) => {
                        m.cacheCleared(freed as i64);
                        m.refresh();
                    }
                    Err((k, msg)) => m.failed(QString::from(k.as_str()), QString::from(msg.as_str())),
                }
            },
        );
    }

    fn clearAppData(&mut self) {
        let Some(core) = core() else { return };
        self.set_busy(true);
        let me = QPointer::from(&*self);
        blocking_then(
            move || core.clear_app_data().map_err(err),
            move |res| {
                let Some(model) = me.as_pinned() else { return };
                let mut m = model.borrow_mut();
                m.set_busy(false);
                match res {
                    Ok(()) => {
                        m.appDataCleared();
                        m.refresh();
                    }
                    Err((k, msg)) => m.failed(QString::from(k.as_str()), QString::from(msg.as_str())),
                }
            },
        );
    }

    fn clearRecents(&mut self) {
        let Some(core) = core() else { return };
        let me = QPointer::from(&*self);
        blocking_then(
            move || core.recents.clear().map_err(err),
            move |res| {
                let Some(model) = me.as_pinned() else { return };
                let m = model.borrow();
                match res {
                    Ok(_) => m.recentsCleared(),
                    Err((k, msg)) => m.failed(QString::from(k.as_str()), QString::from(msg.as_str())),
                }
            },
        );
    }

    fn set_busy(&mut self, busy: bool) {
        self.busy = busy;
        self.changed();
    }

    fn locationsJson(&self) -> QString {
        let list: Vec<serde_json::Value> = core()
            .map(|c| c.locations.locations())
            .unwrap_or_default()
            .into_iter()
            .filter_map(|l| {
                let kind = match l.kind {
                    LocationKind::Server { .. } => "server",
                    LocationKind::AdHoc => "adhoc",
                    LocationKind::Volume => "volume",
                    _ => return None,
                };
                Some(serde_json::json!({ "id": l.id, "name": l.name, "kind": kind }))
            })
            .collect();
        QString::from(to_json(&list).as_str())
    }
}
