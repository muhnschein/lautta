// SPDX-License-Identifier: LGPL-2.1-or-later

use super::link::{rpc, Link};
use super::nearby::Settle;
use super::types::{
    bridge_id, BridgeConfig, BridgeStatus, Consent, Drive, Level, NearbyServer, RemoteLocation,
};
use crate::error::{Error, ErrorKind, Result};
use crate::paths::AppPaths;
use std::ffi::OsStr;
use std::os::unix::ffi::OsStrExt;
use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::{Arc, Mutex, MutexGuard, PoisonError};
use tokio::sync::{watch, Notify};
use tokio::task::JoinHandle;

#[derive(Default)]
pub(super) struct Info {
    pub link: Option<Arc<Link>>,
    pub bridge_version: Option<String>,
    pub features: Vec<String>,
    pub consent: Option<Consent>,
    pub applied_refresh: u64,
    pub applied_drives: u64,
    pub nearby: Settle,
}

pub(super) struct Inner {
    pub config: BridgeConfig,
    pub status: watch::Sender<BridgeStatus>,
    pub locations: watch::Sender<Arc<Vec<RemoteLocation>>>,
    pub nearby: watch::Sender<Arc<Vec<NearbyServer>>>,
    pub drives: watch::Sender<Arc<Vec<Drive>>>,
    pub foreground: watch::Sender<bool>,
    pub wake: Notify,
    pub discover: AtomicBool,
    pub refreshes: AtomicU64,
    pub drive_refreshes: AtomicU64,
    pub attempts: AtomicU64,
    pub info: Mutex<Info>,
    pub task: Mutex<Option<JoinHandle<()>>>,
}

impl Inner {
    pub fn info(&self) -> MutexGuard<'_, Info> {
        self.info.lock().unwrap_or_else(PoisonError::into_inner)
    }

    pub fn set_status(&self, status: BridgeStatus) {
        self.status.send_if_modified(|s| {
            let changed = *s != status;
            *s = status;
            changed
        });
    }
}

#[derive(Clone)]
pub struct BridgeClient {
    pub(super) inner: Arc<Inner>,
}

impl BridgeClient {
    pub fn start(config: BridgeConfig) -> Result<BridgeClient> {
        let runtime = tokio::runtime::Handle::try_current()
            .map_err(|_| Error::new(ErrorKind::Internal, "the bridge client needs a tokio runtime"))?;
        let inner = Arc::new(Inner {
            foreground: watch::channel(config.foreground).0,
            config,
            status: watch::channel(BridgeStatus::Absent).0,
            locations: watch::channel(Arc::new(Vec::new())).0,
            nearby: watch::channel(Arc::new(Vec::new())).0,
            drives: watch::channel(Arc::new(Vec::new())).0,
            wake: Notify::new(),
            discover: AtomicBool::new(false),
            refreshes: AtomicU64::new(0),
            drive_refreshes: AtomicU64::new(0),
            attempts: AtomicU64::new(0),
            info: Mutex::new(Info::default()),
            task: Mutex::new(None),
        });
        let task = runtime.spawn(super::session::supervise(inner.clone()));
        *inner.task.lock().unwrap_or_else(PoisonError::into_inner) = Some(task);
        Ok(BridgeClient { inner })
    }

    pub fn start_for_app(paths: &AppPaths) -> Result<BridgeClient> {
        BridgeClient::start(BridgeConfig::for_app(paths))
    }

    pub fn shutdown(&self) {
        if let Some(task) = self
            .inner
            .task
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .take()
        {
            task.abort();
        }
        if let Some(link) = self.inner.info().link.take() {
            link.close();
        }
        self.inner.set_status(BridgeStatus::Absent);
    }

    pub fn status(&self) -> BridgeStatus {
        *self.inner.status.borrow()
    }

    pub fn access(&self, location: &str) -> Option<Level> {
        let id = bridge_id(location);
        self.inner
            .locations
            .borrow()
            .iter()
            .find(|l| l.bridge_id == id)
            .map(|l| l.access)
    }

    pub fn watch_status(&self) -> watch::Receiver<BridgeStatus> {
        self.inner.status.subscribe()
    }

    pub async fn wait_status(&self, pred: impl Fn(BridgeStatus) -> bool) -> BridgeStatus {
        let mut rx = self.watch_status();
        loop {
            let current = *rx.borrow_and_update();
            if pred(current) {
                return current;
            }
            if rx.changed().await.is_err() {
                return current;
            }
        }
    }

    pub fn locations(&self) -> Arc<Vec<RemoteLocation>> {
        self.inner.locations.borrow().clone()
    }

    pub fn watch_locations(&self) -> watch::Receiver<Arc<Vec<RemoteLocation>>> {
        self.inner.locations.subscribe()
    }

    pub fn location(&self, id: &str) -> Option<RemoteLocation> {
        let bridge = bridge_id(id);
        self.locations().iter().find(|l| l.bridge_id == bridge).cloned()
    }

    pub fn nearby(&self) -> Arc<Vec<NearbyServer>> {
        self.inner.nearby.borrow().clone()
    }

    pub fn watch_nearby(&self) -> watch::Receiver<Arc<Vec<NearbyServer>>> {
        self.inner.nearby.subscribe()
    }

    pub fn consent(&self) -> Option<Consent> {
        self.inner.info().consent
    }

    pub fn connection_attempts(&self) -> u64 {
        self.inner.attempts.load(Ordering::SeqCst)
    }

    pub fn bridge_version(&self) -> Option<String> {
        self.inner.info().bridge_version.clone()
    }

    pub fn drives(&self) -> Arc<Vec<Drive>> {
        self.inner.drives.borrow().clone()
    }

    pub fn watch_drives(&self) -> watch::Receiver<Arc<Vec<Drive>>> {
        self.inner.drives.subscribe()
    }

    pub fn offers(&self, feature: &str) -> bool {
        self.inner.info().features.iter().any(|f| f == feature)
    }

    pub fn set_foreground(&self, foreground: bool) {
        let changed = self.inner.foreground.send_if_modified(|f| {
            let changed = *f != foreground;
            *f = foreground;
            changed
        });
        if changed && foreground {
            self.inner.wake.notify_one();
        }
    }

    pub fn poke(&self) {
        self.inner.wake.notify_one();
    }

    pub(crate) fn link(&self) -> Result<Arc<Link>> {
        match self.inner.info().link.clone() {
            Some(link) => Ok(link),
            None => Err(unavailable(self.status())),
        }
    }

    pub(crate) fn files_link(&self) -> Result<Arc<Link>> {
        let status = self.status();
        if status == BridgeStatus::Ready {
            return self.link();
        }
        Err(unavailable(status))
    }

    pub async fn request_consent(&self) -> Result<()> {
        rpc(self.link()?.proxy().request_consent().await)
    }

    pub async fn discover(&self, on: bool) -> Result<()> {
        self.inner.discover.store(on, Ordering::SeqCst);
        match self.files_link() {
            Ok(link) => {
                if on {
                    super::session::begin_settle(&self.inner);
                }
                rpc(link.proxy().discover(on).await)
            }
            Err(_) => Ok(()),
        }
    }

    pub async fn add_account(&self, provider: &str) -> Result<()> {
        rpc(self.files_link()?.proxy().add_account(provider).await)
    }

    pub async fn open_account_settings(&self, loc: &str) -> Result<()> {
        rpc(self
            .files_link()?
            .proxy()
            .open_account_settings(bridge_id(loc))
            .await)
    }

    pub async fn is_removed(&self, loc: &str) -> bool {
        let Ok(link) = self.files_link() else {
            return false;
        };
        matches!(
            rpc(link.proxy().disconnect(bridge_id(loc)).await),
            Err(e) if e.kind == ErrorKind::NotFound
        )
    }

    pub async fn open_storage_settings(&self) -> Result<()> {
        rpc(self.link()?.proxy().open_storage_settings().await)
    }

    pub async fn mount_drive(&self, drive: &str) -> Result<PathBuf> {
        let point = rpc(self.link()?.proxy().mount(drive).await)?;
        Ok(PathBuf::from(OsStr::from_bytes(&point)))
    }

    pub async fn unmount_drive(&self, drive: &str) -> Result<()> {
        rpc(self.link()?.proxy().unmount(drive).await)
    }
}

pub(super) fn unavailable(status: BridgeStatus) -> Error {
    match status {
        BridgeStatus::Reconnecting => Error::new(ErrorKind::ConnectionLost, "reconnecting to the bridge"),
        BridgeStatus::ConsentUnknown => Error::new(
            ErrorKind::PermissionDenied,
            "the user has not decided yet what this app may do",
        ),
        _ => Error::new(ErrorKind::BridgeUnavailable, "network locations are unavailable"),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn unavailable_maps_status_to_error_kinds() {
        assert_eq!(
            unavailable(BridgeStatus::Reconnecting).kind,
            ErrorKind::ConnectionLost
        );
        assert_eq!(
            unavailable(BridgeStatus::ConsentUnknown).kind,
            ErrorKind::PermissionDenied
        );
        assert_eq!(
            unavailable(BridgeStatus::Absent).kind,
            ErrorKind::BridgeUnavailable
        );
        assert_eq!(
            unavailable(BridgeStatus::Connecting).kind,
            ErrorKind::BridgeUnavailable
        );
    }

    #[test]
    fn starting_without_a_runtime_is_an_error() {
        let paths = AppPaths::new("/nonexistent-home");
        let e = BridgeClient::start_for_app(&paths).err().unwrap();
        assert_eq!(e.kind, ErrorKind::Internal);
    }
}
