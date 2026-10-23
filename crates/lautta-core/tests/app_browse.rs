// SPDX-License-Identifier: LGPL-2.1-or-later
//! The rows of Browse, what the user hid and moved, recent ad-hoc servers and
//! the purge of removed accounts (SPEC §15.2, BRW-12, NVB-3, NVB-6, NVB-12,
//! SEC-5).

mod bridge_support;

use bridge_support::{config, eventually};
use lautta_bridge_proto::fake::{Consent, FakeBridge, FakeDrive, Listener};
use lautta_core::app::Core;
use lautta_core::app_browse::{LayoutRow, Row};
use lautta_core::bridge::{feature, AdHocOptions, BridgeStatus};
use lautta_core::date_group::DateGroup;
use lautta_core::locations::LocationRegistry;
use lautta_core::org::{RecentKind, RecentsFilter};
use lautta_core::paths::AppPaths;
use lautta_core::settings::{LocationPrefs, Settings};
use lautta_core::{ErrorKind, Uri};
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
    assert_eq!(device[0].kind, "recents");
    assert_eq!(device[1].name, "Documents");
    assert_eq!(device[1].uri, "lautta://user-documents/");
    assert_eq!(device[1].count, 1, "hidden files are not counted");
    assert_eq!(device[2].name, "Music");
    let deleted = device.last().unwrap();
    assert_eq!((deleted.kind, deleted.count), ("deleted", 0));
    assert_eq!(of(&rows, "android")[0].name, "DCIM");
}

fn names(rows: &[Row], section: &str) -> Vec<String> {
    of(rows, section).iter().map(|r| r.name.clone()).collect()
}

fn layout_of(rows: &[LayoutRow], section: &str) -> Vec<(String, bool)> {
    rows.iter()
        .filter(|r| r.section == section)
        .map(|r| {
            (
                match r.kind {
                    "deleted" | "recents" => r.kind.to_owned(),
                    _ => r.name.clone(),
                },
                r.hidden,
            )
        })
        .collect()
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn hidden_rows_leave_browse_but_not_its_settings() {
    let e = env(None, &["Documents", "Music", "Pictures"]).await;
    e.core
        .favourites
        .add(&uri("lautta://user-documents/"), "Docs", None, true)
        .unwrap();
    e.core
        .browse_layout
        .set_hidden("device", "user-music", true)
        .unwrap();
    e.core
        .browse_layout
        .set_hidden("device", "deleted", true)
        .unwrap();
    let rows = e.core.browse_rows().await;
    assert_eq!(
        names(&rows, "device"),
        ["", "Documents", "Pictures"],
        "Recents and the folders"
    );
    assert_eq!(names(&rows, "favourites"), ["Docs"]);
    let layout = e.core.browse_layout_rows().await;
    assert_eq!(
        layout_of(&layout, "device"),
        [
            ("recents".to_owned(), false),
            ("Documents".to_owned(), false),
            ("Pictures".to_owned(), false),
            ("Music".to_owned(), true),
            ("deleted".to_owned(), true)
        ]
    );
    assert_eq!(layout_of(&layout, "favourites"), [("Docs".to_owned(), false)]);
    assert!(
        layout_of(&layout, "nearby").is_empty(),
        "no nearby switch without the bridge"
    );
    e.core
        .browse_layout
        .set_hidden("device", "user-music", false)
        .unwrap();
    assert_eq!(
        names(&e.core.browse_rows().await, "device"),
        ["", "Documents", "Pictures", "Music"]
    );
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn rows_move_within_their_section() {
    let e = env(None, &["Documents", "Music", "Pictures"]).await;
    let core = &e.core;
    core.move_browse_row("device", "user-pictures", 0).await.unwrap();
    core.move_browse_row("device", "deleted", 1).await.unwrap();
    let rows = core.browse_rows().await;
    assert_eq!(
        names(&rows, "device")[..2],
        ["Pictures".to_owned(), String::new()]
    );
    assert_eq!(of(&rows, "device")[1].kind, "deleted");
    assert_eq!(
        names(&rows, "device")[2..],
        [String::new(), "Documents".to_owned(), "Music".to_owned()]
    );
    core.move_browse_row("device", "user-pictures", 99).await.unwrap();
    assert_eq!(
        names(&core.browse_rows().await, "device")
            .last()
            .map(String::as_str),
        Some("Pictures"),
        "an index past the end moves to the end"
    );
    let a = core
        .favourites
        .add(&uri("lautta://user-documents/"), "A", None, true)
        .unwrap();
    core.favourites
        .add(&uri("lautta://user-music/"), "B", None, true)
        .unwrap();
    core.move_browse_row("favourites", &a.id.to_string(), 1)
        .await
        .unwrap();
    assert_eq!(names(&core.browse_rows().await, "favourites"), ["B", "A"]);
    for (section, item) in [
        ("device", "nowhere"),
        ("favourites", "x"),
        ("android", "user-music"),
    ] {
        let err = core.move_browse_row(section, item, 0).await.unwrap_err();
        assert_eq!(err.kind, ErrorKind::NotFound, "{section}/{item}");
    }
    core.browse_layout.reset().unwrap();
    let rows = core.browse_rows().await;
    assert_eq!(names(&rows, "device")[..3], ["", "Documents", "Pictures"]);
    assert_eq!(
        names(&rows, "favourites"),
        ["B", "A"],
        "favourites keep their own order"
    );
}

fn kinds(rows: &[Row], section: &str) -> Vec<&'static str> {
    of(rows, section).iter().map(|r| r.kind).collect()
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn recents_is_a_device_row_while_recents_are_remembered() {
    let e = env(None, &["Documents"]).await;
    let core = &e.core;
    let rows = core.browse_rows().await;
    assert_eq!(kinds(&rows, "device"), ["recents", "folder", "deleted"]);
    let recents = of(&rows, "device")[0];
    assert_eq!(recents.icon, "image://theme/icon-m-history");
    assert_eq!((recents.count, recents.uri.as_str()), (-1, ""));

    core.browse_layout.set_hidden("device", "recents", true).unwrap();
    assert_eq!(kinds(&core.browse_rows().await, "device"), ["folder", "deleted"]);
    assert_eq!(
        layout_of(&core.browse_layout_rows().await, "device")[0],
        ("recents".to_owned(), true),
        "hidden, it stays in the settings"
    );
    core.browse_layout.set_hidden("device", "recents", false).unwrap();
    core.move_browse_row("device", "recents", 1).await.unwrap();
    assert_eq!(
        kinds(&core.browse_rows().await, "device"),
        ["folder", "recents", "deleted"]
    );

    core.apply_settings(Settings {
        recents_enabled: false,
        ..Settings::default()
    });
    assert_eq!(kinds(&core.browse_rows().await, "device"), ["folder", "deleted"]);
    assert!(
        layout_of(&core.browse_layout_rows().await, "device")
            .iter()
            .all(|(name, _)| name != "recents"),
        "nothing to switch while recents are off"
    );
}

/// Recents leads *On this device* by default (§15.2); an order the user
/// gave the section stays, Restore defaults brings Recents back to the top.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn recents_comes_first_until_the_user_orders_the_rows() {
    let e = env(None, &["Documents", "Music"]).await;
    let core = &e.core;
    let device = |rows: &[Row]| -> Vec<String> {
        of(rows, "device")
            .iter()
            .map(|r| {
                if r.name.is_empty() {
                    r.kind.to_owned()
                } else {
                    r.name.clone()
                }
            })
            .collect()
    };
    let natural = ["recents", "Documents", "Music", "deleted"];
    assert_eq!(device(&core.browse_rows().await), natural);
    assert_eq!(
        layout_of(&core.browse_layout_rows().await, "device")[0],
        ("recents".to_owned(), false),
        "and first in Settings › Browse page"
    );

    let order: Vec<String> = ["user-documents", "user-music", "recents", "deleted"]
        .iter()
        .map(ToString::to_string)
        .collect();
    core.browse_layout.set_order("device", &order).unwrap();
    assert_eq!(
        device(&core.browse_rows().await),
        ["Documents", "Music", "recents", "deleted"],
        "a saved order stays"
    );
    core.browse_layout.reset().unwrap();
    assert_eq!(device(&core.browse_rows().await), natural);
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
    e.core
        .favourites
        .add(&uni, "Thesis", Some("#e7a33c"), true)
        .unwrap();
    let rows = e.core.browse_rows().await;
    let fav = of(&rows, "favourites");
    assert_eq!(
        (fav[0].kind, fav[0].name.as_str(), fav[0].place.as_str()),
        ("favourite", "Thesis", "Documents › Uni")
    );
    assert_eq!(fav[0].colour, "#e7a33c");
    assert_eq!(fav[0].icon, "image://theme/icon-m-favorite");
    assert!(!fav[0].is_file);
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn file_favourites_name_their_folder_and_icon() {
    let e = env(None, &["Documents/Uni"]).await;
    let pdf = uri("lautta://user-documents/Uni/thesis.pdf");
    let photo = uri("lautta://user-documents/photo.jpg");
    e.core.favourites.add(&pdf, "Thesis", None, false).unwrap();
    e.core
        .favourites
        .add(&photo, "Photo", Some("#e7a33c"), false)
        .unwrap();
    let rows = e.core.browse_rows().await;
    let fav = of(&rows, "favourites");
    let shown: Vec<_> = fav
        .iter()
        .map(|r| (r.name.as_str(), r.place.as_str(), r.icon.as_str(), r.is_file))
        .collect();
    assert_eq!(
        shown,
        [
            ("Thesis", "Documents › Uni", "image://theme/icon-m-document", true),
            ("Photo", "Documents", "image://theme/icon-m-file-image", true),
        ],
        "the folder a file is in, and the file's icon"
    );
    assert_eq!((fav[1].kind, fav[1].colour.as_str()), ("favourite", "#e7a33c"));
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn favourites_show_their_chosen_icon() {
    let e = env(None, &["Documents/Uni"]).await;
    let uni = uri("lautta://user-documents/Uni");
    let pdf = uri("lautta://user-documents/Uni/thesis.pdf");
    let favourites = &e.core.favourites;
    favourites
        .save(&uni, "Uni", Some("#4fa3e5"), Some("home"), true)
        .unwrap();
    favourites
        .save(&pdf, "Thesis", None, Some("company"), false)
        .unwrap();
    let rows = e.core.browse_rows().await;
    let shown: Vec<_> = of(&rows, "favourites")
        .iter()
        .map(|r| (r.name.as_str(), r.icon.as_str(), r.colour.as_str()))
        .collect();
    assert_eq!(
        shown,
        [
            ("Uni", "image://theme/icon-m-home", "#4fa3e5"),
            ("Thesis", "image://theme/icon-m-company", ""),
        ],
        "the chosen icons, tinted as before"
    );
    favourites.save(&uni, "Uni", None, None, true).unwrap();
    let rows = e.core.browse_rows().await;
    assert_eq!(of(&rows, "favourites")[0].icon, "image://theme/icon-m-favorite");
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn location_display_names_apply() {
    let e = env(None, &["Documents"]).await;
    let mut prefs = LocationPrefs::new("user-documents");
    prefs.display_name = Some("Papers".into());
    e.core.location_prefs.set(&prefs).unwrap();
    let rows = e.core.browse_rows().await;
    assert_eq!(of(&rows, "device")[1].name, "Papers");
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
    // The sections are the Activity page's (§15.4), by calendar day.
    let day = 86_400_000;
    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_millis() as i64;
    for (name, days) in [("old.txt", 12), ("older.txt", 400)] {
        let at = uri(&format!("lautta://user-documents/{name}"));
        e.core
            .recents
            .record_at(&at, name, RecentKind::Opened, now - days * day)
            .unwrap();
    }
    let rows = e.core.recent_rows(&RecentsFilter::default()).unwrap();
    assert_eq!(rows.len(), 4);
    assert_eq!(rows[1].place, "Documents › Uni");
    assert_eq!(
        (rows[1].kind, rows[1].date_group),
        ("previewed", DateGroup::Today)
    );
    assert_eq!(rows[0].place, "Documents", "transfers name the location");
    let sections: Vec<DateGroup> = rows.iter().map(|r| r.date_group).collect();
    assert_eq!(
        sections,
        [
            DateGroup::Today,
            DateGroup::Today,
            DateGroup::LastThirtyDays,
            DateGroup::Older
        ]
    );
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

/// A bridge with the drives (netvfs XB-20) that mounts them in the media
/// folder: an unlabelled USB drive and a memory card, neither mounted.
async fn drives_env() -> (FakeBridge, Env) {
    let fake = FakeBridge::new();
    fake.set_features(Some(&[feature::STORAGE_SETTINGS, feature::DRIVES]));
    fake.set_drives(vec![
        FakeDrive::new("sda1", "", "usb", 8).with_uuid("1234-ABCD"),
        FakeDrive::new("mmcblk1p1", "CARD", "sd", 64),
    ])
    .await;
    let e = env(Some(&fake), &["Documents"]).await;
    fake.set_mount_root(Some(&e.home.path().join("media")));
    let client = e.core.bridge.clone().unwrap();
    eventually("the drives", || client.drives().len() == 2).await;
    (fake, e)
}

/// The volume rows as (name, status, drive, item id).
fn volumes(rows: &[Row]) -> Vec<(String, &'static str, String, String)> {
    of(rows, "volumes")
        .iter()
        .map(|r| (r.name.clone(), r.status, r.drive.clone(), r.item_id.clone()))
        .collect()
}

fn volume(
    name: &str,
    status: &'static str,
    drive: &str,
    item: &str,
) -> (String, &'static str, String, String) {
    (name.to_owned(), status, drive.to_owned(), item.to_owned())
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn drives_show_as_volumes_until_they_are_mounted() {
    let (_fake, e) = drives_env().await;
    let rows = e.core.browse_rows().await;
    // By the folder each is mounted on, among the mounted volumes (LOC-3).
    assert_eq!(
        volumes(&rows),
        [
            volume("1234-ABCD", "unmounted", "sda1", "vol-1234-ABCD"),
            volume("CARD", "unmounted", "mmcblk1p1", "vol-CARD"),
            volume("SD", "ready", "", "vol-SD"),
        ]
    );
    let vols = of(&rows, "volumes");
    assert_eq!(vols[0].icon, "image://theme/icon-m-usb");
    assert_eq!(vols[1].icon, "image://theme/icon-m-sd-card");
    assert_eq!((vols[1].uri.as_str(), vols[1].total, vols[1].free), ("", 64, -1));
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_mounted_drive_opens_and_unmounts() {
    let (fake, e) = drives_env().await;
    let client = e.core.bridge.clone().unwrap();
    let root = e.core.mount_drive("mmcblk1p1").await.unwrap();
    assert_eq!(root.to_string(), "lautta://vol-CARD/");
    assert!(e.core.location("vol-CARD").is_some());
    eventually("the mount", || client.drives()[1].mount_point.is_some()).await;
    let rows = e.core.browse_rows().await;
    assert_eq!(
        volumes(&rows)[1],
        volume("CARD", "ready", "mmcblk1p1", "vol-CARD")
    );
    let card = of(&rows, "volumes")[1];
    assert_eq!(card.uri, "lautta://vol-CARD/");
    assert!(card.total > 0, "the space of the mounted volume");

    e.core.unmount_drive("mmcblk1p1").await.unwrap();
    assert!(e.core.location("vol-CARD").is_none(), "gone at once");
    assert!(fake.drives()[1].mount_point.is_empty());
    eventually("the unmount", || client.drives()[1].mount_point.is_none()).await;
    let rows = e.core.browse_rows().await;
    assert_eq!(
        volumes(&rows)[1],
        volume("CARD", "unmounted", "mmcblk1p1", "vol-CARD")
    );

    let e2 = e.core.mount_drive("sdb1").await.unwrap_err();
    assert_eq!(e2.kind, ErrorKind::NotFound);
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_drive_mounted_out_of_sight_is_an_error() {
    // The bridge mounts it where the sandbox does not see it.
    let (fake, e) = drives_env().await;
    fake.set_mount_root(None);
    let err = e.core.mount_drive("mmcblk1p1").await.unwrap_err();
    assert_eq!(err.kind, ErrorKind::NotFound);
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn without_drives_the_volumes_are_as_before() {
    // Standalone, and a netvfs without XB-20.
    let standalone = env(None, &["Documents"]).await;
    let (_fake, older) = ready_env().await;
    for e in [standalone, older] {
        let rows = e.core.browse_rows().await;
        assert_eq!(volumes(&rows), [volume("SD", "ready", "", "vol-SD")]);
        assert_eq!(of(&rows, "volumes")[0].icon, "image://theme/icon-m-sd-card");
        assert!(e.core.mount_drive("mmcblk1p1").await.is_err());
    }
}

/// What a server row says about its connection (§15.2, NVB-4).
fn connection_of(rows: &[Row]) -> (&'static str, String) {
    let server = of(rows, "servers")
        .into_iter()
        .find(|r| r.kind == "account")
        .unwrap();
    (server.connection, server.connection_error.clone())
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn server_rows_show_the_connection_state() {
    let (fake, e) = ready_env().await;
    let state = |s: &'static str, error: &str| (s, error.to_owned());
    assert_eq!(
        connection_of(&e.core.browse_rows().await),
        state("disconnected", "")
    );
    fake.set_state("account:1", "failed", Some("AuthFailed")).await;
    eventually("failed", || {
        e.core
            .location("nv-account:1")
            .is_some_and(|l| l.connection.name() == "failed")
    })
    .await;
    let rows = e.core.browse_rows().await;
    assert_eq!(connection_of(&rows), state("failed", "AuthFailed"));
    assert_eq!(
        of(&rows, "servers")[0].status,
        "ready",
        "a failure is no attention"
    );
    fake.set_state("account:1", "connected", None).await;
    eventually("connected", || {
        e.core
            .location("nv-account:1")
            .is_some_and(|l| l.connection.name() == "connected")
    })
    .await;
    assert_eq!(connection_of(&e.core.browse_rows().await), state("connected", ""));
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
    // Whatever the bridge said last, it is connecting again (NVB-12).
    assert_eq!(servers[1].connection, "connecting");
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
    }])
    .await;
    eventually("nearby", || !client.nearby().is_empty()).await;
    let rows = e.core.browse_rows().await;
    let nearby = of(&rows, "nearby");
    assert_eq!(nearby.len(), 1);
    assert_eq!(nearby[0].uri, "sftp://raspberrypi.local/");
    assert_eq!(nearby[0].provider, "SFTP");

    // One switch hides every nearby server (BRW-12).
    let switch = |layout: &[LayoutRow]| layout_of(layout, "nearby");
    assert_eq!(
        switch(&e.core.browse_layout_rows().await),
        [(String::new(), false)]
    );
    e.core.browse_layout.set_hidden("nearby", "nearby", true).unwrap();
    assert!(of(&e.core.browse_rows().await, "nearby").is_empty());
    assert_eq!(
        switch(&e.core.browse_layout_rows().await),
        [(String::new(), true)]
    );
    assert_eq!(of(&e.core.browse_rows().await, "servers").len(), 1);
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
    assert_eq!(adhoc[0].connection, "connected");

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
    assert_eq!(recent[0].connection, "disconnected");
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
    core.favourites.add(&u, "Photos", None, true).unwrap();
    core.recents.record(&u, "photos", RecentKind::Opened).unwrap();
    core.location_prefs
        .set(&LocationPrefs {
            display_name: Some("x".into()),
            ..LocationPrefs::new(&format!("nv-account:{n}"))
        })
        .unwrap();
    core.browse_layout
        .set_hidden("servers", &format!("nv-account:{n}"), true)
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
        || core
            .location_prefs
            .get(&format!("nv-account:{n}"))
            .unwrap()
            .display_name
            .is_some()
        || core
            .browse_layout
            .load()
            .unwrap()
            .is_hidden("servers", &format!("nv-account:{n}"))
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_removed_account_is_forgotten() {
    let (fake, e) = ready_env().await;
    fake.add_account(2, "sftp", "Office", "office.home").await;
    eventually("second account", || e.core.location("nv-account:2").is_some()).await;
    remember_account(&e.core, 1);
    remember_account(&e.core, 2);
    let local = uri("lautta://user-documents/");
    e.core.favourites.add(&local, "Docs", None, true).unwrap();
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
    e.core.favourites.add(&adhoc, "Adhoc", None, true).unwrap();
    e.core.spawn_account_purge(|| {});
    eventually("purge", || !remembered(&e.core, 9)).await;
    assert!(remembered(&e.core, 1));
    assert!(e.core.favourites.is_favourite(&adhoc).unwrap());
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
