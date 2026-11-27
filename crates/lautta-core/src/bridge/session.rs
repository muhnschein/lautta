// SPDX-License-Identifier: LGPL-2.1-or-later

use super::client::Inner;
use super::link::{rpc, Link};
use super::types::{BridgeStatus, Consent, Drive, NearbyServer, Network, RemoteLocation};
use futures::{FutureExt, StreamExt};
use lautta_bridge_proto::{feature, BridgeError, Signal, SignalStream};
use std::future::Future;
use std::os::unix::fs::FileTypeExt;
use std::path::Path;
use std::sync::atomic::Ordering;
use std::sync::Arc;
use std::time::Duration;
use tokio::sync::watch;
use tokio::time::{sleep, timeout};

enum Attempt {
    Ended,
    Failed,
}

pub(super) async fn supervise(inner: Arc<Inner>) {
    let mut lost = false;
    let mut attempt = 0usize;
    loop {
        if wait_foreground(&inner).await {
            inner.wake.notified().now_or_never();
        }
        if !lost && !socket_present(&inner.config.socket).await {
            inner.set_status(BridgeStatus::Absent);
            idle(&inner).await;
            continue;
        }
        if !lost {
            inner.set_status(BridgeStatus::Connecting);
        }
        match attempt_session(&inner, lost).await {
            Attempt::Ended => {
                lost = true;
                attempt = 0;
                inner.wake.notified().now_or_never();
                inner.set_status(BridgeStatus::Reconnecting);
                wait_or_wake(&inner, inner.config.backoff_for(0)).await;
            }
            Attempt::Failed if lost => {
                attempt += 1;
                inner.set_status(BridgeStatus::Reconnecting);
                wait_or_wake(&inner, inner.config.backoff_for(attempt)).await;
            }
            Attempt::Failed => {
                inner.set_status(BridgeStatus::Absent);
                idle(&inner).await;
            }
        }
    }
}

async fn wait_foreground(inner: &Inner) -> bool {
    let mut rx = inner.foreground.subscribe();
    let mut waited = false;
    while !*rx.borrow_and_update() {
        waited = true;
        if rx.changed().await.is_err() {
            break;
        }
    }
    waited
}

async fn socket_present(path: &Path) -> bool {
    tokio::fs::symlink_metadata(path)
        .await
        .map(|m| m.file_type().is_socket())
        .unwrap_or(false)
}

async fn idle(inner: &Inner) {
    match inner.config.poll_interval {
        Some(every) => wait_or_wake(inner, every).await,
        None => inner.wake.notified().await,
    }
}

async fn wait_or_wake(inner: &Inner, wait: Duration) {
    tokio::select! {
        () = sleep(wait) => {}
        () = inner.wake.notified() => {}
    }
}

fn clear_lists(inner: &Inner) {
    inner.info().nearby.cancel();
    inner.locations.send_if_modified(|l| {
        let changed = !l.is_empty();
        if changed {
            *l = Arc::new(Vec::new());
        }
        changed
    });
    inner.nearby.send_if_modified(|n| {
        let changed = !n.is_empty();
        if changed {
            *n = Arc::new(Vec::new());
        }
        changed
    });
}

async fn bounded<T>(limit: Option<Duration>, work: impl Future<Output = T>) -> Option<T> {
    match limit {
        Some(limit) => timeout(limit, work).await.ok(),
        None => Some(work.await),
    }
}

async fn attempt_session(inner: &Arc<Inner>, lost: bool) -> Attempt {
    inner.attempts.fetch_add(1, Ordering::SeqCst);
    let limit = inner.config.handshake_timeout;
    let Some(Ok(conn)) = bounded(limit, lautta_bridge_proto::connect(&inner.config.socket)).await else {
        return Attempt::Failed;
    };
    let mut signals = conn.signals();
    let link = Link::new(conn.clone());
    let shaken = bounded(limit, handshake(inner, &link))
        .await
        .unwrap_or(Err(Attempt::Failed));
    if let Err(attempt) = shaken {
        link.close();
        conn.close().await;
        return attempt;
    }
    log::debug!("bridge connected (after a loss: {lost})");
    run_signals(inner, &link, &mut signals).await;
    teardown(inner, &link);
    Attempt::Ended
}

async fn handshake(inner: &Arc<Inner>, link: &Arc<Link>) -> Result<(), Attempt> {
    let proxy = link.proxy();
    let version = rpc(proxy.hello(&inner.config.client_name).await).map_err(|_| Attempt::Failed)?;
    let features = features(proxy.get_features().await.map_err(BridgeError::from))?;
    let consent = rpc(proxy.get_consent().await).map_err(|_| Attempt::Failed)?;
    {
        let mut info = inner.info();
        info.link = Some(link.clone());
        info.bridge_version = Some(version);
        info.features = features;
    }
    apply_consent(inner, link, Consent::from_wire(&consent));
    spawn_drives(inner, link);
    spawn_network(inner, link);
    Ok(())
}

fn features(answer: Result<Vec<String>, BridgeError>) -> Result<Vec<String>, Attempt> {
    match answer {
        Ok(features) => Ok(features),
        Err(BridgeError::Disconnected(_)) => Err(Attempt::Failed),
        Err(e) => {
            if !matches!(e, BridgeError::UnknownMethod(_)) {
                log::warn!("bridge features not known: {e}");
            }
            Ok(Vec::new())
        }
    }
}

async fn run_signals(inner: &Arc<Inner>, link: &Arc<Link>, signals: &mut SignalStream) {
    while let Some(item) = signals.next().await {
        match item {
            Ok(signal) => handle_signal(inner, link, signal),
            Err(BridgeError::Protocol(m)) => log::warn!("unreadable bridge signal: {m}"),
            Err(_) => break,
        }
    }
}

fn teardown(inner: &Arc<Inner>, link: &Arc<Link>) {
    link.close();
    {
        let mut info = inner.info();
        info.link = None;
        info.features.clear();
        info.nearby.cancel();
    }
    inner.nearby.send_if_modified(|n| {
        let changed = !n.is_empty();
        *n = Arc::new(Vec::new());
        changed
    });
    publish(&inner.drives, Vec::new());
    set_network(inner, Network::Unknown);
}

fn handle_signal(inner: &Arc<Inner>, link: &Arc<Link>, signal: Signal) {
    let Some(signal) = link.route(signal) else {
        return;
    };
    match signal {
        Signal::ConsentChanged(consent) => apply_consent(inner, link, Consent::from_wire(&consent)),
        Signal::LocationsChanged if consent_decided(inner) => {
            spawn_refresh(inner, link);
        }
        Signal::NearbyChanged(list) if consent_decided(inner) => {
            let list: Vec<NearbyServer> = list.iter().map(NearbyServer::from_wire).collect();
            let shown = inner.nearby.borrow().clone();
            let show = inner.info().nearby.receive(&shown, list);
            show_nearby(inner, show);
        }
        Signal::DrivesChanged => spawn_drives(inner, link),
        Signal::NetworkChanged(kind) => set_network(inner, Network::from_wire(&kind)),
        _ => {}
    }
}

fn consent_decided(inner: &Inner) -> bool {
    inner.info().consent == Some(Consent::Decided)
}

fn apply_consent(inner: &Arc<Inner>, link: &Arc<Link>, consent: Consent) {
    inner.info().consent = Some(consent);
    match consent {
        Consent::Decided => {
            let (inner2, link2) = (inner.clone(), link.clone());
            tokio::spawn(async move {
                refresh_locations(&inner2, &link2).await;
                let current = inner2
                    .info()
                    .link
                    .as_ref()
                    .is_some_and(|l| Arc::ptr_eq(l, &link2));
                if current && consent_decided(&inner2) {
                    inner2.set_status(BridgeStatus::Ready);
                }
            });
            if inner.discover.load(Ordering::SeqCst) {
                begin_settle(inner);
                let link = link.clone();
                tokio::spawn(async move {
                    let _ = link.proxy().discover(true).await;
                });
            }
        }
        Consent::Unknown => {
            inner.set_status(BridgeStatus::ConsentUnknown);
            clear_lists(inner);
        }
    }
}

fn show_nearby(inner: &Inner, list: Vec<NearbyServer>) {
    inner.nearby.send_if_modified(|n| {
        let changed = **n != list;
        if changed {
            *n = Arc::new(list);
        }
        changed
    });
}

pub(super) fn begin_settle(inner: &Arc<Inner>) {
    let generation = inner.info().nearby.begin();
    let inner = inner.clone();
    tokio::spawn(async move {
        sleep(inner.config.nearby_settle).await;
        let settled = inner.info().nearby.finish(generation);
        if let Some(list) = settled {
            show_nearby(&inner, list);
        }
    });
}

fn spawn_refresh(inner: &Arc<Inner>, link: &Arc<Link>) {
    let inner = inner.clone();
    let link = link.clone();
    tokio::spawn(async move { refresh_locations(&inner, &link).await });
}

async fn refresh_locations(inner: &Arc<Inner>, link: &Arc<Link>) {
    let generation = inner.refreshes.fetch_add(1, Ordering::SeqCst) + 1;
    let wire = match rpc(link.proxy().list_locations().await) {
        Ok(wire) => wire,
        Err(e) => {
            log::warn!("bridge locations not listed: {e}");
            return;
        }
    };
    let list: Vec<RemoteLocation> = wire.iter().map(RemoteLocation::from_wire).collect();
    {
        let mut info = inner.info();
        if generation <= info.applied_refresh || info.consent != Some(Consent::Decided) {
            return;
        }
        info.applied_refresh = generation;
    }
    inner.locations.send_if_modified(|current| {
        let changed = **current != list;
        if changed {
            *current = Arc::new(list);
        }
        changed
    });
}

fn set_network(inner: &Inner, network: Network) {
    inner.network.send_if_modified(|current| {
        let changed = *current != network;
        *current = network;
        changed
    });
}

fn spawn_network(inner: &Arc<Inner>, link: &Arc<Link>) {
    if !inner.info().features.iter().any(|f| f == feature::NETWORK) {
        return;
    }
    let inner = inner.clone();
    let link = link.clone();
    tokio::spawn(async move {
        match rpc(link.proxy().get_network().await) {
            Ok(kind) => {
                let current = inner.info().link.as_ref().is_some_and(|l| Arc::ptr_eq(l, &link));
                if current {
                    set_network(&inner, Network::from_wire(&kind));
                }
            }
            Err(e) => log::warn!("bridge network kind not known: {e}"),
        }
    });
}

fn spawn_drives(inner: &Arc<Inner>, link: &Arc<Link>) {
    if !inner.info().features.iter().any(|f| f == feature::DRIVES) {
        return;
    }
    let inner = inner.clone();
    let link = link.clone();
    tokio::spawn(async move { refresh_drives(&inner, &link).await });
}

async fn refresh_drives(inner: &Arc<Inner>, link: &Arc<Link>) {
    let generation = inner.drive_refreshes.fetch_add(1, Ordering::SeqCst) + 1;
    let wire = match rpc(link.proxy().list_drives().await) {
        Ok(wire) => wire,
        Err(e) => {
            log::warn!("bridge drives not listed: {e}");
            return;
        }
    };
    let list: Vec<Drive> = wire.iter().map(Drive::from_wire).collect();
    {
        let mut info = inner.info();
        let current = info.link.as_ref().is_some_and(|l| Arc::ptr_eq(l, link));
        if generation <= info.applied_drives || !current {
            return;
        }
        info.applied_drives = generation;
    }
    publish(&inner.drives, list);
}

fn publish<T: PartialEq>(tx: &watch::Sender<Arc<T>>, value: T) {
    tx.send_if_modified(|current| {
        let changed = **current != value;
        if changed {
            *current = Arc::new(value);
        }
        changed
    });
}
