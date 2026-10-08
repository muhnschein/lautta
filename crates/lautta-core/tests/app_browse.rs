// SPDX-License-Identifier: LGPL-2.1-or-later
//! The rows of Browse, recent ad-hoc servers and the purge of removed
//! accounts (SPEC §15.2, NVB-3, NVB-6, NVB-12, SEC-5, ORG-3).

mod bridge_support;

use bridge_support::{config, eventually};
use lautta_bridge_proto::fake::{Consent, FakeBridge, Listener};
use lautta_core::app::Core;
use lautta_core::app_browse::Row;
use lautta_core::bridge::{AdHocOptions, BridgeStatus};
use lautta_core::locations::LocationRegistry;
use lautta_core::org::{RecentKind, RecentsFilter};
use lautta_core::paths::AppPaths;
use lautta_core::settings::LocationPrefs;
use lautta_core::Uri;
use std::sync::Arc;
use std::time::Duration;

struct Env {
    home: tempfile::TempDir,
    core: Arc<Core>,
    _listener: Option<Listener>,
}

async fn env(fake: Option<&FakeBridge>, folders: &[&str]) -> Env {
    let home = tempfile::tempdir().unwrap();
    for d in folders {
        std::fs::create_dir_all(home.path().join(d)).unwrap();
    }
    std::fs::create_dir_all(home.path().join("media/SD")).unwrap();
    let paths = AppPaths::new(home.path());
    let listener = fake.map(|f| f.listen(&paths.bridge_socket()).unwrap());
    let registry = LocationRegistry::with_media_root(paths.clone(), home.path().join("media"));
    let bridge = fake.map(|_| config(&paths));
    let core = Core::open_full(paths, registry, bridge).await.unwrap();
    Env {
        home,
        core,
        _listener: listener,
    }
}

fn uri(s: &str) -> Uri {
    Uri::parse(s).unwrap()
}

fn of<'a>(rows: &'a [Row], section: &str) -> Vec<&'a Row> {
    rows.iter().filter(|r| r.section == section).collect()
}

async fn account_fake() -> FakeBridge {
    let fake = FakeBridge::new();
    fake.add_account(1, "sftp", "NAS", "nas.home").await;
    fake
}

async fn ready_env() -> (FakeBridge, Env) {
    let fake = account_fake().await;
    let e = env(Some(&fake), &["Documents"]).await;
    e.core
        .bridge
        .as_ref()
        .unwrap()
        .wait_status(|s| s == BridgeStatus::Ready)
        .await;
    eventually("the account", || e.core.location("nv-account:1").is_some()).await;
    (fake, e)
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn standalone_shows_no_trace_of_the_bridge() {
    let e = env(None, &["Documents", "Music", "android_storage/DCIM"]).await;
    std::fs::write(e.home.path().join("Documents/a.txt"), "").unwrap();
    std::fs::write(e.home.path().join("Documents/.hidden"), "").unwrap();
    let rows = e.core.browse_rows().await;
    assert!(of(&rows, "servers").is_empty());
    assert!(of(&rows, "nearby").is_empty());
    let device = of(&rows, "device");
    assert_eq!(device[0].name, "Documents");
    assert_eq!(device[0].uri, "lautta://user-documents/");
    assert_eq!(device[0].count, 1, "hidden files are not counted");
    assert_eq!(device[1].name, "Music");
    let deleted = device.last().unwrap();
    assert_eq!((deleted.kind, deleted.count), ("deleted", 0));
    assert_eq!(of(&rows, "android")[0].name, "DCIM");
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn volumes_have_space_and_favourites_have_places() {
    let e = env(None, &["Documents/Uni"]).await;
    e.core.locations.refresh_volumes();
    let rows = e.core.browse_rows().await;
    let volume = of(&rows, "volumes");
    assert_eq!(volume.len(), 1);
    assert_eq!(volume[0].name, "SD");
    assert!(volume[0].total > 0 && volume[0].free >= 0 && volume[0].free <= volume[0].total);
    let uni = uri("lautta://user-documents/Uni");
    e.core.favourites.add(&uni, "Thesis", Some("#e7a33c")).unwrap();
    e.core
        .sync_pairs
        .add(&lautta_core::org::SyncPairSpec::new(
            "Camera → NAS",
            uri("lautta://user-documents/"),
            uni.clone(),
        ))
        .unwrap();
    let rows = e.core.browse_rows().await;
    let fav = of(&rows, "favourites");
    assert_eq!(
        (fav[0].kind, fav[0].name.as_str(), fav[0].place.as_str()),
        ("favourite", "Thesis", "Documents › Uni")
    );
    assert_eq!(fav[0].colour, "#e7a33c");
    assert_eq!((fav[1].kind, fav[1].mode.as_str()), ("syncpair", "update_both"));
    assert_eq!(fav[1].right_uri, uni.to_string());
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn tags_come_with_counts_and_missing_items() {
    let e = env(None, &["Documents"]).await;
    let a = uri("lautta://user-documents/a.txt");
    let b = uri("lautta://user-documents/b.txt");
    std::fs::write(e.home.path().join("Documents/a.txt"), "x").unwrap();
    std::fs::write(e.home.path().join("Documents/b.txt"), "x").unwrap();
    let work = e.core.tags.create("Work", "#e5604f").unwrap();
    e.core.tags.create("Empty", "#4fa3e5").unwrap();
    e.core.tags.assign(work.id, &[a.clone(), b.clone()]).unwrap();
    // A server that is not there says nothing about its items.
    let far = uri("lautta://nv-account:5/x.txt");
    e.core.tags.assign(work.id, std::slice::from_ref(&far)).unwrap();
    let rows = e.core.browse_rows().await;
    let tags = of(&rows, "tags");
    assert_eq!((tags[0].name.as_str(), tags[0].count), ("Work", 3));
    assert_eq!((tags[1].name.as_str(), tags[1].count), ("Empty", 0));

    std::fs::remove_file(e.home.path().join("Documents/b.txt")).unwrap();
    assert_eq!(e.core.check_tag_missing().await.unwrap(), 1);
    let items = e.core.tagged_rows(work.id).unwrap();
    assert_eq!(items.len(), 3);
    assert!(!items[0].missing && !items[1].missing);
    assert_eq!(
        (items[2].name.as_str(), items[2].place.as_str(), items[2].missing),
        ("b.txt", "Documents", true)
    );
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn location_display_names_apply() {
    let e = env(None, &["Documents"]).await;
    let mut prefs = LocationPrefs::new("user-documents");
    prefs.display_name = Some("Papers".into());
    e.core.location_prefs.set(&prefs).unwrap();
    let rows = e.core.browse_rows().await;
    assert_eq!(of(&rows, "device")[0].name, "Papers");
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn recents_rows_say_where_and_when() {
    let e = env(None, &["Documents/Uni"]).await;
    let f = uri("lautta://user-documents/Uni/notes.txt");
    e.core
        .recents
        .record(&f, "notes.txt", RecentKind::Previewed)
        .unwrap();
    e.core
        .recents
        .record(
            &uri("lautta://user-documents/x.pdf"),
            "x.pdf",
            RecentKind::Transferred,
        )
        .unwrap();
    let rows = e.core.recent_rows(&RecentsFilter::default()).unwrap();
    assert_eq!(rows.len(), 2);
    assert_eq!(rows[1].place, "Documents › Uni");
    assert_eq!((rows[1].kind, rows[1].day), ("previewed", "today"));
    assert_eq!(rows[0].place, "Documents", "transfers name the location");
    let only = RecentsFilter {
        text: Some("NOTES".into()),
        ..RecentsFilter::default()
    };
    assert_eq!(e.core.recent_rows(&only).unwrap().len(), 1);
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn servers_follow_the_bridge() {
    let (fake, e) = ready_env().await;
    fake.set_attention("account:1", Some("auth-failed")).await;
    eventually("attention", || {
        e.core
            .location("nv-account:1")
            .is_some_and(|l| l.attention.is_some())
    })
    .await;
    let rows = e.core.browse_rows().await;
    let servers = of(&rows, "servers");
    assert_eq!(servers.len(), 1);
    assert_eq!(
        (
            servers[0].kind,
            servers[0].name.as_str(),
            servers[0].provider.as_str()
        ),
        ("account", "NAS", "SFTP")
    );
    assert_eq!(
        (
            servers[0].status,
            servers[0].attention.as_str(),
            servers[0].host.as_str()
        ),
        ("attention", "auth-failed", "nas.home")
    );
    assert_eq!(servers[0].uri, "lautta://nv-account:1/");
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn consent_row_until_allowed_and_nothing_when_denied() {
    let fake = account_fake().await;
    fake.set_consent(Consent::Unknown).await;
    let e = env(Some(&fake), &["Documents"]).await;
    let client = e.core.bridge.clone().unwrap();
    client.wait_status(|s| s == BridgeStatus::ConsentUnknown).await;
    let rows = e.core.browse_rows().await;
    let servers = of(&rows, "servers");
    assert_eq!(servers.len(), 1);
    assert_eq!(servers[0].kind, "consent");

    fake.set_consent(Consent::Denied).await;
    client.wait_status(|s| s == BridgeStatus::ConsentDenied).await;
    assert!(of(&e.core.browse_rows().await, "servers").is_empty());
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn no_socket_means_no_servers_section() {
    let fake = account_fake().await;
    let e = env(Some(&fake), &["Documents"]).await;
    // The bridge client is there but nothing listens: standalone (UI-8).
    drop(e._listener);
    let rows = e.core.browse_rows().await;
    assert!(of(&rows, "servers").is_empty());
    assert!(of(&rows, "nearby").is_empty());
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn reconnecting_shows_a_notice_and_busy_rows() {
    let (fake, e) = ready_env().await;
    fake.set_accepting(false);
    fake.disconnect_clients().await;
    let client = e.core.bridge.clone().unwrap();
    client.wait_status(|s| s == BridgeStatus::Reconnecting).await;
    eventually("busy rows", || {
        e.core
            .location("nv-account:1")
            .is_some_and(|l| l.status == lautta_core::locations::LocationStatus::Connecting)
    })
    .await;
    let rows = e.core.browse_rows().await;
    let servers = of(&rows, "servers");
    assert_eq!(servers[0].kind, "unavailable");
    assert_eq!(
        (servers[1].name.as_str(), servers[1].status),
        ("NAS", "connecting")
    );
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn nearby_servers_prefill_the_connect_form() {
    let (fake, e) = ready_env().await;
    let client = e.core.bridge.clone().unwrap();
    client.discover(true).await.unwrap();
    fake.set_nearby(vec![lautta_bridge_proto::WireNearby {
        name: "raspberrypi".into(),
        provider: "sftp".into(),
        host: "raspberrypi.local".into(),
        port: 22,
        path: Vec::new(),
    }])
    .await;
    eventually("nearby", || !client.nearby().is_empty()).await;
    let rows = e.core.browse_rows().await;
    let nearby = of(&rows, "nearby");
    assert_eq!(nearby.len(), 1);
    assert_eq!(nearby[0].uri, "sftp://raspberrypi.local/");
    assert_eq!(nearby[0].provider, "SFTP");
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn adhoc_servers_are_remembered_and_forgotten() {
    let (fake, e) = ready_env().await;
    let id = e
        .core
        .connect_adhoc(
            "sftp://pi@host.example:2222/docs",
            b"pw".to_vec(),
            AdHocOptions::default(),
        )
        .await
        .unwrap();
    assert_eq!(fake.received_secrets(), vec![b"pw".to_vec()]);
    let recents = e.core.adhoc_recents().unwrap();
    assert_eq!(recents.len(), 1);
    assert_eq!(recents[0].url, "sftp://pi@host.example:2222/docs");
    assert_eq!(recents[0].name, "host.example:2222");
    eventually("the ad-hoc location", || e.core.location(&id).is_some()).await;
    let rows = e.core.browse_rows().await;
    let adhoc: Vec<&Row> = of(&rows, "servers")
        .into_iter()
        .filter(|r| r.kind == "adhoc")
        .collect();
    assert_eq!(adhoc.len(), 1, "a connected server is not listed twice");

    // The bridge forgets it (another session): it stays as a recent one.
    fake.remove_location(&id.replace("nv-", "")).await;
    eventually("gone", || e.core.location(&id).is_none()).await;
    let rows = e.core.browse_rows().await;
    let recent: Vec<&Row> = of(&rows, "servers")
        .into_iter()
        .filter(|r| r.kind == "adhocRecent")
        .collect();
    assert_eq!(recent.len(), 1);
    assert_eq!(
        (
            recent[0].status,
            recent[0].provider.as_str(),
            recent[0].uri.as_str()
        ),
        ("offline", "SFTP", "sftp://pi@host.example:2222/docs")
    );
    e.core.remove_adhoc_recent(&id).unwrap();
    assert!(e.core.adhoc_recents().unwrap().is_empty());
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_failed_connect_stores_nothing() {
    let (_fake, e) = ready_env().await;
    let err = e
        .core
        .connect_adhoc("gopher://host/", b"pw".to_vec(), AdHocOptions::default())
        .await
        .unwrap_err();
    assert_eq!(err.kind, lautta_core::ErrorKind::Unsupported);
    assert!(e.core.adhoc_recents().unwrap().is_empty());
}

fn remember_account(core: &Core, n: i32) -> Uri {
    let u = uri(&format!("lautta://nv-account:{n}/photos"));
    core.favourites.add(&u, "Photos", None).unwrap();
    let tag = core.tags.create(&format!("t{n}"), "#fff").unwrap();
    core.tags.assign(tag.id, std::slice::from_ref(&u)).unwrap();
    core.recents.record(&u, "photos", RecentKind::Opened).unwrap();
    core.location_prefs
        .set(&LocationPrefs {
            display_name: Some("x".into()),
            ..LocationPrefs::new(&format!("nv-account:{n}"))
        })
        .unwrap();
    u
}

fn remembered(core: &Core, n: i32) -> bool {
    let u = uri(&format!("lautta://nv-account:{n}/photos"));
    core.favourites.is_favourite(&u).unwrap()
        || core
            .recents
            .list(&RecentsFilter::default())
            .unwrap()
            .iter()
            .any(|r| r.uri == u)
        || !core.tags.tags_for(&u).unwrap().is_empty()
        || core
            .location_prefs
            .get(&format!("nv-account:{n}"))
            .unwrap()
            .display_name
            .is_some()
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_removed_account_is_forgotten() {
    let (fake, e) = ready_env().await;
    fake.add_account(2, "sftp", "Office", "office.home").await;
    eventually("second account", || e.core.location("nv-account:2").is_some()).await;
    remember_account(&e.core, 1);
    remember_account(&e.core, 2);
    let local = uri("lautta://user-documents/");
    e.core.favourites.add(&local, "Docs", None).unwrap();
    e.core.spawn_account_purge(|| {});
    tokio::time::sleep(Duration::from_millis(100)).await;
    assert!(
        remembered(&e.core, 1) && remembered(&e.core, 2),
        "present accounts stay"
    );

    fake.remove_location("account:2").await;
    eventually("purge", || !remembered(&e.core, 2)).await;
    assert!(remembered(&e.core, 1));
    assert!(e.core.favourites.is_favourite(&local).unwrap());
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn withdrawn_consent_and_lost_connection_purge_nothing() {
    let (fake, e) = ready_env().await;
    remember_account(&e.core, 1);
    e.core.spawn_account_purge(|| {});
    let client = e.core.bridge.clone().unwrap();
    tokio::time::sleep(Duration::from_millis(100)).await;
    fake.set_consent(Consent::Unknown).await;
    client.wait_status(|s| s == BridgeStatus::ConsentUnknown).await;
    eventually("the list is cleared", || client.locations().is_empty()).await;
    tokio::time::sleep(Duration::from_millis(100)).await;
    assert!(
        remembered(&e.core, 1),
        "an empty list without consent is not a removal"
    );
    fake.set_consent(Consent::Granted).await;
    client.wait_status(|s| s == BridgeStatus::Ready).await;
    tokio::time::sleep(Duration::from_millis(100)).await;
    assert!(remembered(&e.core, 1));
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn accounts_removed_while_the_app_was_closed_are_purged_at_start() {
    let (_fake, e) = ready_env().await;
    remember_account(&e.core, 1);
    remember_account(&e.core, 9);
    // Ad-hoc ids change between runs; their data is not touched at start.
    let adhoc = uri("lautta://nv-adhoc:4/x");
    e.core.favourites.add(&adhoc, "Adhoc", None).unwrap();
    e.core.spawn_account_purge(|| {});
    eventually("purge", || !remembered(&e.core, 9)).await;
    assert!(remembered(&e.core, 1));
    assert!(e.core.favourites.is_favourite(&adhoc).unwrap());
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn tag_dialog_changes_apply_together() {
    use lautta_core::app_browse::TagChanges;
    let e = env(None, &["Documents"]).await;
    let a = uri("lautta://user-documents/a.txt");
    let b = uri("lautta://user-documents/b.txt");
    let work = e.core.tags.create("Work", "#e5604f").unwrap();
    let old = e.core.tags.create("Old", "#4fa3e5").unwrap();
    e.core.tags.assign(old.id, &[a.clone(), b.clone()]).unwrap();
    e.core.tags.assign(work.id, std::slice::from_ref(&a)).unwrap();
    let both = [a.clone(), b.clone()];
    assert_eq!(
        e.core.tag_usage(&both).unwrap(),
        vec![(work.id, 1), (old.id, 2)],
        "partly carried tags show a count"
    );
    let changes = TagChanges {
        add: vec![work.id],
        remove: vec![old.id],
        new_tag: Some(("Fresh".into(), "#9bd26a".into())),
    };
    e.core.apply_tag_changes(&both, &changes).unwrap();
    let names = |u: &Uri| -> Vec<String> {
        e.core
            .tags
            .tags_for(u)
            .unwrap()
            .into_iter()
            .map(|t| t.name)
            .collect()
    };
    assert_eq!(names(&a), ["Work", "Fresh"]);
    assert_eq!(names(&b), ["Work", "Fresh"]);
    // A name that exists changes nothing at all.
    let clash = TagChanges {
        remove: vec![work.id],
        new_tag: Some(("Work".into(), "#000".into())),
        ..TagChanges::default()
    };
    let err = e.core.apply_tag_changes(&both, &clash).unwrap_err();
    assert_eq!(err.kind, lautta_core::ErrorKind::AlreadyExists);
    assert_eq!(names(&a), ["Work", "Fresh"]);
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn the_cover_counts_the_cached_listing() {
    use lautta_core::entry::{Entry, Kind};
    let e = env(None, &["Documents"]).await;
    let docs = uri("lautta://user-documents/");
    assert_eq!(e.core.cached_folder_count(&docs), None);
    let entries = vec![
        Entry::new(b"a.txt", Kind::File),
        Entry::new(b".hidden", Kind::File),
        Entry::new(b"d", Kind::Dir),
    ];
    e.core.dircache.put(&docs, &entries).unwrap();
    assert_eq!(e.core.cached_folder_count(&docs), Some(2));
}
