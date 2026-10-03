// SPDX-License-Identifier: LGPL-2.1-or-later
//! Settings → Storage and Locations: cache sizes and clearing (SEC-5,
//! DAT-3), the list of locations with preferences, and `LocationPrefsModel`
//! (per-location settings, §18).

use crate::json::to_json;
use crate::runtime::{blocking_then, core};
use lautta_core::locations::LocationKind;
use lautta_core::settings::LocationPrefs;
use lautta_core::Uri;
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

#[derive(QObject, Default)]
pub struct LocationPrefsModel {
    base: qt_base_class!(trait QObject),
    locationId: qt_property!(QString; NOTIFY idChanged WRITE set_location_id),
    idChanged: qt_signal!(),
    /// The name the location has without an override.
    defaultName: qt_property!(QString; NOTIFY loaded),
    /// Display name override; blank means none.
    displayName: qt_property!(QString; NOTIFY loaded),
    /// Folder the location opens in (URI); blank means the root.
    startFolder: qt_property!(QString; NOTIFY loaded),
    noListingCache: qt_property!(bool; NOTIFY loaded),
    noThumbCache: qt_property!(bool; NOTIFY loaded),
    /// Bulk lanes of a remote location (1 to 6); 0 uses the global setting.
    laneSize: qt_property!(i32; NOTIFY loaded),
    ready: qt_property!(bool; NOTIFY loaded),
    loaded: qt_signal!(),

    load: qt_method!(fn(&mut self)),
    save: qt_method!(fn(&mut self)),
    /// Back to the defaults (stores nothing).
    reset: qt_method!(fn(&mut self)),
    saved: qt_signal!(),
    failed: qt_signal!(kind: QString, message: QString),
}

impl LocationPrefsModel {
    fn set_location_id(&mut self, id: QString) {
        self.locationId = id;
        self.idChanged();
        self.load();
    }

    fn load(&mut self) {
        let Some(core) = core() else { return };
        let id = self.locationId.to_string();
        let me = QPointer::from(&*self);
        blocking_then(
            move || {
                let name = core.location(&id).map(|l| l.name).unwrap_or_default();
                (
                    name,
                    core.location_prefs
                        .get(&id)
                        .unwrap_or_else(|_| LocationPrefs::new(&id)),
                )
            },
            move |(name, prefs)| {
                if let Some(model) = me.as_pinned() {
                    model.borrow_mut().show(&name, &prefs);
                }
            },
        );
    }

    fn show(&mut self, default_name: &str, prefs: &LocationPrefs) {
        self.defaultName = QString::from(default_name);
        self.displayName = QString::from(prefs.display_name.as_deref().unwrap_or(""));
        self.startFolder = QString::from(
            prefs
                .start_folder
                .as_ref()
                .map(ToString::to_string)
                .unwrap_or_default()
                .as_str(),
        );
        self.noListingCache = prefs.no_listing_cache;
        self.noThumbCache = prefs.no_thumb_cache;
        self.laneSize = prefs.bulk_lanes.map_or(0, |n| n as i32);
        self.ready = true;
        self.loaded();
    }

    fn prefs(&self) -> LocationPrefs {
        let name = self.displayName.to_string();
        LocationPrefs {
            location_id: self.locationId.to_string(),
            display_name: Some(name).filter(|n| !n.trim().is_empty()),
            start_folder: Uri::parse(&self.startFolder.to_string()).ok(),
            no_listing_cache: self.noListingCache,
            no_thumb_cache: self.noThumbCache,
            bulk_lanes: u32::try_from(self.laneSize).ok().filter(|n| *n > 0),
        }
    }

    fn save(&mut self) {
        self.store(self.prefs());
    }

    fn reset(&mut self) {
        let prefs = LocationPrefs::new(&self.locationId.to_string());
        self.store(prefs);
    }

    fn store(&mut self, prefs: LocationPrefs) {
        let Some(core) = core() else { return };
        let me = QPointer::from(&*self);
        blocking_then(
            move || core.set_location_prefs(&prefs).map(|()| prefs).map_err(err),
            move |res| {
                let Some(model) = me.as_pinned() else { return };
                let mut m = model.borrow_mut();
                match res {
                    Ok(prefs) => {
                        let name = m.defaultName.to_string();
                        m.show(&name, &prefs);
                        m.saved();
                    }
                    Err((k, msg)) => m.failed(QString::from(k.as_str()), QString::from(msg.as_str())),
                }
            },
        );
    }
}
