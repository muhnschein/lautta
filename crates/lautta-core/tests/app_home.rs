// SPDX-License-Identifier: LGPL-2.1-or-later

mod bridge_support;

use bridge_support::{config, eventually, status};
use lautta_bridge_proto::fake::{Consent, FakeBridge, FakeDrive, Listener};
use lautta_core::app::Core;
use lautta_core::app_home::{LayoutRow, RecentLook, RecentTile, Row, SECTION_KIND};
use lautta_core::bridge::{feature, BridgeStatus, Level};
use lautta_core::date_group::DateGroup;
use lautta_core::home_layout::SECTION_ITEM;
use lautta_core::locations::LocationRegistry;
use lautta_core::org::{RecentKind, RecentsFilter};
use lautta_core::paths::AppPaths;
use lautta_core::settings::{LocationPrefs, Settings};
use lautta_core::volumes::VolumeWords;
use lautta_core::{ErrorKind, Uri};
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::Arc;

struct Env {
    home: tempfile::TempDir,
    core: Arc<Core>,
    _listener: Option<Listener>,
}

async fn env(fake: Option<&FakeBridge>, folders: &[&str]) -> Env {
    open(fake, fake.is_some(), folders).await
}

async fn open(fake: Option<&FakeBridge>, bridge: bool, folders: &[&str]) -> Env {
    let home = tempfile::tempdir().unwrap();
    for d in folders {
        std::fs::create_dir_all(home.path().join(d)).unwrap();
    }
    std::fs::create_dir_all(home.path().join("media/SD")).unwrap();
    let paths = AppPaths::new(home.path());
    let listener = fake.map(|f| f.listen(&paths.bridge_socket()).unwrap());
    let registry = LocationRegistry::with_media_root(paths.clone(), home.path().join("media"));
    let bridge = bridge.then(|| config(&paths));
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
    status(e.core.bridge.as_ref().unwrap(), BridgeStatus::Ready).await;
    eventually("the account", || e.core.location("nv-account:1").is_some()).await;
    (fake, e)
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn standalone_shows_no_trace_of_the_bridge() {
    let e = env(None, &["Documents", "Music", "android_storage/DCIM"]).await;
    let rows = e.core.home_rows().await;
    assert!(of(&rows, "servers").is_empty());
    assert!(of(&rows, "nearby").is_empty());
    let device = of(&rows, "device");
    assert_eq!(device[0].name, "Documents");
    assert_eq!(device[0].uri, "lautta://user-documents/");
    assert_eq!(device[1].name, "Music");
    let deleted = device.last().unwrap();
    assert_eq!(deleted.kind, "deleted");
    assert_eq!(of(&rows, "android")[0].name, "DCIM");
    assert_eq!(
        (device[0].icon.as_str(), of(&rows, "android")[0].icon.as_str()),
        ("file-folder", "file-folder")
    );
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
                    "deleted" => r.kind.to_owned(),
                    _ => r.name.clone(),
                },
                r.hidden,
            )
        })
        .collect()
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn hidden_rows_leave_home_but_not_its_settings() {
    let e = env(None, &["Documents", "Music", "Pictures"]).await;
    e.core
        .favourites
        .add(&uri("lautta://user-documents/"), "Docs", None, true)
        .unwrap();
    e.core
        .home_layout
        .set_hidden("device", "user-music", true)
        .unwrap();
    e.core.home_layout.set_hidden("device", "deleted", true).unwrap();
    let rows = e.core.home_rows().await;
    assert_eq!(names(&rows, "device"), ["Documents", "Pictures"]);
    assert_eq!(names(&rows, "favourites"), ["Docs"]);
    let layout = e.core.home_layout_rows().await;
    assert_eq!(
        layout_of(&layout, "device"),
        [
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
        .home_layout
        .set_hidden("device", "user-music", false)
        .unwrap();
    assert_eq!(
        names(&e.core.home_rows().await, "device"),
        ["Documents", "Pictures", "Music"]
    );
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn rows_move_within_their_section() {
    let e = env(None, &["Documents", "Music", "Pictures"]).await;
    let core = &e.core;
    core.move_home_row("device", "user-pictures", 0).await.unwrap();
    core.move_home_row("device", "deleted", 1).await.unwrap();
    let rows = core.home_rows().await;
    assert_eq!(
        names(&rows, "device")[..2],
        ["Pictures".to_owned(), String::new()]
    );
    assert_eq!(of(&rows, "device")[1].kind, "deleted");
    assert_eq!(
        names(&rows, "device")[2..],
        ["Documents".to_owned(), "Music".to_owned()]
    );
    core.move_home_row("device", "user-pictures", 99).await.unwrap();
    assert_eq!(
        names(&core.home_rows().await, "device")
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
    core.move_home_row("favourites", &a.id.to_string(), 1)
        .await
        .unwrap();
    assert_eq!(names(&core.home_rows().await, "favourites"), ["B", "A"]);
    for (section, item) in [
        ("device", "nowhere"),
        ("favourites", "x"),
        ("android", "user-music"),
    ] {
        let err = core.move_home_row(section, item, 0).await.unwrap_err();
        assert_eq!(err.kind, ErrorKind::NotFound, "{section}/{item}");
    }
    core.home_layout.reset().unwrap();
    let rows = core.home_rows().await;
    assert_eq!(names(&rows, "device")[..2], ["Documents", "Pictures"]);
    assert_eq!(
        names(&rows, "favourites"),
        ["B", "A"],
        "favourites keep their own order"
    );
}

fn kinds(rows: &[Row], section: &str) -> Vec<&'static str> {
    of(rows, section).iter().map(|r| r.kind).collect()
}

fn record(core: &Core, name: &str, kind: RecentKind, at_ms: i64) {
    let at = uri(&format!("lautta://user-documents/{name}"));
    core.recents.record_at(&at, name, kind, at_ms).unwrap();
}

fn strip(rows: &[Row]) -> Vec<RecentTile> {
    match of(rows, "recents").as_slice() {
        [] => Vec::new(),
        [row] => {
            assert_eq!(row.kind, "recent-strip");
            row.tiles.clone()
        }
        more => panic!("one strip, not {}", more.len()),
    }
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn the_newest_recents_lead_home_as_tiles() {
    let e = env(None, &["Documents"]).await;
    let core = &e.core;
    assert!(
        strip(&core.home_rows().await).is_empty(),
        "nothing recent: no section"
    );

    record(core, "old.txt", RecentKind::Opened, 1);
    record(core, "a.jpg", RecentKind::Opened, 2);
    record(core, "a.jpg", RecentKind::Transferred, 4);
    assert!(
        strip(&core.home_rows().await).is_empty(),
        "two files, one used twice: still no section"
    );
    record(core, "b.pdf", RecentKind::Previewed, 3);
    assert_eq!(
        strip(&core.home_rows().await).len(),
        3,
        "three files fill the tiles"
    );
    record(core, "c.ogg", RecentKind::Opened, 5);
    let rows = core.home_rows().await;
    assert_eq!(rows[0].section, "recents", "above favourites and folders");
    let tiles = strip(&rows);
    let told: Vec<(&str, &str, &str, &str)> = tiles
        .iter()
        .map(|t| {
            (
                t.name.as_str(),
                t.kind,
                t.look.category,
                t.look.mime_type.as_str(),
            )
        })
        .collect();
    assert_eq!(
        told,
        [
            ("c.ogg", "opened", "audio", "audio/ogg"),
            ("a.jpg", "transferred", "image", "image/jpeg"),
            ("b.pdf", "previewed", "pdf", "application/pdf"),
            ("old.txt", "opened", "text", "text/plain"),
        ],
        "newest first, a file once however often it was used"
    );
    assert_eq!(tiles[1].uri, "lautta://user-documents/a.jpg");
    assert!(tiles.iter().all(|t| t.id > 0));
    let ids: std::collections::BTreeSet<i64> = tiles.iter().map(|t| t.id).collect();
    assert_eq!(ids.len(), 4);

    for i in 0..9 {
        record(core, &format!("more{i}.txt"), RecentKind::Opened, 10 + i);
    }
    let tiles = strip(&core.home_rows().await);
    assert_eq!(tiles.len(), 12, "twelve at most");
    assert_eq!(tiles[0].name, "more8.txt");
    assert_eq!(tiles[11].name, "b.pdf");
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_file_tile_is_what_a_recent_tile_shows() {
    let e = env(None, &["Documents"]).await;
    let core = &e.core;
    let tile = core.file_tile(&uri("lautta://user-documents/a b.jpg"), &Settings::default());
    assert_eq!(
        (
            tile.id,
            tile.kind,
            tile.name.as_str(),
            tile.look.category,
            tile.look.mime_type.as_str()
        ),
        (0, "", "a b.jpg", "image", "image/jpeg")
    );
    assert_eq!(tile.uri, "lautta://user-documents/a%20b.jpg");
    assert_eq!(
        tile.look.thumbnail_source,
        format!("file://{}/Documents/a%20b.jpg", e.home.path().display())
    );
    let none = Settings {
        thumbnails_local: false,
        ..Settings::default()
    };
    assert_eq!(
        core.file_tile(&uri("lautta://user-documents/a b.jpg"), &none)
            .look
            .thumbnail_source,
        ""
    );
    let far = core.file_tile(&uri("lautta://nv-account:1/far.jpg"), &Settings::default());
    assert_eq!(
        far.look.thumbnail_source,
        "image://lautta-thumb/lautta://nv-account:1/far.jpg"
    );
    let note = core.file_tile(&uri("lautta://user-documents/run.sh"), &Settings::default());
    assert!(note.look.text_thumb && note.look.text_mono && !tile.look.text_thumb);
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn tiles_get_thumbnails_as_folders_do() {
    let e = env(None, &["Documents"]).await;
    let core = &e.core;
    let remote = uri("lautta://nv-account:1/far.jpg");
    core.recents
        .record_at(&remote, "far.jpg", RecentKind::Opened, 1)
        .unwrap();
    record(core, "note.txt", RecentKind::Opened, 2);
    record(core, "a b.jpg", RecentKind::Opened, 3);
    record(core, "c.ogg", RecentKind::Opened, 4);
    let sources = |settings: &Settings| -> Vec<String> {
        core.recent_tiles(9, settings)
            .unwrap()
            .into_iter()
            .map(|t| t.look.thumbnail_source)
            .collect()
    };
    let local = format!("file://{}/Documents/a%20b.jpg", e.home.path().display());
    assert_eq!(
        sources(&Settings::default()),
        [
            "image://lautta-thumb/lautta://user-documents/c.ogg",
            local.as_str(),
            "",
            "image://lautta-thumb/lautta://nv-account:1/far.jpg",
        ],
        "audio: app; local picture: system thumbnailer; text: glyph; server picture: app"
    );
    let no_remote = Settings {
        thumbnails_remote: false,
        ..Settings::default()
    };
    assert_eq!(sources(&no_remote)[3], "", "Thumbnails from servers off");
    assert_eq!(sources(&no_remote)[1], local);
    let none = Settings {
        thumbnails_local: false,
        ..Settings::default()
    };
    assert_eq!(sources(&none), ["", "", "", ""], "Show thumbnails off");
    assert_eq!(core.recent_tiles(2, &Settings::default()).unwrap().len(), 2);
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn recents_rows_look_as_their_tiles_under_the_thumbnail_settings() {
    let e = env(None, &["Documents"]).await;
    let core = &e.core;
    let remote = uri("lautta://nv-account:1/far.jpg");
    core.recents
        .record_at(&remote, "far.jpg", RecentKind::Transferred, 1)
        .unwrap();
    let remote_text = uri("lautta://nv-account:1/run.sh");
    core.recents
        .record_at(&remote_text, "run.sh", RecentKind::Opened, 2)
        .unwrap();
    record(core, "note.txt", RecentKind::Opened, 3);
    record(core, "a b.jpg", RecentKind::Previewed, 4);
    let local = format!("file://{}/Documents/a%20b.jpg", e.home.path().display());
    let looks = |settings: Settings| {
        core.apply_settings(settings.clone());
        let rows: Vec<RecentLook> = core
            .recent_rows(&RecentsFilter::default())
            .unwrap()
            .into_iter()
            .map(|r| r.look)
            .collect();
        let tiles: Vec<RecentLook> = core
            .recent_tiles(9, &settings)
            .unwrap()
            .into_iter()
            .map(|t| t.look)
            .collect();
        assert_eq!(rows, tiles, "a row looks as its tile on Home");
        rows.into_iter()
            .map(|l| (l.thumbnail_source, l.text_thumb))
            .collect::<Vec<_>>()
    };
    let server = "image://lautta-thumb/lautta://nv-account:1/far.jpg".to_owned();
    let none = || (String::new(), false);
    let text = || (String::new(), true);
    assert_eq!(
        looks(Settings::default()),
        [(local.clone(), false), text(), text(), (server, false)],
        "local picture: system thumbnailer; text: first lines, also from a server; server picture: app"
    );
    let no_remote = Settings {
        thumbnails_remote: false,
        ..Settings::default()
    };
    assert_eq!(
        looks(no_remote),
        [(local, false), text(), none(), none()],
        "Thumbnails from servers off"
    );
    let off = Settings {
        thumbnails_local: false,
        ..Settings::default()
    };
    assert_eq!(
        looks(off),
        [none(), none(), none(), none()],
        "Show thumbnails off"
    );
    let rows = core.recent_rows(&RecentsFilter::default()).unwrap();
    assert_eq!(
        rows.iter()
            .map(|r| (
                r.look.category,
                r.look.mime_type.as_str(),
                r.look.text_mono,
                r.look.type_icon
            ))
            .collect::<Vec<_>>(),
        [
            ("image", "image/jpeg", false, "types/text"),
            ("text", "text/plain", false, "types/text"),
            ("code", "application/x-shellscript", true, "types/shell"),
            ("image", "image/jpeg", false, "types/text"),
        ]
    );
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn the_recents_section_follows_its_switch_and_the_recents_setting() {
    let e = env(None, &["Documents"]).await;
    let core = &e.core;
    record(core, "a.jpg", RecentKind::Opened, 1);
    record(core, "b.jpg", RecentKind::Opened, 2);
    record(core, "c.jpg", RecentKind::Opened, 3);
    assert_eq!(strip(&core.home_rows().await).len(), 3);
    assert_eq!(
        kinds(&core.home_rows().await, "device"),
        ["folder", "deleted"],
        "no Recents row among the folders"
    );
    let layout = core.home_layout_rows().await;
    assert_eq!(layout[0].section, "recents", "first in Settings › Home page");
    assert_eq!(
        section_of(&layout, "recents"),
        [switch(true)],
        "the strip is no row to hide or move"
    );

    set_section(core, "recents", false);
    assert!(strip(&core.home_rows().await).is_empty());
    assert_eq!(
        section_of(&core.home_layout_rows().await, "recents"),
        [switch(false)]
    );
    set_section(core, "recents", true);
    assert_eq!(strip(&core.home_rows().await).len(), 3);

    core.apply_settings(Settings {
        recents_enabled: false,
        ..Settings::default()
    });
    assert!(
        strip(&core.home_rows().await).is_empty(),
        "kept rows stay out of sight"
    );
    assert!(
        section_of(&core.home_layout_rows().await, "recents").is_empty(),
        "nothing to switch while recents are off"
    );
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn the_device_tells_its_space_when_it_has_user_folders() {
    let e = env(None, &["Documents"]).await;
    let space = e.core.device_space().await.unwrap();
    assert!(space.total > 0 && space.free <= space.total, "{space:?}");
    let android_only = env(None, &["android_storage/DCIM"]).await;
    assert_eq!(android_only.core.device_space().await, None);
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn volumes_have_space_and_favourites_have_places() {
    let e = env(None, &["Documents/Uni"]).await;
    e.core.locations.refresh_volumes();
    let rows = e.core.home_rows().await;
    let volume = of(&rows, "volumes");
    assert_eq!(volume.len(), 1);
    assert_eq!(volume[0].name, "SD");
    assert!(volume[0].total > 0 && volume[0].free >= 0 && volume[0].free <= volume[0].total);
    let uni = uri("lautta://user-documents/Uni");
    e.core
        .favourites
        .add(&uni, "Thesis", Some("#e7a33c"), true)
        .unwrap();
    let rows = e.core.home_rows().await;
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
    let rows = e.core.home_rows().await;
    let fav = of(&rows, "favourites");
    let shown: Vec<_> = fav
        .iter()
        .map(|r| (r.name.as_str(), r.place.as_str(), r.icon.as_str(), r.is_file))
        .collect();
    assert_eq!(
        shown,
        [
            ("Thesis", "Documents › Uni", "file-document", true),
            ("Photo", "Documents", "file-image", true),
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
    let rows = e.core.home_rows().await;
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
    let rows = e.core.home_rows().await;
    assert_eq!(of(&rows, "favourites")[0].icon, "image://theme/icon-m-favorite");
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn location_display_names_apply() {
    let e = env(None, &["Documents"]).await;
    let mut prefs = LocationPrefs::new("user-documents");
    prefs.display_name = Some("Papers".into());
    e.core.location_prefs.set(&prefs).unwrap();
    let rows = e.core.home_rows().await;
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
    let rows = e.core.home_rows().await;
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
    assert_eq!(servers[0].icon, "server");
}

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
    let rows = e.core.home_rows().await;
    assert_eq!(
        volumes(&rows),
        [
            volume("CARD", "unmounted", "mmcblk1p1", "vol-CARD"),
            volume("SD", "ready", "", "vol-SD"),
            volume("USB drive", "unmounted", "sda1", "vol-1234-ABCD"),
        ]
    );
    let vols = of(&rows, "volumes");
    assert_eq!(vols[2].icon, "image://theme/icon-m-usb");
    assert_eq!(vols[0].icon, "image://theme/icon-m-sd-card");
    assert_eq!(vols[1].icon, "image://theme/icon-m-usb");
    assert_eq!((vols[0].uri.as_str(), vols[0].total, vols[0].free), ("", 64, -1));
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn an_unlabelled_drive_is_named_by_kind_before_and_after_mounting() {
    let (_fake, e) = drives_env().await;
    let client = e.core.bridge.clone().unwrap();
    e.core.locations.set_volume_words(VolumeWords {
        card: "Card".into(),
        usb: "Stick".into(),
    });
    let rows = e.core.home_rows().await;
    assert_eq!(
        volumes(&rows)[2],
        volume("Stick", "unmounted", "sda1", "vol-1234-ABCD")
    );
    let root = e.core.mount_drive("sda1").await.unwrap();
    assert_eq!(
        root.to_string(),
        "lautta://vol-1234-ABCD/",
        "id from the mount folder"
    );
    eventually("the mount", || client.drives()[0].mount_point.is_some()).await;
    let rows = e.core.home_rows().await;
    assert_eq!(
        volumes(&rows)[2],
        volume("Stick", "ready", "sda1", "vol-1234-ABCD")
    );
    assert_eq!(of(&rows, "volumes")[2].icon, "image://theme/icon-m-usb");
    assert_eq!(e.core.place_of(&root), "Stick");
    assert_eq!(
        e.core.place_of(&uri("lautta://vol-1234-ABCD/DCIM")),
        "Stick › DCIM"
    );
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_mounted_drive_opens_and_unmounts() {
    let (fake, e) = drives_env().await;
    let client = e.core.bridge.clone().unwrap();
    let root = e.core.mount_drive("mmcblk1p1").await.unwrap();
    assert_eq!(root.to_string(), "lautta://vol-CARD/");
    assert!(e.core.location("vol-CARD").is_some());
    eventually("the mount", || client.drives()[1].mount_point.is_some()).await;
    let rows = e.core.home_rows().await;
    assert_eq!(
        volumes(&rows)[0],
        volume("CARD", "ready", "mmcblk1p1", "vol-CARD")
    );
    let card = of(&rows, "volumes")[0];
    assert_eq!(card.uri, "lautta://vol-CARD/");
    assert!(card.total > 0, "the space of the mounted volume");

    e.core.unmount_drive("mmcblk1p1").await.unwrap();
    assert!(e.core.location("vol-CARD").is_none(), "gone at once");
    assert!(fake.drives()[1].mount_point.is_empty());
    eventually("the unmount", || client.drives()[1].mount_point.is_none()).await;
    let rows = e.core.home_rows().await;
    assert_eq!(
        volumes(&rows)[0],
        volume("CARD", "unmounted", "mmcblk1p1", "vol-CARD")
    );

    let e2 = e.core.mount_drive("sdb1").await.unwrap_err();
    assert_eq!(e2.kind, ErrorKind::NotFound);
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_locked_drive_is_a_locked_volume_until_it_is_unlocked() {
    let (fake, e) = drives_env().await;
    let client = e.core.bridge.clone().unwrap();
    let mut drives = fake.drives();
    drives.push(
        FakeDrive::new("mmcblk1p2", "", "sd", 32)
            .with_uuid("5c1e0a4f")
            .locked(),
    );
    fake.set_drives(drives).await;
    eventually("the locked drive", || client.drives().len() == 3).await;
    let rows = e.core.home_rows().await;
    assert_eq!(
        volumes(&rows),
        [
            volume("CARD", "unmounted", "mmcblk1p1", "vol-CARD"),
            volume("Memory card", "locked", "mmcblk1p2", "vol-5c1e0a4f"),
            volume("SD", "ready", "", "vol-SD"),
            volume("USB drive", "unmounted", "sda1", "vol-1234-ABCD"),
        ]
    );
    let vault = of(&rows, "volumes")[1];
    assert_eq!(
        (vault.uri.as_str(), vault.total, vault.icon.as_str()),
        ("", 32, "image://theme/icon-m-sd-card")
    );

    let mut drives = fake.drives();
    drives.retain(|d| d.id != "mmcblk1p2");
    drives.push(FakeDrive::new("dm_2d0", "SECRET", "sd", 31));
    fake.set_drives(drives).await;
    eventually("the unlocked drive", || {
        client.drives().iter().any(|d| d.id == "dm_2d0")
    })
    .await;
    let rows = e.core.home_rows().await;
    assert_eq!(
        volumes(&rows),
        [
            volume("CARD", "unmounted", "mmcblk1p1", "vol-CARD"),
            volume("SD", "ready", "", "vol-SD"),
            volume("SECRET", "unmounted", "dm_2d0", "vol-SECRET"),
            volume("USB drive", "unmounted", "sda1", "vol-1234-ABCD"),
        ]
    );
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_drive_mounted_out_of_sight_is_an_error() {
    let (fake, e) = drives_env().await;
    fake.set_mount_root(None);
    let err = e.core.mount_drive("mmcblk1p1").await.unwrap_err();
    assert_eq!(err.kind, ErrorKind::NotFound);
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn without_drives_the_volumes_are_as_before() {
    let standalone = env(None, &["Documents"]).await;
    let (_fake, older) = ready_env().await;
    for e in [standalone, older] {
        let rows = e.core.home_rows().await;
        assert_eq!(volumes(&rows), [volume("SD", "ready", "", "vol-SD")]);
        assert_eq!(of(&rows, "volumes")[0].icon, "image://theme/icon-m-usb");
        assert!(e.core.mount_drive("mmcblk1p1").await.is_err());
    }
}

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
        connection_of(&e.core.home_rows().await),
        state("disconnected", "")
    );
    fake.set_state("account:1", "failed", Some("AuthFailed")).await;
    eventually("failed", || {
        e.core
            .location("nv-account:1")
            .is_some_and(|l| l.connection.name() == "failed")
    })
    .await;
    let rows = e.core.home_rows().await;
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
    assert_eq!(connection_of(&e.core.home_rows().await), state("connected", ""));
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn consent_row_until_decided_and_nothing_when_denied() {
    let fake = account_fake().await;
    fake.set_consent(Consent::Unknown).await;
    let e = env(Some(&fake), &["Documents"]).await;
    let client = e.core.bridge.clone().unwrap();
    status(&client, BridgeStatus::ConsentUnknown).await;
    let rows = e.core.home_rows().await;
    let servers = of(&rows, "servers");
    assert_eq!(servers.len(), 1);
    assert_eq!((servers[0].kind, servers[0].icon.as_str()), ("consent", "server"));

    fake.set_consent(Consent::Denied).await;
    status(&client, BridgeStatus::Ready).await;
    assert!(of(&e.core.home_rows().await, "servers").is_empty());
    assert_eq!(
        section_of(&e.core.home_layout_rows().await, "servers"),
        [switch(true)],
        "the bridge is there, so is the switch of its section"
    );
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn no_socket_means_no_servers_section() {
    let e = open(None, true, &["Documents"]).await;
    assert_eq!(e.core.bridge.as_ref().unwrap().status(), BridgeStatus::Absent);
    let rows = e.core.home_rows().await;
    assert!(of(&rows, "servers").is_empty());
    assert!(of(&rows, "nearby").is_empty());
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn reconnecting_shows_a_notice_and_busy_rows() {
    let (fake, e) = ready_env().await;
    fake.set_accepting(false);
    fake.disconnect_clients().await;
    let client = e.core.bridge.clone().unwrap();
    status(&client, BridgeStatus::Reconnecting).await;
    eventually("busy rows", || {
        e.core
            .location("nv-account:1")
            .is_some_and(|l| l.status == lautta_core::locations::LocationStatus::Connecting)
    })
    .await;
    let rows = e.core.home_rows().await;
    let servers = of(&rows, "servers");
    assert_eq!(servers[0].kind, "unavailable");
    assert_eq!(
        (servers[1].name.as_str(), servers[1].status),
        ("NAS", "connecting")
    );
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
    let rows = e.core.home_rows().await;
    let nearby = of(&rows, "nearby");
    assert_eq!(nearby.len(), 1);
    assert_eq!(nearby[0].uri, "sftp://raspberrypi.local/");
    assert_eq!(nearby[0].provider, "SFTP");

    let switch = |layout: &[LayoutRow]| layout_of(layout, "nearby");
    assert_eq!(switch(&e.core.home_layout_rows().await), [(String::new(), false)]);
    e.core.home_layout.set_hidden("nearby", "nearby", true).unwrap();
    assert!(of(&e.core.home_rows().await, "nearby").is_empty());
    assert_eq!(switch(&e.core.home_layout_rows().await), [(String::new(), true)]);
    assert_eq!(of(&e.core.home_rows().await, "servers").len(), 1);
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
    core.home_layout
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
            .home_layout
            .load()
            .unwrap()
            .is_hidden("servers", &format!("nv-account:{n}"))
}

fn counted_purge(core: &Arc<Core>) -> Arc<AtomicUsize> {
    remember_account(core, 9);
    let purges = Arc::new(AtomicUsize::new(0));
    let p = purges.clone();
    core.spawn_account_purge(move || {
        p.fetch_add(1, Ordering::SeqCst);
    });
    purges
}

async fn purged(purges: &AtomicUsize, n: usize) {
    eventually(&format!("purge {n}"), || purges.load(Ordering::SeqCst) >= n).await;
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
    let purges = counted_purge(&e.core);
    purged(&purges, 1).await;
    assert!(!remembered(&e.core, 9));
    assert!(
        remembered(&e.core, 1) && remembered(&e.core, 2),
        "present accounts stay"
    );

    fake.remove_location("account:2").await;
    purged(&purges, 2).await;
    assert!(!remembered(&e.core, 2));
    assert!(remembered(&e.core, 1));
    assert!(e.core.favourites.is_favourite(&local).unwrap());
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn withdrawn_consent_and_lost_connection_purge_nothing() {
    let (fake, e) = ready_env().await;
    remember_account(&e.core, 1);
    let purges = counted_purge(&e.core);
    let client = e.core.bridge.clone().unwrap();
    purged(&purges, 1).await;
    fake.set_consent(Consent::Unknown).await;
    status(&client, BridgeStatus::ConsentUnknown).await;
    eventually("the list is cleared", || client.locations().is_empty()).await;
    fake.set_consent(Consent::Allowed(Level::Full)).await;
    status(&client, BridgeStatus::Ready).await;
    fake.add_account(3, "sftp", "Office", "office.home").await;
    eventually("third account", || e.core.location("nv-account:3").is_some()).await;
    remember_account(&e.core, 3);
    fake.remove_location("account:3").await;
    purged(&purges, 2).await;
    assert!(!remembered(&e.core, 3));
    assert!(
        remembered(&e.core, 1),
        "an empty list without consent is not a removal"
    );
    assert_eq!(purges.load(Ordering::SeqCst), 2, "nothing else purged");
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn an_account_gone_before_the_purge_saw_it_listed_is_purged_on_the_next_change() {
    let (fake, e) = ready_env().await;
    remember_account(&e.core, 1);
    let purges = counted_purge(&e.core);
    purged(&purges, 1).await;
    remember_account(&e.core, 3);
    fake.add_account(4, "sftp", "Office", "office.home").await;
    purged(&purges, 2).await;
    assert!(!remembered(&e.core, 3));
    assert!(remembered(&e.core, 1));
    assert_eq!(purges.load(Ordering::SeqCst), 2, "nothing else purged");
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn accounts_the_app_may_not_use_are_not_purged() {
    let fake = account_fake().await;
    fake.add_account(2, "sftp", "Office", "office.home").await;
    fake.set_access("account:2", None).await;
    let e = env(Some(&fake), &["Documents"]).await;
    let client = e.core.bridge.clone().unwrap();
    status(&client, BridgeStatus::Ready).await;
    eventually("the account", || e.core.location("nv-account:1").is_some()).await;
    remember_account(&e.core, 1);
    remember_account(&e.core, 2);
    let purges = counted_purge(&e.core);
    purged(&purges, 1).await;
    assert!(!remembered(&e.core, 9));
    assert!(
        remembered(&e.core, 2),
        "an account hidden from the app is not a removal"
    );
    assert!(remembered(&e.core, 1));
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn accounts_removed_while_the_app_was_closed_are_purged_at_start() {
    let (_fake, e) = ready_env().await;
    remember_account(&e.core, 1);
    remember_account(&e.core, 9);
    let local = uri("lautta://user-documents/x");
    e.core.favourites.add(&local, "Local", None, true).unwrap();
    e.core.spawn_account_purge(|| {});
    eventually("purge", || !remembered(&e.core, 9)).await;
    assert!(remembered(&e.core, 1));
    assert!(e.core.favourites.is_favourite(&local).unwrap());
}

fn section_of(rows: &[LayoutRow], section: &str) -> Vec<(&'static str, String, bool)> {
    rows.iter()
        .filter(|r| r.section == section)
        .map(|r| (r.kind, r.item.clone(), r.hidden))
        .collect()
}

fn switch(on: bool) -> (&'static str, String, bool) {
    (SECTION_KIND, SECTION_ITEM.to_owned(), !on)
}

fn row_of(kind: &'static str, item: &str, hidden: bool) -> (&'static str, String, bool) {
    (kind, item.to_owned(), hidden)
}

fn set_section(core: &Core, section: &str, on: bool) {
    core.home_layout.set_hidden(section, SECTION_ITEM, !on).unwrap();
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn the_volumes_switch_hides_the_section_and_keeps_the_choices_of_its_rows() {
    let e = env(None, &["Documents"]).await;
    let core = &e.core;
    core.locations.refresh_volumes();
    let sd = row_of("volume", "vol-SD", false);
    assert_eq!(
        section_of(&core.home_layout_rows().await, "volumes"),
        [switch(true), sd.clone()],
        "the switch leads the section"
    );
    assert_eq!(names(&core.home_rows().await, "volumes"), ["SD"]);

    set_section(core, "volumes", false);
    assert!(
        of(&core.home_rows().await, "volumes").is_empty(),
        "off: gone from Home"
    );
    assert_eq!(
        section_of(&core.home_layout_rows().await, "volumes"),
        [switch(false)],
        "off: its rows are gone from the settings, not dimmed"
    );
    assert!(
        !of(&core.home_rows().await, "device").is_empty(),
        "other sections stay"
    );

    set_section(core, "volumes", true);
    assert_eq!(names(&core.home_rows().await, "volumes"), ["SD"]);

    core.home_layout.set_hidden("volumes", "vol-SD", true).unwrap();
    set_section(core, "volumes", false);
    set_section(core, "volumes", true);
    assert!(of(&core.home_rows().await, "volumes").is_empty());
    assert_eq!(
        section_of(&core.home_layout_rows().await, "volumes"),
        [switch(true), row_of("volume", "vol-SD", true)]
    );
    core.home_layout.set_hidden("volumes", "vol-SD", false).unwrap();
    assert_eq!(names(&core.home_rows().await, "volumes"), ["SD"]);
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn the_volumes_switch_is_there_without_volumes() {
    let e = env(None, &["Documents"]).await;
    let core = &e.core;
    std::fs::remove_dir(e.home.path().join("media/SD")).unwrap();
    core.locations.refresh_volumes();
    assert_eq!(
        section_of(&core.home_layout_rows().await, "volumes"),
        [switch(true)],
        "on and empty, the section is its switch"
    );
    assert!(
        of(&core.home_rows().await, "volumes").is_empty(),
        "and Home shows nothing of it"
    );
    set_section(core, "volumes", false);
    assert_eq!(
        section_of(&core.home_layout_rows().await, "volumes"),
        [switch(false)],
        "off, it can be turned on again"
    );
    set_section(core, "volumes", true);
    assert_eq!(
        section_of(&core.home_layout_rows().await, "volumes"),
        [switch(true)]
    );
    assert!(of(&core.home_rows().await, "volumes").is_empty());

    std::fs::create_dir_all(e.home.path().join("media/SD")).unwrap();
    core.locations.refresh_volumes();
    assert_eq!(
        section_of(&core.home_layout_rows().await, "volumes"),
        [switch(true), row_of("volume", "vol-SD", false)]
    );
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn the_servers_switch_is_there_while_the_bridge_is_without_servers() {
    let fake = FakeBridge::new();
    let e = env(Some(&fake), &["Documents"]).await;
    status(e.core.bridge.as_ref().unwrap(), BridgeStatus::Ready).await;
    assert_eq!(
        section_of(&e.core.home_layout_rows().await, "servers"),
        [switch(true)],
        "no server added yet: the section is its switch"
    );
    assert!(
        of(&e.core.home_rows().await, "servers").is_empty(),
        "and Home shows nothing of it, no empty header"
    );
    set_section(&e.core, "servers", false);
    assert_eq!(
        section_of(&e.core.home_layout_rows().await, "servers"),
        [switch(false)]
    );
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn the_servers_switch_hides_servers_that_come_later_and_restores_the_rows() {
    let (fake, e) = ready_env().await;
    let core = &e.core;
    assert_eq!(
        section_of(&core.home_layout_rows().await, "servers"),
        [switch(true), row_of("account", "nv-account:1", false)]
    );
    core.home_layout
        .set_hidden("servers", "nv-account:1", true)
        .unwrap();
    set_section(core, "servers", false);
    assert!(of(&core.home_rows().await, "servers").is_empty());
    assert_eq!(
        section_of(&core.home_layout_rows().await, "servers"),
        [switch(false)]
    );

    fake.add_account(2, "sftp", "Office", "office.home").await;
    eventually("second account", || core.location("nv-account:2").is_some()).await;
    assert!(
        of(&core.home_rows().await, "servers").is_empty(),
        "a server added later"
    );
    assert_eq!(
        section_of(&core.home_layout_rows().await, "servers"),
        [switch(false)]
    );

    set_section(core, "servers", true);
    assert_eq!(
        names(&core.home_rows().await, "servers"),
        ["Office"],
        "NAS stays hidden"
    );
    assert_eq!(
        section_of(&core.home_layout_rows().await, "servers"),
        [
            switch(true),
            row_of("account", "nv-account:1", true),
            row_of("account", "nv-account:2", false)
        ]
    );
    assert_eq!(of(&core.home_rows().await, "nearby").len(), 0);
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn the_settings_list_keeps_the_sections_in_home_order() {
    let fake = account_fake().await;
    let e = env(Some(&fake), &["Documents", "android_storage/DCIM"]).await;
    status(e.core.bridge.as_ref().unwrap(), BridgeStatus::Ready).await;
    eventually("the account", || e.core.location("nv-account:1").is_some()).await;
    e.core
        .favourites
        .add(&uri("lautta://user-documents/"), "Docs", None, true)
        .unwrap();
    let mut order: Vec<&str> = e
        .core
        .home_layout_rows()
        .await
        .iter()
        .map(|r| r.section)
        .collect();
    order.dedup();
    assert_eq!(
        order,
        [
            "recents",
            "favourites",
            "device",
            "android",
            "volumes",
            "servers",
            "nearby"
        ]
    );
    set_section(&e.core, "volumes", false);
    set_section(&e.core, "servers", false);
    let mut order: Vec<&str> = e
        .core
        .home_layout_rows()
        .await
        .iter()
        .map(|r| r.section)
        .collect();
    order.dedup();
    assert_eq!(
        order,
        [
            "recents",
            "favourites",
            "device",
            "android",
            "volumes",
            "servers",
            "nearby"
        ]
    );
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn moving_a_server_leaves_the_switch_out_of_the_order() {
    let (fake, e) = ready_env().await;
    let core = &e.core;
    fake.add_account(2, "sftp", "Office", "office.home").await;
    eventually("second account", || core.location("nv-account:2").is_some()).await;
    core.move_home_row("servers", "nv-account:2", 0).await.unwrap();
    assert_eq!(
        section_of(&core.home_layout_rows().await, "servers"),
        [
            switch(true),
            row_of("account", "nv-account:2", false),
            row_of("account", "nv-account:1", false)
        ]
    );
    let layout = core.home_layout.load().unwrap();
    assert_eq!(layout.position("servers", SECTION_ITEM), None);
    assert_eq!(layout.position("servers", "nv-account:2"), Some(0));
    assert_eq!(layout.position("servers", "nv-account:1"), Some(1));
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn servers_off_hides_the_consent_notice_too() {
    let fake = account_fake().await;
    fake.set_consent(Consent::Unknown).await;
    let e = env(Some(&fake), &["Documents"]).await;
    let client = e.core.bridge.clone().unwrap();
    status(&client, BridgeStatus::ConsentUnknown).await;
    assert_eq!(
        section_of(&e.core.home_layout_rows().await, "servers"),
        [switch(true)],
        "the notice is no row of the settings, the bridge's switch is"
    );
    set_section(&e.core, "servers", false);
    assert!(of(&e.core.home_rows().await, "servers").is_empty());
    assert_eq!(
        section_of(&e.core.home_layout_rows().await, "servers"),
        [switch(false)]
    );
    set_section(&e.core, "servers", true);
    assert_eq!(of(&e.core.home_rows().await, "servers")[0].kind, "consent");
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn standalone_has_no_servers_switch_even_when_it_was_off() {
    let e = env(None, &["Documents"]).await;
    set_section(&e.core, "servers", false);
    assert!(section_of(&e.core.home_layout_rows().await, "servers").is_empty());

    let home = tempfile::tempdir().unwrap();
    let paths = AppPaths::new(home.path());
    let registry = LocationRegistry::with_media_root(paths.clone(), home.path().join("media"));
    let core = Core::open_full(paths.clone(), registry, Some(config(&paths)))
        .await
        .unwrap();
    set_section(&core, "servers", false);
    assert_eq!(core.bridge.as_ref().unwrap().status(), BridgeStatus::Absent);
    assert!(section_of(&core.home_layout_rows().await, "servers").is_empty());
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_client_without_netvfs_has_no_nearby_switch() {
    let e = open(None, true, &["Documents"]).await;
    assert_eq!(e.core.bridge.as_ref().unwrap().status(), BridgeStatus::Absent);
    assert!(section_of(&e.core.home_layout_rows().await, "nearby").is_empty());
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn restore_defaults_switches_both_sections_on() {
    let (_fake, e) = ready_env().await;
    let core = &e.core;
    set_section(core, "volumes", false);
    set_section(core, "servers", false);
    assert!(of(&core.home_rows().await, "servers").is_empty());
    core.home_layout.reset().unwrap();
    assert_eq!(names(&core.home_rows().await, "servers"), ["NAS"]);
    assert_eq!(names(&core.home_rows().await, "volumes"), ["SD"]);
    let layout = core.home_layout_rows().await;
    assert_eq!(section_of(&layout, "servers")[0], switch(true));
    assert_eq!(section_of(&layout, "volumes")[0], switch(true));
}
