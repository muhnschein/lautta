// SPDX-License-Identifier: LGPL-2.1-or-later
//! `status`: `absent`, `connecting`, `consentUnknown`, `ready`, `reconnecting`.

use super::{error_parts, Guard};
use crate::runtime::{core, handle, spawn_then};
use lautta_core::app::Core;
use lautta_core::bridge::{feature, BridgeStatus};
use lautta_core::Uri;
use qmetaobject::prelude::*;
use qmetaobject::{queued_callback, QPointer, QSingletonInit, QVariant, QVariantList};
use std::sync::{Arc, Once};

static PURGE: Once = Once::new();

pub fn status_name(s: BridgeStatus) -> &'static str {
    match s {
        BridgeStatus::Absent => "absent",
        BridgeStatus::Connecting => "connecting",
        BridgeStatus::ConsentUnknown => "consentUnknown",
        BridgeStatus::Ready => "ready",
        BridgeStatus::Reconnecting => "reconnecting",
    }
}

/// Provider preselected by *Add server*.
const DEFAULT_PROVIDER: &str = "sftp";

/// One entry per call: two calls on one drive keep it busy until both ended.
#[derive(Default)]
struct Busy(Vec<String>);

impl Busy {
    fn begin(&mut self, drive: &str) {
        self.0.push(drive.to_owned());
    }

    fn end(&mut self, drive: &str) {
        if let Some(i) = self.0.iter().position(|d| d == drive) {
            self.0.remove(i);
        }
    }

    fn list(&self) -> QVariantList {
        self.0
            .iter()
            .map(|d| QVariant::from(QString::from(d.as_str())))
            .collect()
    }
}

#[derive(QObject, Default)]
pub struct Bridge {
    base: qt_base_class!(trait QObject),
    status: qt_property!(QString; NOTIFY statusChanged),
    statusChanged: qt_signal!(),
    version: qt_property!(QString; NOTIFY statusChanged),
    ready: qt_property!(bool; NOTIFY statusChanged),
    storageSettings: qt_property!(bool; NOTIFY statusChanged),
    /// One entry per call. Browse rows pulse.
    busyDrives: qt_property!(QVariantList; NOTIFY statusChanged),

    requestConsent: qt_method!(fn(&mut self)),
    addServer: qt_method!(fn(&mut self, provider: QString)),
    openStorageSettings: qt_method!(fn(&mut self)),
    mountDrive: qt_method!(fn(&mut self, drive: QString, name: QString)),
    unmountDrive: qt_method!(fn(&mut self, drive: QString, name: QString)),
    editAccount: qt_method!(fn(&mut self, location_uri: QString)),
    setDiscover: qt_method!(fn(&mut self, on: bool)),
    setForeground: qt_method!(fn(&mut self, foreground: bool)),
    poke: qt_method!(fn(&mut self)),

    /// A drive mounted: its folder to open.
    mounted: qt_signal!(locationUri: QString),
    /// `subject` names what it was about (server, address).
    failed: qt_signal!(kind: QString, message: QString, subject: QString),

    guard: Guard,
    busy: Busy,
}

impl QSingletonInit for Bridge {
    fn init(&mut self) {
        let Some(core) = core() else { return };
        self.publish_status(&core);
        PURGE.call_once(|| {
            let core = core.clone();
            handle().spawn(async move { core.spawn_account_purge(super::store_changed) });
        });
        self.follow_status(core);
    }
}

impl Bridge {
    fn publish_status(&mut self, core: &Core) {
        let (status, version, storage) = match &core.bridge {
            Some(b) => (
                b.status(),
                b.bridge_version().unwrap_or_default(),
                b.offers(feature::STORAGE_SETTINGS),
            ),
            None => (BridgeStatus::Absent, String::new(), false),
        };
        self.status = QString::from(status_name(status));
        self.ready = status == BridgeStatus::Ready;
        self.version = QString::from(version.as_str());
        self.storageSettings = storage;
        self.statusChanged();
    }

    fn follow_status(&mut self, core: Arc<Core>) {
        let Some(client) = core.bridge.clone() else {
            return;
        };
        let me = QPointer::from(&*self);
        let notify = queued_callback(move |core: Arc<Core>| {
            if let Some(b) = me.as_pinned() {
                b.borrow_mut().publish_status(&core);
            }
        });
        let mut stop = self.guard.subscribe();
        handle().spawn(async move {
            let mut status = client.watch_status();
            loop {
                tokio::select! {
                    r = status.changed() => if r.is_err() { return },
                    _ = stop.changed() => return,
                }
                notify(core.clone());
            }
        });
    }

    fn report(&self, e: &lautta_core::Error, subject: &str) {
        let (kind, message) = error_parts(e);
        self.failed(kind, message, QString::from(subject));
    }

    fn call<F>(&self, subject: String, f: impl FnOnce(Arc<Core>) -> F)
    where
        F: std::future::Future<Output = lautta_core::Result<()>> + Send + 'static,
    {
        let Some(core) = core() else { return };
        let me = QPointer::from(self);
        spawn_then(f(core), move |res| {
            if let (Err(e), Some(b)) = (res, me.as_pinned()) {
                b.borrow().report(&e, &subject);
            }
        });
    }

    /// `drive` is in `busyDrives` until the call ends.
    fn drive_call<F>(&mut self, drive: String, subject: String, f: impl FnOnce(Arc<Core>) -> F)
    where
        F: std::future::Future<Output = lautta_core::Result<Option<Uri>>> + Send + 'static,
    {
        let Some(core) = core() else { return };
        self.busy.begin(&drive);
        self.publish_busy();
        let me = QPointer::from(&*self);
        spawn_then(f(core), move |res| {
            let Some(b) = me.as_pinned() else { return };
            b.borrow_mut().drive_done(&drive);
            let b = b.borrow();
            match res {
                Ok(Some(uri)) => b.mounted(QString::from(uri.to_string().as_str())),
                Ok(None) => {}
                Err(e) => b.report(&e, &subject),
            }
        });
    }

    fn drive_done(&mut self, drive: &str) {
        self.busy.end(drive);
        self.publish_busy();
    }

    fn publish_busy(&mut self) {
        self.busyDrives = self.busy.list();
        self.statusChanged();
    }

    fn requestConsent(&mut self) {
        self.call(String::new(), |core| async move {
            match &core.bridge {
                Some(b) => b.request_consent().await,
                None => Ok(()),
            }
        });
    }

    fn addServer(&mut self, provider: QString) {
        let provider = match provider.to_string() {
            p if p.is_empty() => DEFAULT_PROVIDER.to_owned(),
            p => p,
        };
        self.call(String::new(), |core| async move {
            match &core.bridge {
                Some(b) => b.add_account(&provider).await,
                None => Ok(()),
            }
        });
    }

    fn openStorageSettings(&mut self) {
        self.call(String::new(), |core| async move {
            match &core.bridge {
                Some(b) => b.open_storage_settings().await,
                None => Ok(()),
            }
        });
    }

    fn mountDrive(&mut self, drive: QString, name: QString) {
        let drive = drive.to_string();
        self.drive_call(drive.clone(), name.to_string(), |core| async move {
            core.mount_drive(&drive).await.map(Some)
        });
    }

    fn unmountDrive(&mut self, drive: QString, name: QString) {
        let drive = drive.to_string();
        self.drive_call(drive.clone(), name.to_string(), |core| async move {
            core.unmount_drive(&drive).await.map(|()| None)
        });
    }

    fn editAccount(&mut self, location_uri: QString) {
        let id = location_id(&location_uri.to_string());
        self.call(name_of(&id), |core| async move {
            match &core.bridge {
                Some(b) => b.open_account_settings(&id).await,
                None => Ok(()),
            }
        });
    }

    fn setDiscover(&mut self, on: bool) {
        self.call(String::new(), |core| async move {
            match &core.bridge {
                Some(b) => b.discover(on).await,
                None => Ok(()),
            }
        });
    }

    fn setForeground(&mut self, foreground: bool) {
        if let Some(b) = core().and_then(|c| c.bridge.clone()) {
            b.set_foreground(foreground);
        }
    }

    fn poke(&mut self) {
        if let Some(b) = core().and_then(|c| c.bridge.clone()) {
            b.poke();
        }
    }
}

fn name_of(id: &str) -> String {
    core()
        .and_then(|c| c.location(id))
        .map(|l| l.name)
        .unwrap_or_default()
}

fn location_id(uri_or_id: &str) -> String {
    Uri::parse(uri_or_id)
        .map(|u| u.location)
        .unwrap_or_else(|_| uri_or_id.to_owned())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn status_names_are_the_documented_ones() {
        assert_eq!(status_name(BridgeStatus::Absent), "absent");
        assert_eq!(status_name(BridgeStatus::ConsentUnknown), "consentUnknown");
        assert_eq!(status_name(BridgeStatus::Ready), "ready");
        assert_eq!(status_name(BridgeStatus::Reconnecting), "reconnecting");
        assert_eq!(status_name(BridgeStatus::Connecting), "connecting");
    }

    #[test]
    fn a_drive_is_busy_until_every_call_on_it_has_ended() {
        let names = |b: &Busy| b.0.clone();
        let mut busy = Busy::default();
        busy.begin("sda1");
        busy.begin("mmcblk1p1");
        busy.begin("sda1");
        assert_eq!(names(&busy), ["sda1", "mmcblk1p1", "sda1"]);
        assert_eq!(busy.list().len(), 3);
        busy.end("sda1");
        assert_eq!(names(&busy), ["mmcblk1p1", "sda1"], "one call ended, one is left");
        busy.end("sdb1");
        assert_eq!(
            names(&busy),
            ["mmcblk1p1", "sda1"],
            "an unknown drive changes nothing"
        );
        busy.end("sda1");
        busy.end("mmcblk1p1");
        assert!(names(&busy).is_empty());
        assert!(busy.list().is_empty());
    }

    #[test]
    fn locations_come_from_uris_or_ids() {
        assert_eq!(location_id("lautta://nv-account:1/photos"), "nv-account:1");
        assert_eq!(location_id("nv-account:3"), "nv-account:3");
    }
}
