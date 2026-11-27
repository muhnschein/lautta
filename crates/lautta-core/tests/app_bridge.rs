// SPDX-License-Identifier: LGPL-2.1-or-later

mod bridge_support;

use bridge_support::{config, eventually};
use lautta_bridge_proto::fake::FakeBridge;
use lautta_core::app::{Core, Started};
use lautta_core::bridge::Level;
use lautta_core::error::ErrorKind;
use lautta_core::locations::{ConnectionState, LocationRegistry, LocationStatus};
use lautta_core::ops::OperationKind;
use lautta_core::paths::AppPaths;
use lautta_core::transfer::{TransferState, WaitReason};
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

async fn core_at(
    fake: &FakeBridge,
    level: Level,
) -> (
    tempfile::TempDir,
    Arc<Core>,
    Option<lautta_bridge_proto::fake::Listener>,
) {
    fake.set_access("account:1", Some(level)).await;
    let (home, core, l) = core_with(fake, true).await;
    let client = core.bridge.clone().unwrap();
    eventually("the level", || client.access(SERVER) == Some(level)).await;
    eventually("server location", || core.location(SERVER).is_some()).await;
    std::fs::write(home.path().join("Documents/a.jpg"), b"new").unwrap();
    (home, core, l)
}

fn upload_a() -> (OperationKind, Vec<Uri>, Uri) {
    (
        OperationKind::Copy,
        vec![Uri::parse("lautta://user-documents/a.jpg").unwrap()],
        Uri::parse("lautta://nv-account:1/photos").unwrap(),
    )
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_file_the_level_hides_is_kept_beside_the_copy() {
    let fake = fake_with_files().await;
    let (_home, core, _l) = core_at(&fake, Level::Add).await;
    let (kind, items, to) = upload_a();
    let started = core.copy_or_move(kind, items, to).await;
    let Started::Transfer(id) = started.map_err(|e| format!("{e:?}")).unwrap() else {
        panic!("no conflict the user could see")
    };
    let done = core.engine.wait_for(id, |s| s.state.is_finished()).await.unwrap();
    assert_eq!(done.state, TransferState::Completed);
    assert_eq!(
        fake.file("account:1", b"photos/a.jpg").as_deref(),
        Some(&b"jpeg bytes"[..])
    );
    assert_eq!(
        fake.file("account:1", b"photos/a 2.jpg").as_deref(),
        Some(&b"new"[..])
    );
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_conflict_where_items_stay_offers_no_replace() {
    let fake = fake_with_files().await;
    let (_home, core, _l) = core_at(&fake, Level::ReadAdd).await;
    let (kind, items, to) = upload_a();
    let Started::Transfer(id) = core.copy_or_move(kind, items, to).await.unwrap() else {
        panic!("small copy starts at once")
    };
    let asking = TransferState::Waiting(WaitReason::Question);
    core.engine.wait_for(id, |s| s.state == asking).await.unwrap();
    let questions = core.question_list(id).unwrap();
    assert_eq!(questions[0]["add_only"], true);
    assert_eq!(questions[0]["choices"], serde_json::json!(["KeepBoth", "Skip"]));
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn copying_from_a_server_that_went_away_says_so() {
    let fake = fake_with_files().await;
    let (_home, core, _l) = core_with(&fake, true).await;
    eventually("server location", || core.location(SERVER).is_some()).await;
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

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn the_connection_state_reaches_the_location() {
    let fake = fake_with_files().await;
    let (_home, core, _l) = core_with(&fake, true).await;
    eventually("server location", || core.location(SERVER).is_some()).await;
    let connection = || core.location(SERVER).map(|l| l.connection);
    assert_eq!(connection(), Some(ConnectionState::Disconnected));
    assert_eq!(core.location(SERVER).unwrap().status, LocationStatus::Ready);
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
    fake.set_state("account:1", "disconnected", None).await;
    eventually("disconnected", || {
        connection() == Some(ConnectionState::Disconnected)
    })
    .await;
    assert_eq!(core.location(SERVER).unwrap().status, LocationStatus::Ready);
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn items_move_into_a_folder_made_on_a_server() {
    let fake = fake_with_files().await;
    let (_home, core, _l) = core_with(&fake, true).await;
    eventually("server location", || core.location(SERVER).is_some()).await;
    let photos = Uri::parse("lautta://nv-account:1/photos").unwrap();
    let pair = core.make_folder(&photos, b"Pair").await.unwrap();
    let started = core
        .copy_or_move(
            OperationKind::Move,
            vec![Uri::parse("lautta://nv-account:1/photos/a.jpg").unwrap()],
            pair,
        )
        .await
        .unwrap();
    let Started::Transfer(id) = started else {
        panic!("a small move starts at once")
    };
    let done = core.engine.wait_for(id, |s| s.state.is_finished()).await.unwrap();
    assert_eq!(done.state, TransferState::Completed);
    assert_eq!(
        fake.file("account:1", b"photos/Pair/a.jpg").as_deref(),
        Some(&b"jpeg bytes"[..])
    );
    assert!(!fake.exists("account:1", b"photos/a.jpg"));
}
