// SPDX-License-Identifier: LGPL-2.1-or-later
//! `LocationPrefsModel { locationId }`: the settings of one location
//! (display name, start folder, caches, bulk lanes) and what the page needs
//! to show of the location itself.

use super::{error_parts, store_changed};
use crate::runtime::{blocking_then, core};
use lautta_core::app::Core;
use lautta_core::locations::{Location, LocationKind};
use lautta_core::settings::LocationPrefs;
use lautta_core::Uri;
use qmetaobject::prelude::*;
use qmetaobject::QPointer;

/// Engineering name of the kind of a location, for QML.
fn kind_name(l: &Location) -> &'static str {
    match l.kind {
        LocationKind::UserFolder => "userFolder",
        LocationKind::Android => "android",
        LocationKind::Volume => "volume",
        LocationKind::Server { .. } => "account",
        LocationKind::AdHoc => "adhoc",
        LocationKind::Archive => "archive",
    }
}

/// What a page shows of a location besides its settings.
struct Info {
    name: String,
    kind: &'static str,
    provider: String,
    address: String,
    attention: String,
    prefs: LocationPrefs,
}

fn load(core: &Core, id: &str) -> lautta_core::Result<Info> {
    let prefs = core.location_prefs.get(id)?;
    let location = core.location(id);
    let provider = match location.as_ref().map(|l| &l.kind) {
        Some(LocationKind::Server { provider }) => lautta_core::app_browse::provider_label(provider),
        _ => String::new(),
    };
    Ok(Info {
        name: location.as_ref().map(|l| l.name.clone()).unwrap_or_default(),
        kind: location.as_ref().map_or("", kind_name),
        provider,
        address: core.locations.display_address(&Uri::root(id)),
        attention: location.and_then(|l| l.attention).unwrap_or_default(),
        prefs,
    })
}

/// The stored form of the page's fields: blank means "not set", lane 0
/// means "use the global number".
fn prefs_from(
    id: &str,
    display_name: &str,
    start_folder: &str,
    no_listing_cache: bool,
    no_thumb_cache: bool,
    bulk_lanes: i32,
) -> LocationPrefs {
    LocationPrefs {
        location_id: id.to_owned(),
        display_name: Some(display_name.to_owned()),
        start_folder: Uri::parse(start_folder).ok(),
        no_listing_cache,
        no_thumb_cache,
        bulk_lanes: u32::try_from(bulk_lanes).ok().filter(|n| *n > 0),
    }
}

#[derive(QObject, Default)]
pub struct LocationPrefsModel {
    base: qt_base_class!(trait QObject),
    locationId: qt_property!(QString; NOTIFY idChanged WRITE set_location_id),
    idChanged: qt_signal!(),
    loaded: qt_property!(bool; NOTIFY changed),
    /// The location's own name and kind (`userFolder`, `android`, `volume`,
    /// `account`, `adhoc`, `archive`), protocol label and address.
    locationName: qt_property!(QString; NOTIFY changed),
    kind: qt_property!(QString; NOTIFY changed),
    provider: qt_property!(QString; NOTIFY changed),
    address: qt_property!(QString; NOTIFY changed),
    /// `auth-failed`, `server-identity-changed` or empty.
    attention: qt_property!(QString; NOTIFY changed),
    displayName: qt_property!(QString; NOTIFY changed),
    startFolder: qt_property!(QString; NOTIFY changed),
    noListingCache: qt_property!(bool; NOTIFY changed),
    noThumbCache: qt_property!(bool; NOTIFY changed),
    /// `0` uses the global number of lanes.
    bulkLanes: qt_property!(i32; NOTIFY changed),
    changed: qt_signal!(),

    reload: qt_method!(fn(&mut self)),
    save: qt_method!(fn(&mut self)),
    clearCache: qt_method!(fn(&mut self)),
    saved: qt_signal!(),
    failed: qt_signal!(kind: QString, message: QString),
}

impl LocationPrefsModel {
    fn set_location_id(&mut self, id: QString) {
        if self.locationId != id {
            self.locationId = id;
            self.idChanged();
            self.reload();
        }
    }

    fn reload(&mut self) {
        let Some(core) = core() else { return };
        let id = self.locationId.to_string();
        let me = QPointer::from(&*self);
        blocking_then(
            move || load(&core, &id),
            move |res| {
                let Some(model) = me.as_pinned() else { return };
                let mut model = model.borrow_mut();
                match res {
                    Ok(info) => model.show(info),
                    Err(e) => model.report(&e),
                }
            },
        );
    }

    fn show(&mut self, info: Info) {
        self.locationName = QString::from(info.name.as_str());
        self.kind = QString::from(info.kind);
        self.provider = QString::from(info.provider.as_str());
        self.address = QString::from(info.address.as_str());
        self.attention = QString::from(info.attention.as_str());
        self.displayName = QString::from(info.prefs.display_name.unwrap_or_default().as_str());
        self.startFolder = QString::from(
            info.prefs
                .start_folder
                .map(|u| u.to_string())
                .unwrap_or_default()
                .as_str(),
        );
        self.noListingCache = info.prefs.no_listing_cache;
        self.noThumbCache = info.prefs.no_thumb_cache;
        self.bulkLanes = info.prefs.bulk_lanes.map_or(0, |n| n as i32);
        self.loaded = true;
        self.changed();
    }

    fn report(&self, e: &lautta_core::Error) {
        let (kind, message) = error_parts(e);
        self.failed(kind, message);
    }

    fn save(&mut self) {
        let Some(core) = core() else { return };
        let id = self.locationId.to_string();
        let prefs = prefs_from(
            &id,
            &self.displayName.to_string(),
            &self.startFolder.to_string(),
            self.noListingCache,
            self.noThumbCache,
            self.bulkLanes,
        );
        let me = QPointer::from(&*self);
        blocking_then(
            move || {
                core.location_prefs.set(&prefs)?;
                core.dircache.set_disabled(&id, prefs.no_listing_cache)
            },
            move |res| {
                let Some(model) = me.as_pinned() else { return };
                let model = model.borrow();
                match res {
                    Ok(()) => {
                        store_changed();
                        model.saved();
                    }
                    Err(e) => model.report(&e),
                }
            },
        );
    }

    /// *Clear this location's cache*: its cached listings.
    fn clearCache(&mut self) {
        let Some(core) = core() else { return };
        let id = self.locationId.to_string();
        let me = QPointer::from(&*self);
        blocking_then(
            move || core.dircache.invalidate_location(&id),
            move |res| {
                if let (Err(e), Some(model)) = (res, me.as_pinned()) {
                    model.borrow().report(&e);
                }
            },
        );
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn page_fields_become_stored_prefs() {
        let p = prefs_from(
            "nv-account:1",
            " NAS ",
            "lautta://nv-account:1/srv",
            true,
            false,
            3,
        );
        assert_eq!(p.display_name.as_deref(), Some(" NAS "));
        assert_eq!(p.start_folder.unwrap().to_string(), "lautta://nv-account:1/srv");
        assert!(p.no_listing_cache && !p.no_thumb_cache);
        assert_eq!(p.bulk_lanes, Some(3));
        let none = prefs_from("x", "", "", false, false, 0);
        assert_eq!(none.start_folder, None);
        assert_eq!(none.bulk_lanes, None);
    }

    #[test]
    fn kinds_have_names() {
        let l = Location::remote("nv-adhoc:1", LocationKind::AdHoc, "x", None);
        assert_eq!(kind_name(&l), "adhoc");
        let l = Location::remote(
            "nv-account:1",
            LocationKind::Server {
                provider: "sftp".into(),
            },
            "x",
            None,
        );
        assert_eq!(kind_name(&l), "account");
    }
}
