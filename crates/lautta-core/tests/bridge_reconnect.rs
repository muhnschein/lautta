// SPDX-License-Identifier: LGPL-2.1-or-later
//! Connection loss and reconnecting with backoff (SPEC NVB-12).
//!
//! The exact 1, 2, 4, 8, 15 s schedule is checked on a paused clock while no
//! I/O is in flight (an idle runtime jumps a paused clock to the next timer,
//! which would fire timeouts around I/O at once). Everything that needs a live
//! connection runs in real time on the same schedule at a twentieth of it.

mod bridge_support;

use bridge_support::{eventually, quick_backoff, Rig, ACCOUNT};
use lautta_bridge_proto::fake::FakeBridge;
use lautta_bridge_proto::WireNearby;
use lautta_core::bridge::{BridgeClient, BridgeStatus};
use lautta_core::error::ErrorKind;
use lautta_core::provider::{Lane, Provider};
use lautta_core::questions::Questions;
use lautta_core::vpath::VPath;
use std::time::Duration;
use tokio::time::{sleep, Instant};

fn ms(n: u64) -> Duration {
    Duration::from_millis(n)
}

fn secs(n: u64) -> Duration {
    Duration::from_secs(n)
}

/// Gaps between the connections the bridge saw, from the second on.
fn gaps(rig: &Rig) -> Vec<Duration> {
    let times = rig.fake.accept_times();
    times.windows(2).map(|w| w[1].duration_since(w[0])).collect()
}

#[tokio::test]
async fn a_lost_connection_is_re_established() {
    let rig = Rig::ready_with(quick_backoff).await;
    rig.fake.put_file("account:1", b"f", b"x");
    let provider = rig.provider();
    rig.client.discover(true).await.unwrap();

    rig.fake.set_accepting(false);
    rig.fake.disconnect_clients().await;
    rig.client.wait_status(|s| s == BridgeStatus::Reconnecting).await;
    // The locations stay on screen as "Reconnecting…" (NVB-12).
    assert_eq!(rig.client.locations().len(), 1);
    let e = provider
        .stat(&VPath::root(), true, Lane::Interactive)
        .await
        .unwrap_err();
    assert_eq!(e.kind, ErrorKind::ConnectionLost);
    assert!(e.kind.is_transient(), "transfers wait and resume (XFR-12)");

    rig.fake.set_accepting(true);
    rig.client.wait_status(|s| s == BridgeStatus::Ready).await;
    assert_eq!(rig.fake.calls_of("Hello").len(), 2, "a new session");
    provider
        .stat(&VPath::root(), true, Lane::Interactive)
        .await
        .unwrap();

    // A discovery the user asked for is asked for again.
    rig.fake
        .set_nearby(vec![WireNearby {
            name: "NAS".into(),
            provider: "smb".into(),
            host: "nas".into(),
            port: 445,
            path: vec![],
        }])
        .await;
    eventually("nearby after the reconnect", || rig.client.nearby().len() == 1).await;
}

#[tokio::test]
async fn attempts_back_off_in_doubling_steps_and_start_over() {
    let rig = Rig::ready_with(quick_backoff).await;
    rig.fake.set_accepting(false);
    rig.fake.disconnect_clients().await;
    // The initial connection, then six attempts that the bridge turns away.
    eventually("six attempts", || rig.fake.accept_times().len() >= 7).await;
    let g = gaps(&rig);
    // Lower bounds are exact (a timer never fires early); upper ones leave room for a loaded machine.
    for (got, want) in g
        .iter()
        .zip([ms(50), ms(100), ms(200), ms(400), ms(750), ms(750)])
    {
        assert!(
            *got + ms(5) >= want,
            "attempt after {got:?}, wanted at least {want:?} ({g:?})"
        );
        assert!(
            *got < want + ms(900),
            "attempt after {got:?}, wanted about {want:?} ({g:?})"
        );
    }
    assert_eq!(rig.client.status(), BridgeStatus::Reconnecting);

    // The bridge comes back, and the next loss starts at the first step again.
    rig.fake.set_accepting(true);
    rig.client.wait_status(|s| s == BridgeStatus::Ready).await;
    rig.fake.set_accepting(false);
    rig.fake.disconnect_clients().await;
    let seen = rig.fake.accept_times().len();
    eventually("the first attempt", || rig.fake.accept_times().len() > seen).await;
    let g = gaps(&rig);
    let first = *g.last().unwrap();
    assert!(first + ms(5) >= ms(50) && first < ms(900), "{first:?}");
}

#[tokio::test]
async fn the_background_does_not_reconnect_and_coming_back_does_at_once() {
    let rig = Rig::ready_with(quick_backoff).await;
    rig.client.set_foreground(false);
    rig.fake.disconnect_clients().await;
    rig.client.wait_status(|s| s == BridgeStatus::Reconnecting).await;
    let accepted = rig.fake.accept_times().len();
    sleep(ms(600)).await; // twelve first steps
    assert_eq!(
        rig.fake.accept_times().len(),
        accepted,
        "no attempts in the background"
    );
    assert_eq!(rig.client.status(), BridgeStatus::Reconnecting);

    rig.client.set_foreground(true);
    rig.client.wait_status(|s| s == BridgeStatus::Ready).await;
}

#[tokio::test]
async fn going_to_the_background_during_the_backoff_holds_the_next_attempt() {
    let rig = Rig::ready_with(quick_backoff).await;
    rig.fake.set_accepting(false);
    rig.fake.disconnect_clients().await;
    eventually("two attempts", || rig.fake.accept_times().len() >= 3).await;
    rig.client.set_foreground(false);
    sleep(ms(200)).await;
    let settled = rig.fake.accept_times().len();
    sleep(ms(1000)).await;
    assert_eq!(
        rig.fake.accept_times().len(),
        settled,
        "nothing while in the background"
    );
}

#[tokio::test]
async fn a_poke_skips_the_remaining_wait() {
    let rig = Rig::ready_with(|mut c| {
        c.backoff = vec![secs(30)];
        c
    })
    .await;
    rig.fake.set_accepting(false);
    rig.fake.disconnect_clients().await;
    rig.client.wait_status(|s| s == BridgeStatus::Reconnecting).await;
    sleep(ms(100)).await;
    rig.fake.set_accepting(true);
    rig.client.poke();
    let t0 = Instant::now();
    rig.client.wait_status(|s| s == BridgeStatus::Ready).await;
    assert!(t0.elapsed() < secs(5), "{:?}", t0.elapsed());
}

#[tokio::test]
async fn a_missing_socket_is_polled_while_in_the_foreground() {
    let fake = FakeBridge::new();
    fake.add_account(1, "fake", "Fake 1", "fake.example").await;
    let mut rig = Rig::with_fake(fake, false, |mut c| {
        c.poll_interval = Some(ms(100));
        c
    })
    .await;
    rig.client.wait_status(|s| s == BridgeStatus::Absent).await;
    rig.listen();
    // No poke: the poll finds it.
    rig.client.wait_status(|s| s == BridgeStatus::Ready).await;
}

#[tokio::test]
async fn a_socket_without_a_bridge_behind_it_is_standalone_not_an_error() {
    let rig = Rig::ready_with(quick_backoff).await;
    // The socket file remains but nothing answers (stale after a crash).
    rig.fake.set_accepting(false);
    rig.fake.disconnect_clients().await;
    rig.client.wait_status(|s| s == BridgeStatus::Reconnecting).await;
    assert!(rig.paths.bridge_socket().exists());

    // A fresh start against such a socket shows no network locations and no message.
    let fresh = BridgeClient::start(bridge_support::config(&rig.paths), Questions::new()).unwrap();
    fresh.wait_status(|s| s == BridgeStatus::Absent).await;
    assert!(fresh.locations().is_empty());
    let e = fresh
        .provider(ACCOUNT)
        .stat(&VPath::root(), true, Lane::Interactive)
        .await
        .unwrap_err();
    assert_eq!(e.kind, ErrorKind::BridgeUnavailable);
    fresh.shutdown();
}

#[tokio::test]
async fn a_silent_bridge_is_given_up_on_after_the_handshake_timeout() {
    // A server that accepts the socket but never speaks the protocol.
    let home = tempfile::tempdir().unwrap();
    let paths = lautta_core::paths::AppPaths::new(home.path());
    std::fs::create_dir_all(paths.bridge_socket().parent().unwrap()).unwrap();
    let listener = tokio::net::UnixListener::bind(paths.bridge_socket()).unwrap();
    let hold = tokio::spawn(async move {
        let mut held = Vec::new();
        while let Ok((stream, _)) = listener.accept().await {
            held.push(stream);
        }
    });
    let mut config = bridge_support::config(&paths);
    config.handshake_timeout = Some(ms(300));
    let t0 = Instant::now();
    let client = BridgeClient::start(config, Questions::new()).unwrap();
    client.wait_status(|s| s == BridgeStatus::Connecting).await;
    client.wait_status(|s| s == BridgeStatus::Absent).await;
    assert!(t0.elapsed() >= ms(295), "gave up after {:?}", t0.elapsed());
    assert_eq!(client.connection_attempts(), 1);
    client.shutdown();
    hold.abort();
}

/// The default schedule, exactly: 1, 2, 4, 8 s, then every 15 s (NVB-12). No
/// socket is left, so an attempt fails at once and only timers are in play.
#[tokio::test(start_paused = true)]
async fn the_default_schedule_is_1_2_4_8_then_every_15_seconds() {
    let mut rig = Rig::ready().await;
    drop(rig.listener.take());
    rig.fake.disconnect_clients().await;
    rig.client.wait_status(|s| s == BridgeStatus::Reconnecting).await;
    let t0 = Instant::now();
    let before = rig.client.connection_attempts();

    // Attempts are due 1, 3, 7, 15, 30, 45 and 60 s after the loss.
    for (k, due) in [1u64, 3, 7, 15, 30, 45, 60].into_iter().enumerate() {
        tokio::time::sleep_until(t0 + secs(due) - ms(100)).await;
        assert_eq!(
            rig.client.connection_attempts() - before,
            k as u64,
            "before {due} s"
        );
        tokio::time::sleep_until(t0 + secs(due) + ms(100)).await;
        assert_eq!(
            rig.client.connection_attempts() - before,
            k as u64 + 1,
            "after {due} s"
        );
    }
    assert_eq!(rig.client.status(), BridgeStatus::Reconnecting);
}

/// While in the background nothing is attempted, however long it takes.
#[tokio::test(start_paused = true)]
async fn the_background_holds_every_attempt_on_a_paused_clock() {
    let mut rig = Rig::ready().await;
    drop(rig.listener.take());
    rig.fake.disconnect_clients().await;
    rig.client.wait_status(|s| s == BridgeStatus::Reconnecting).await;
    rig.client.set_foreground(false);
    sleep(secs(1)).await;
    let held = rig.client.connection_attempts();
    sleep(secs(3600)).await;
    assert_eq!(rig.client.connection_attempts(), held);
    rig.client.set_foreground(true);
    sleep(ms(1)).await;
    assert_eq!(
        rig.client.connection_attempts(),
        held + 1,
        "an attempt at once, without the backoff"
    );
}
