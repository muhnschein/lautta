// SPDX-License-Identifier: LGPL-2.1-or-later
//! Backoff schedule is checked on a paused clock while no I/O is in flight (idle runtime jumps a
//! paused clock to the next timer, firing timeouts around I/O at once). Live-connection tests run
//! real time at 1/20 of it.

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
    assert_eq!(rig.client.locations().len(), 1);
    let e = provider
        .stat(&VPath::root(), true, Lane::Interactive)
        .await
        .unwrap_err();
    assert_eq!(e.kind, ErrorKind::ConnectionLost);
    assert!(e.kind.is_transient(), "transfers wait and resume");

    rig.fake.set_accepting(true);
    rig.client.wait_status(|s| s == BridgeStatus::Ready).await;
    assert_eq!(rig.fake.calls_of("Hello").len(), 2, "a new session");
    provider
        .stat(&VPath::root(), true, Lane::Interactive)
        .await
        .unwrap();

    rig.fake
        .set_nearby(vec![WireNearby {
            name: "NAS".into(),
            provider: "smb".into(),
            host: "nas".into(),
            port: 445,
        }])
        .await;
    eventually("nearby after the reconnect", || rig.client.nearby().len() == 1).await;
}

/// Real sockets and clock: no attempt comes early. The exact schedule is proven on the paused clock.
#[tokio::test]
async fn attempts_over_real_sockets_never_come_before_their_backoff() {
    let rig = Rig::ready_with(quick_backoff).await;
    rig.fake.set_accepting(false);
    rig.fake.disconnect_clients().await;
    eventually("six attempts", || rig.fake.accept_times().len() >= 7).await;
    let g = gaps(&rig);
    for (got, want) in g
        .iter()
        .zip([ms(50), ms(100), ms(200), ms(400), ms(750), ms(750)])
    {
        assert!(
            *got + ms(5) >= want,
            "attempt after {got:?}, wanted at least {want:?} ({g:?})"
        );
    }
    assert_eq!(rig.client.status(), BridgeStatus::Reconnecting);
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
    rig.client.wait_status(|s| s == BridgeStatus::Ready).await;
}

#[tokio::test]
async fn a_socket_without_a_bridge_behind_it_is_standalone_not_an_error() {
    let rig = Rig::ready_with(quick_backoff).await;
    rig.fake.set_accepting(false);
    rig.fake.disconnect_clients().await;
    rig.client.wait_status(|s| s == BridgeStatus::Reconnecting).await;
    assert!(rig.paths.bridge_socket().exists());

    let fresh = BridgeClient::start(bridge_support::config(&rig.paths), Questions::new()).unwrap();
    // Absent is also the status before the first look at the socket.
    eventually("an attempt", || fresh.connection_attempts() >= 1).await;
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

#[tokio::test(start_paused = true)]
async fn the_default_schedule_is_1_2_4_8_then_every_15_seconds() {
    let mut rig = Rig::ready().await;
    drop(rig.listener.take());
    rig.fake.disconnect_clients().await;
    rig.client.wait_status(|s| s == BridgeStatus::Reconnecting).await;
    let t0 = Instant::now();
    let before = rig.client.connection_attempts();

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

#[tokio::test(start_paused = true)]
async fn after_a_reconnect_the_backoff_starts_over() {
    let mut rig = Rig::ready().await;
    drop(rig.listener.take());
    rig.fake.disconnect_clients().await;
    rig.client.wait_status(|s| s == BridgeStatus::Reconnecting).await;
    let lost = rig.client.connection_attempts();
    sleep(secs(16)).await;
    assert_eq!(
        rig.client.connection_attempts() - lost,
        4,
        "backoff grown to 15 s"
    );
    rig.listen();
    rig.client.wait_status(|s| s == BridgeStatus::Ready).await;

    drop(rig.listener.take());
    rig.fake.disconnect_clients().await;
    rig.client.wait_status(|s| s == BridgeStatus::Reconnecting).await;
    let t0 = Instant::now();
    let before = rig.client.connection_attempts();
    tokio::time::sleep_until(t0 + secs(1) - ms(100)).await;
    assert_eq!(rig.client.connection_attempts(), before);
    tokio::time::sleep_until(t0 + secs(1) + ms(100)).await;
    assert_eq!(rig.client.connection_attempts(), before + 1, "first after 1 s");
    tokio::time::sleep_until(t0 + secs(3) - ms(100)).await;
    assert_eq!(rig.client.connection_attempts(), before + 1);
    tokio::time::sleep_until(t0 + secs(3) + ms(100)).await;
    assert_eq!(rig.client.connection_attempts(), before + 2, "then 2 s, not 15 s");
}

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
