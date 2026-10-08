// SPDX-License-Identifier: LGPL-2.1-or-later
//! The supervisor: finds the socket, connects, says `Hello`, follows consent,
//! locations and questions, and reconnects with backoff while the app is in the
//! foreground (SPEC NVB-1..4, NVB-12).

use super::client::Inner;
use super::link::{rpc, Link};
use super::types::{BridgeStatus, Consent, NearbyServer, RemoteLocation};
use crate::questions::{Answer, QuestionKind};
use futures::{FutureExt, StreamExt};
use lautta_bridge_proto::zvariant::{OwnedValue, Value};
use lautta_bridge_proto::{value_i64, BridgeError, Connection, Signal, SignalStream, Zeroizing};
use std::collections::{BTreeMap, HashMap};
use std::future::Future;
use std::os::unix::fs::FileTypeExt;
use std::path::Path;
use std::sync::atomic::Ordering;
use std::sync::Arc;
use std::time::Duration;
use tokio::time::{sleep, timeout};

enum Attempt {
    /// Connected and served until the connection ended.
    Ended,
    TooOld,
    Failed,
}

pub(super) async fn supervise(inner: Arc<Inner>) {
    let mut lost = false;
    let mut attempt = 0usize;
    loop {
        if wait_foreground(&inner).await {
            // Coming back is itself the poke; a second one would skip the next backoff.
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
                // A poke from before the loss must not skip the first backoff.
                inner.wake.notified().now_or_never();
                inner.set_status(BridgeStatus::Reconnecting);
                wait_or_wake(&inner, inner.config.backoff_for(0)).await;
            }
            Attempt::TooOld => {
                lost = false;
                inner.set_status(BridgeStatus::TooOld);
                clear_lists(&inner);
                idle(&inner).await;
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

/// Waits until the app is in the foreground; true if it had to wait.
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

/// Waits for a poke, or for the next poll of a missing bridge (NVB-1).
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

/// Runs `work` within `limit`, if there is one; `None` when it ran out.
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
    // Subscribed before the first call so that no signal is missed.
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

/// `Hello` (NVB-2), then the consent (NVB-3). Failure is an `Attempt`.
async fn handshake(inner: &Arc<Inner>, link: &Arc<Link>) -> Result<(), Attempt> {
    let proxy = link.proxy();
    let (protocol, version, features) = rpc(proxy
        .hello(inner.config.required_protocol, &inner.config.client_name)
        .await)
    .map_err(|_| Attempt::Failed)?;
    if protocol < inner.config.required_protocol {
        return Err(Attempt::TooOld);
    }
    let consent = rpc(proxy.get_consent().await).map_err(|_| Attempt::Failed)?;
    {
        let mut info = inner.info();
        info.link = Some(link.clone());
        info.bridge_version = Some(version);
        info.features = features;
    }
    apply_consent(inner, link, Consent::from_wire(&consent));
    Ok(())
}

async fn run_signals(inner: &Arc<Inner>, link: &Arc<Link>, signals: &mut SignalStream) {
    while let Some(item) = signals.next().await {
        match item {
            Ok(signal) => handle_signal(inner, link, signal),
            // A signal this client cannot read is skipped, not fatal.
            Err(BridgeError::Protocol(m)) => log::warn!("unreadable bridge signal: {m}"),
            Err(_) => break,
        }
    }
}

/// The connection is gone: wait for nothing, ask nothing (NVB-12).
fn teardown(inner: &Arc<Inner>, link: &Arc<Link>) {
    link.close();
    let questions = {
        let mut info = inner.info();
        info.link = None;
        info.nearby.cancel();
        std::mem::take(&mut info.questions)
    };
    for id in questions {
        inner.questions.cancel(&id);
    }
    inner.nearby.send_if_modified(|n| {
        let changed = !n.is_empty();
        *n = Arc::new(Vec::new());
        changed
    });
}

fn handle_signal(inner: &Arc<Inner>, link: &Arc<Link>, signal: Signal) {
    let Some(signal) = link.route(signal) else {
        return;
    };
    match signal {
        Signal::ConsentChanged(consent) => apply_consent(inner, link, Consent::from_wire(&consent)),
        Signal::LocationsChanged => {
            if consent_granted(inner) {
                spawn_refresh(inner, link);
            }
        }
        Signal::NearbyChanged(list) => {
            if consent_granted(inner) {
                let list: Vec<NearbyServer> = list.iter().map(NearbyServer::from_wire).collect();
                let shown = inner.nearby.borrow().clone();
                let show = inner.info().nearby.receive(&shown, list);
                show_nearby(inner, show);
            }
        }
        Signal::Question { id, kind, details } => spawn_question(inner, link, id, &kind, &details),
        _ => {}
    }
}

fn consent_granted(inner: &Inner) -> bool {
    inner.info().consent == Some(Consent::Granted)
}

/// The consent decides what the app shows (NVB-3): nothing but the status
/// until it is `granted`. `Ready` follows the first list of locations, so that
/// the Servers section never flashes empty.
fn apply_consent(inner: &Arc<Inner>, link: &Arc<Link>, consent: Consent) {
    inner.info().consent = Some(consent);
    match consent {
        Consent::Granted => {
            let (inner2, link2) = (inner.clone(), link.clone());
            tokio::spawn(async move {
                refresh_locations(&inner2, &link2).await;
                let current = inner2
                    .info()
                    .link
                    .as_ref()
                    .is_some_and(|l| Arc::ptr_eq(l, &link2));
                // The consent or the connection may have changed meanwhile.
                if current && consent_granted(&inner2) {
                    inner2.set_status(BridgeStatus::Ready);
                }
            });
            if inner.discover.load(Ordering::SeqCst) {
                begin_settle(inner);
                let link = link.clone();
                tokio::spawn(async move {
                    // Discovery is a convenience; a failure only leaves Nearby empty.
                    let _ = link.proxy().discover(true).await;
                });
            }
        }
        Consent::Unknown => {
            inner.set_status(BridgeStatus::ConsentUnknown);
            clear_lists(inner);
        }
        Consent::Denied => {
            inner.set_status(BridgeStatus::ConsentDenied);
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

/// The discovery is about to (re)start: the servers shown stay until it has
/// settled (see [`super::nearby`]).
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

/// `ListLocations`; of overlapping refreshes the newest request wins.
async fn refresh_locations(inner: &Arc<Inner>, link: &Arc<Link>) {
    let generation = inner.refreshes.fetch_add(1, Ordering::SeqCst) + 1;
    let wire = match rpc(link.proxy().list_locations().await) {
        Ok(wire) => wire,
        Err(e) => {
            // The Servers section stays as it is; say why, for the journal.
            log::warn!("bridge locations not listed: {e}");
            return;
        }
    };
    let list: Vec<RemoteLocation> = wire.iter().map(RemoteLocation::from_wire).collect();
    {
        let mut info = inner.info();
        if generation <= info.applied_refresh || info.consent != Some(Consent::Granted) {
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

/// A display string for a question detail.
fn show(v: &Value<'_>) -> String {
    match v {
        Value::Str(s) => s.as_str().to_owned(),
        Value::Bool(b) => b.to_string(),
        Value::Value(inner) => show(inner),
        Value::Array(a) if a.element_signature().as_str() == "y" => {
            hex::encode(a.iter().filter_map(|b| u8::try_from(b).ok()).collect::<Vec<u8>>())
        }
        Value::Array(a) => a.iter().map(show).collect::<Vec<_>>().join("\n"),
        other => value_i64(other).map(|n| n.to_string()).unwrap_or_default(),
    }
}

/// A question of the bridge goes to the hub; the answer goes back (NVB-6).
fn spawn_question(
    inner: &Arc<Inner>,
    link: &Arc<Link>,
    id: String,
    kind: &str,
    details: &HashMap<String, OwnedValue>,
) {
    let details: BTreeMap<String, String> = details.iter().map(|(k, v)| (k.clone(), show(v))).collect();
    let pending = inner.questions.ask(QuestionKind::from_wire(kind), details);
    let hub_id = pending.id().to_owned();
    inner.info().questions.push(hub_id.clone());
    let inner = inner.clone();
    let link = link.clone();
    tokio::spawn(async move {
        let answer = pending.wait().await;
        inner.info().questions.retain(|q| *q != hub_id);
        send_answer(link.connection(), &id, answer).await;
    });
}

async fn send_answer(conn: &Connection, id: &str, answer: Option<Answer>) {
    let (accept, mut secrets) = match answer {
        Some(Answer::Responses(r)) => (true, r.into_inner()),
        Some(Answer::Accept | Answer::Choice(_)) => (true, Zeroizing::new(Vec::new())),
        Some(Answer::Decline) | None => (false, Zeroizing::new(Vec::new())),
    };
    if let Err(e) = conn.answer_wiping(id, accept, &mut secrets).await {
        // The question is gone with its connection; nothing is left to answer.
        log::debug!("answer to {id} not delivered: {e}");
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use lautta_bridge_proto::OptsBuilder;

    #[test]
    fn details_become_display_strings() {
        let o = OptsBuilder::new()
            .str("host", "nas")
            .int("port", 22)
            .bool("systemTrusted", false)
            .bytes("publicKey", &[0xde, 0xad])
            .build();
        assert_eq!(show(&o["host"]), "nas");
        assert_eq!(show(&o["port"]), "22");
        assert_eq!(show(&o["systemTrusted"]), "false");
        assert_eq!(show(&o["publicKey"]), "dead");
        assert_eq!(show(&Value::from(vec!["a".to_owned(), "b".to_owned()])), "a\nb");
        assert_eq!(show(&Value::from(1.5f64)), "");
        assert_eq!(show(&Value::Value(Box::new(Value::from("x")))), "x");
    }
}
