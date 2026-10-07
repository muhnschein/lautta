// SPDX-License-Identifier: LGPL-2.1-or-later
//! The bridge client the rest of the app talks to: status, consent, locations,
//! nearby servers, handoff and ad-hoc servers (SPEC NVB-1..6, NVB-12). The
//! connection itself is kept by the supervisor in [`super::session`].

use super::convert::map_error;
use super::link::{rpc, Link};
use super::nearby::Settle;
use super::types::{
    bridge_id, location_id, AdHocOptions, BridgeConfig, BridgeStatus, Consent, NearbyServer, RemoteLocation,
};
use crate::error::{Error, ErrorKind, Result};
use crate::paths::AppPaths;
use crate::questions::Questions;
use lautta_bridge_proto::{OptsBuilder, Zeroizing};
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
    /// Hub ids of questions the bridge asked on the current connection.
    pub questions: Vec<String>,
    /// Newest `ListLocations` request that was applied.
    pub applied_refresh: u64,
    /// Nearby servers while a restarted discovery settles.
    pub nearby: Settle,
}

pub(super) struct Inner {
    pub config: BridgeConfig,
    pub questions: Arc<Questions>,
    pub status: watch::Sender<BridgeStatus>,
    pub locations: watch::Sender<Arc<Vec<RemoteLocation>>>,
    pub nearby: watch::Sender<Arc<Vec<NearbyServer>>>,
    pub foreground: watch::Sender<bool>,
    pub wake: Notify,
    pub discover: AtomicBool,
    pub refreshes: AtomicU64,
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

/// Handle to the bridge connection; cheap to clone.
#[derive(Clone)]
pub struct BridgeClient {
    pub(super) inner: Arc<Inner>,
}

impl BridgeClient {
    /// Starts discovery and the connection in the background (NVB-1, NVB-2).
    /// Needs a running tokio runtime.
    pub fn start(config: BridgeConfig, questions: Arc<Questions>) -> Result<BridgeClient> {
        let runtime = tokio::runtime::Handle::try_current()
            .map_err(|_| Error::new(ErrorKind::Internal, "the bridge client needs a tokio runtime"))?;
        let inner = Arc::new(Inner {
            foreground: watch::channel(config.foreground).0,
            config,
            questions,
            status: watch::channel(BridgeStatus::Absent).0,
            locations: watch::channel(Arc::new(Vec::new())).0,
            nearby: watch::channel(Arc::new(Vec::new())).0,
            wake: Notify::new(),
            discover: AtomicBool::new(false),
            refreshes: AtomicU64::new(0),
            attempts: AtomicU64::new(0),
            info: Mutex::new(Info::default()),
            task: Mutex::new(None),
        });
        let task = runtime.spawn(super::session::supervise(inner.clone()));
        *inner.task.lock().unwrap_or_else(PoisonError::into_inner) = Some(task);
        Ok(BridgeClient { inner })
    }

    /// [`BridgeClient::start`] with the app's defaults for `paths`.
    pub fn start_for_app(paths: &AppPaths, questions: Arc<Questions>) -> Result<BridgeClient> {
        BridgeClient::start(BridgeConfig::for_app(paths), questions)
    }

    /// Stops the connection; the client is unusable afterwards.
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

    // ---- state ---------------------------------------------------------

    pub fn status(&self) -> BridgeStatus {
        *self.inner.status.borrow()
    }

    pub fn watch_status(&self) -> watch::Receiver<BridgeStatus> {
        self.inner.status.subscribe()
    }

    /// Resolves once the status satisfies `pred`.
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

    /// The bridge locations (remote locations, LOC-7); empty until consent.
    pub fn locations(&self) -> Arc<Vec<RemoteLocation>> {
        self.inner.locations.borrow().clone()
    }

    pub fn watch_locations(&self) -> watch::Receiver<Arc<Vec<RemoteLocation>>> {
        self.inner.locations.subscribe()
    }

    /// A location by internal id (`nv-account:1`).
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

    /// The consent the bridge reported on this connection (NVB-3).
    pub fn consent(&self) -> Option<Consent> {
        self.inner.info().consent
    }

    /// How many times the client has tried to connect, for diagnostics.
    pub fn connection_attempts(&self) -> u64 {
        self.inner.attempts.load(Ordering::SeqCst)
    }

    /// The bridge's own version, for *About*.
    pub fn bridge_version(&self) -> Option<String> {
        self.inner.info().bridge_version.clone()
    }

    /// Feature strings of the bridge (`error-details`, …).
    pub fn features(&self) -> Vec<String> {
        self.inner.info().features.clone()
    }

    // ---- lifecycle -------------------------------------------------------

    /// Tells the client whether the app is in the foreground. Coming back
    /// checks for the socket at once; while in the background it does not
    /// reconnect (NVB-1, NVB-12).
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

    /// Looks for the bridge now (the socket appeared, a *Retry* was tapped).
    pub fn poke(&self) {
        self.inner.wake.notify_one();
    }

    // ---- calls -----------------------------------------------------------

    /// The live connection, whatever the consent.
    pub(crate) fn link(&self) -> Result<Arc<Link>> {
        match self.inner.info().link.clone() {
            Some(link) => Ok(link),
            None => Err(unavailable(self.status())),
        }
    }

    /// The live connection if locations may be used (consent granted).
    pub(crate) fn files_link(&self) -> Result<Arc<Link>> {
        let status = self.status();
        if status == BridgeStatus::Ready {
            return self.link();
        }
        Err(unavailable(status))
    }

    /// Asks again for the permission (NVB-3 *Ask again*).
    pub async fn request_consent(&self) -> Result<()> {
        rpc(self.link()?.proxy().request_consent().await)
    }

    /// Starts or stops looking for servers nearby (LOC-6). The wish is kept and
    /// applied again after a reconnect.
    pub async fn discover(&self, on: bool) -> Result<()> {
        self.inner.discover.store(on, Ordering::SeqCst);
        match self.files_link() {
            Ok(link) => {
                if on {
                    super::session::begin_settle(&self.inner);
                }
                rpc(link.proxy().discover(on).await)
            }
            // Applied when the bridge becomes ready.
            Err(_) => Ok(()),
        }
    }

    /// *Add server*: opens the netvfs account creation (NVB-5).
    pub async fn add_account(&self, provider: &str) -> Result<()> {
        rpc(self.files_link()?.proxy().add_account(provider).await)
    }

    /// *Edit*, *Update sign-in*, *Review server identity*: opens Settings (NVB-5).
    pub async fn open_account_settings(&self, loc: &str) -> Result<()> {
        rpc(self
            .files_link()?
            .proxy()
            .open_account_settings(bridge_id(loc))
            .await)
    }

    /// *Connect to server* (NVB-6). The secret is wiped before this returns,
    /// whatever the outcome; identity prompts arrive as questions.
    pub async fn connect_adhoc(
        &self,
        url: &str,
        secret: Zeroizing<Vec<u8>>,
        opts: AdHocOptions,
    ) -> Result<String> {
        let mut secret = secret;
        self.connect_adhoc_wiping(url, &mut secret, &opts).await
    }

    /// [`BridgeClient::connect_adhoc`] on a buffer the caller keeps: it is empty
    /// afterwards, also when the call fails or is dropped.
    pub async fn connect_adhoc_wiping(
        &self,
        url: &str,
        secret: &mut Zeroizing<Vec<u8>>,
        opts: &AdHocOptions,
    ) -> Result<String> {
        let link = match self.files_link() {
            Ok(link) => link,
            Err(e) => {
                zeroize::Zeroize::zeroize(&mut **secret);
                return Err(e);
            }
        };
        let mut wire = OptsBuilder::new();
        if let Some(user) = &opts.user {
            wire = wire.str("user", user);
        }
        if let Some(profile) = &opts.security_profile {
            wire = wire.str("security_profile", profile);
        }
        let loc = link
            .connection()
            .connect_ad_hoc_wiping(url, secret, &wire.build())
            .await
            .map_err(map_error)?;
        Ok(location_id(&loc))
    }

    /// Forgets an ad-hoc server and the identity pinned for it (XB-14).
    pub async fn forget_adhoc(&self, loc: &str) -> Result<()> {
        rpc(self.files_link()?.proxy().forget_ad_hoc(bridge_id(loc)).await)
    }

    /// Closes the connections of a location in the bridge.
    pub async fn disconnect(&self, loc: &str) -> Result<()> {
        rpc(self.files_link()?.proxy().disconnect(bridge_id(loc)).await)
    }
}

/// Why a call cannot be made in `status` (SPEC §20 "bridge gone").
pub(super) fn unavailable(status: BridgeStatus) -> Error {
    match status {
        BridgeStatus::Reconnecting => Error::new(ErrorKind::ConnectionLost, "reconnecting to the bridge"),
        BridgeStatus::ConsentUnknown | BridgeStatus::ConsentDenied => Error::new(
            ErrorKind::PermissionDenied,
            "the user has not allowed this app to use network locations",
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
            unavailable(BridgeStatus::ConsentDenied).kind,
            ErrorKind::PermissionDenied
        );
        assert_eq!(
            unavailable(BridgeStatus::Absent).kind,
            ErrorKind::BridgeUnavailable
        );
        assert_eq!(
            unavailable(BridgeStatus::TooOld).kind,
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
        let e = BridgeClient::start_for_app(&paths, Questions::new())
            .err()
            .unwrap();
        assert_eq!(e.kind, ErrorKind::Internal);
    }
}
