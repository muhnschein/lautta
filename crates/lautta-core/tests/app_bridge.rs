// SPDX-License-Identifier: LGPL-2.1-or-later
//! The app context with a (fake) netvfs bridge: servers become locations
//! that list and copy like local ones (SPEC §3.4, NVB-4), and disappear
//! entirely without the bridge (UI-8).

mod bridge_support;

use bridge_support::{config, eventually};
use lautta_bridge_proto::fake::{Consent, FakeBridge};
use lautta_core::app::{Core, Started};
use lautta_core::bridge::BridgeStatus;
use lautta_core::error::ErrorKind;
use lautta_core::locations::{ConnectionState, LocationRegistry, LocationStatus};
use lautta_core::ops::OperationKind;
use lautta_core::paths::AppPaths;
use lautta_core::transfer::TransferState;
use lautta_core::Uri;
use std::sync::Arc;

const SERVER: &str = "nv-account:1";

async fn core_with(
    fake: &FakeBridge,
    listen: bool,
) -> (
    tempfile::TempDir,
    Arc<Core>,
    Option<lautta_bridge_proto::fake::Listener>,
) {
    let home = tempfile::tempdir().unwrap();
    std::fs::create_dir_all(home.path().join("Documents")).unwrap();
    let paths = AppPaths::new(home.path());
    let listener = listen.then(|| fake.listen(&paths.bridge_socket()).unwrap());
    let registry = LocationRegistry::with_media_root(paths.clone(), home.path().join("media"));
    let core = Core::open_full(paths.clone(), registry, Some(config(&paths)))
        .await
        .unwrap();
    (home, core, listener)
}

async fn fake_with_files() -> FakeBridge {
    let fake = FakeBridge::new();
    fake.add_account(1, "sftp", "NAS", "nas.home").await;
    fake.put_file("account:1", b"photos/a.jpg", b"jpeg bytes");
    fake
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn transfers_wait_for_the_bridge_only_while_consent_is_denied() {
    let fake = fake_with_files().await;
    fake.set_consent(Consent::Denied).await;
    let (_home, core, _l) = core_with(&fake, true).await;
    let bridge = core.bridge.clone().unwrap();
    bridge.wait_status(|s| s == BridgeStatus::ConsentDenied).await;
    assert!(!core.bridge_reachable());
    fake.set_consent(Consent::Granted).await;
    bridge.wait_status(|s| s == BridgeStatus::Ready).await;
    assert!(core.bridge_reachable());
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn servers_are_locations() {
    let fake = fake_with_files().await;
    let (_home, core, _l) = core_with(&fake, true).await;
    eventually("server location", || core.location(SERVER).is_some()).await;
    let server = core.location(SERVER).unwrap();
    assert_eq!(server.name, "NAS");
    assert_eq!(server.status, LocationStatus::Ready);
    let listing = core
        .list(&Uri::parse("lautta://nv-account:1/photos").unwrap(), |_| {})
        .await
        .unwrap();
    assert_eq!(listing.len(), 1);
    assert_eq!(listing[0].name, b"a.jpg");
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn download_from_a_server() {
    let fake = fake_with_files().await;
    let (home, core, _l) = core_with(&fake, true).await;
    eventually("server location", || core.location(SERVER).is_some()).await;
    let started = core
        .copy_or_move(
            OperationKind::Copy,
            vec![Uri::parse("lautta://nv-account:1/photos/a.jpg").unwrap()],
            Uri::parse("lautta://user-documents/").unwrap(),
        )
        .await
        .unwrap();
    let Started::Transfer(id) = started else {
        panic!("small copy starts at once")
    };
    let done = core.engine.wait_for(id, |s| s.state.is_finished()).await.unwrap();
    assert_eq!(done.state, TransferState::Completed);
    assert_eq!(
        std::fs::read(home.path().join("Documents/a.jpg")).unwrap(),
        b"jpeg bytes"
    );
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn uploads_follow_the_server_name_rules() {
    let fake = fake_with_files().await;
    // A share with Windows' name rules (SMB): a name it refuses gets a safe one.
    fake.set_capabilities("account:1", &["WindowsNames"], &[], 255);
    let (home, core, _l) = core_with(&fake, true).await;
    eventually("server location", || core.location(SERVER).is_some()).await;
    std::fs::write(home.path().join("Documents/a:b.txt"), b"text").unwrap();
    let started = core
        .copy_or_move(
            OperationKind::Copy,
            vec![Uri::parse("lautta://user-documents/a:b.txt").unwrap()],
            Uri::parse("lautta://nv-account:1/photos").unwrap(),
        )
        .await
        .unwrap();
    let Started::Transfer(id) = started else {
        panic!("small copy starts at once")
    };
    let done = core.engine.wait_for(id, |s| s.state.is_finished()).await.unwrap();
    assert_eq!(done.state, TransferState::Completed);
    assert_eq!(
        fake.file("account:1", b"photos/a_b.txt").as_deref(),
        Some(&b"text"[..])
    );
    assert!(!fake.exists("account:1", b"photos/a:b.txt"));
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn copying_from_a_server_that_went_away_says_so() {
    let fake = fake_with_files().await;
    let (_home, core, _l) = core_with(&fake, true).await;
    eventually("server location", || core.location(SERVER).is_some()).await;
    // Copied while the server was there; it goes before the paste.
    fake.remove_location("account:1").await;
    eventually("server gone", || core.location(SERVER).is_none()).await;
    let photo = Uri::parse("lautta://nv-account:1/photos/a.jpg").unwrap();
    let e = core
        .copy_or_move(
            OperationKind::Copy,
            vec![photo.clone()],
            Uri::parse("lautta://user-documents/").unwrap(),
        )
        .await
        .err()
        .unwrap();
    assert_eq!(e.kind, ErrorKind::LocationGone, "{e}");
    assert_eq!(e.subject, Some(photo));
    assert_eq!(core.locations.name(SERVER).as_deref(), Some("NAS"));
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn no_bridge_means_no_servers() {
    let fake = fake_with_files().await;
    let (_home, core, _l) = core_with(&fake, false).await;
    tokio::time::sleep(std::time::Duration::from_millis(200)).await;
    assert!(core.location(SERVER).is_none());
    assert!(core
        .locations
        .locations()
        .iter()
        .all(|l| !l.id.starts_with("nv-")));
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn attention_reaches_the_location() {
    let fake = fake_with_files().await;
    let (_home, core, _l) = core_with(&fake, true).await;
    eventually("server location", || core.location(SERVER).is_some()).await;
    fake.set_attention("account:1", Some("auth-failed")).await;
    eventually("attention", || {
        core.location(SERVER)
            .is_some_and(|l| l.status == LocationStatus::NeedsAttention)
    })
    .await;
    assert_eq!(
        core.location(SERVER).unwrap().attention.as_deref(),
        Some("auth-failed")
    );
}

/// The location carries the bridge's connection state (NVB-4, netvfs
/// XB-12) and follows each `LocationsChanged`; a server that is not
/// connected stays ready to browse.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn the_connection_state_reaches_the_location() {
    let fake = fake_with_files().await;
    let (_home, core, _l) = core_with(&fake, true).await;
    eventually("server location", || core.location(SERVER).is_some()).await;
    let connection = || core.location(SERVER).map(|l| l.connection);
    assert_eq!(connection(), Some(ConnectionState::Disconnected));
    assert_eq!(core.location(SERVER).unwrap().status, LocationStatus::Ready);
    // Listing needs a connection: the bridge connects and tells.
    core.list(&Uri::parse("lautta://nv-account:1/photos").unwrap(), |_| {})
        .await
        .unwrap();
    eventually("connected", || connection() == Some(ConnectionState::Connected)).await;
    fake.set_state("account:1", "failed", Some("Timeout")).await;
    eventually("failed", || {
        connection() == Some(ConnectionState::Failed("Timeout".into()))
    })
    .await;
    assert_eq!(core.location(SERVER).unwrap().status, LocationStatus::Ready);
    fake.set_state("account:1", "connecting", None).await;
    eventually("connecting", || connection() == Some(ConnectionState::Connecting)).await;
    // Closed when idle: still a location to browse.
    fake.set_state("account:1", "disconnected", None).await;
    eventually("disconnected", || {
        connection() == Some(ConnectionState::Disconnected)
    })
    .await;
    assert_eq!(core.location(SERVER).unwrap().status, LocationStatus::Ready);
}
