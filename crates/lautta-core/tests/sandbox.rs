// SPDX-License-Identifier: LGPL-2.1-or-later
//! The app in the sandbox's file system layout (SPEC §3.3): Sailjail mounts
//! a tmpfs on `$HOME` and bind-mounts every permitted folder and the app data
//! folder onto it, each on its own, so no folder is on `$HOME`'s device and
//! `rename(2)` between any two of them fails with `EXDEV`. Without root,
//! two file systems (the temp folder and `/dev/shm`) joined by symlinks
//! stand in for the bind mounts; hosts without a separate `/dev/shm` skip
//! those tests. Also: removable volumes that come and go while the app
//! runs (LOC-3).

use lautta_core::app::Core;
use lautta_core::app_browse::Row;
use lautta_core::app_search::SearchRequest;
use lautta_core::entry::Entry;
use lautta_core::locations::LocationRegistry;
use lautta_core::paths::AppPaths;
use lautta_core::trash::now_secs;
use lautta_core::Uri;
use std::os::unix::fs::{symlink, MetadataExt};
use std::path::{Path, PathBuf};
use std::sync::atomic::AtomicBool;
use std::sync::Arc;
use std::time::Duration;
use tokio::sync::mpsc;

const DAY: i64 = 86_400;

fn uri(s: &str) -> Uri {
    Uri::parse(s).unwrap()
}

fn write(path: &Path, data: &[u8]) {
    std::fs::create_dir_all(path.parent().unwrap()).unwrap();
    std::fs::write(path, data).unwrap();
}

async fn open(home: &Path) -> Arc<Core> {
    let paths = AppPaths::new(home);
    let registry = LocationRegistry::with_media_root(paths.clone(), home.join("media"));
    Core::open_with(paths, registry).await.unwrap()
}

/// A folder on another file system than the temp folder.
fn other_fs() -> Option<tempfile::TempDir> {
    let shm = tempfile::tempdir_in("/dev/shm").ok()?;
    let probe = tempfile::tempdir().ok()?;
    let dev = |p: &Path| std::fs::metadata(p).map(|m| m.dev()).ok();
    (dev(shm.path())? != dev(probe.path())?).then_some(shm)
}

struct Sandbox {
    _disk: tempfile::TempDir,
    _shm: tempfile::TempDir,
    home: PathBuf,
    core: Arc<Core>,
}

/// `$HOME` with the app data folder and `Downloads` on its file system and
/// `Documents` on another one, as a bind mount would be.
async fn sandbox() -> Option<Sandbox> {
    let shm = other_fs()?;
    let disk = tempfile::tempdir().unwrap();
    let home = disk.path().join("home");
    std::fs::create_dir_all(home.join("Downloads")).unwrap();
    std::fs::create_dir(shm.path().join("Documents")).unwrap();
    symlink(shm.path().join("Documents"), home.join("Documents")).unwrap();
    let core = open(&home).await;
    Some(Sandbox {
        _disk: disk,
        _shm: shm,
        home,
        core,
    })
}

fn names(entries: &[Entry]) -> Vec<String> {
    let mut v: Vec<String> = entries.iter().map(Entry::display_name).collect();
    v.sort();
    v
}

fn row<'a>(rows: &'a [Row], section: &str, kind: &str, name: &str) -> Option<&'a Row> {
    rows.iter()
        .find(|r| r.section == section && r.kind == kind && r.name == name)
}

async fn search(core: &Arc<Core>, root: &Uri, text: &str) -> Vec<String> {
    let (tx, mut rx) = mpsc::channel(8);
    let (core, root) = (core.clone(), root.clone());
    let req = SearchRequest {
        text: text.to_owned(),
        include_hidden: true,
        ..SearchRequest::default()
    };
    let task = tokio::spawn(async move {
        core.search_tree(root, &req, Arc::new(AtomicBool::new(false)), tx)
            .await
    });
    let mut hits = Vec::new();
    while let Some(batch) = rx.recv().await {
        hits.extend(batch.iter().map(|h| h.uri.to_string()));
    }
    task.await.unwrap().unwrap();
    hits
}

async fn eventually(what: &str, cond: impl Fn() -> bool) {
    for _ in 0..500 {
        if cond() {
            return;
        }
        tokio::time::sleep(Duration::from_millis(10)).await;
    }
    panic!("timed out waiting for {what}");
}

// ------------------------------------------------------------ Recently deleted

#[tokio::test]
async fn deletes_go_to_the_bin_at_their_locations_root() {
    let Some(s) = sandbox().await else { return };
    write(&s.home.join("Documents/Uni/thesis.odt"), b"draft");
    let docs = uri("lautta://user-documents/");
    let item = uri("lautta://user-documents/Uni");
    let transfer = s.core.delete(std::slice::from_ref(&item)).await.unwrap();
    assert!(
        transfer.is_none(),
        "kept in Recently deleted, not deleted for good"
    );
    assert!(!s.home.join("Documents/Uni").exists());
    let items = s.core.trash.list().unwrap();
    assert_eq!(items.len(), 1);
    assert_eq!(items[0].bin, s.home.join("Documents/.lautta-trash"));
    assert_eq!(
        std::fs::read(items[0].stored_path().join("thesis.odt")).unwrap(),
        b"draft"
    );

    // The bin is not listed (hidden files or not), searched or counted.
    write(&s.home.join("Documents/.hidden"), b"h");
    let listed = s.core.list(&docs, |_| {}).await.unwrap();
    assert_eq!(names(&listed), [".hidden"]);
    assert!(search(&s.core, &docs, "thesis").await.is_empty());
    let rows = s.core.browse_rows().await;
    assert_eq!(row(&rows, "device", "folder", "Documents").unwrap().count, 0);
    assert_eq!(rows.iter().find(|r| r.kind == "deleted").unwrap().count, 1);

    assert!(s.core.undo().await.unwrap(), "undo restores by rename");
    assert_eq!(
        std::fs::read(s.home.join("Documents/Uni/thesis.odt")).unwrap(),
        b"draft"
    );
    assert!(s.core.trash.list().unwrap().is_empty());
}

#[tokio::test]
async fn deletes_are_kept_when_the_app_data_folder_is_a_mount_of_its_own() {
    let Some(shm) = other_fs() else { return };
    let disk = tempfile::tempdir().unwrap();
    // Documents on $HOME's file system, the app data folder elsewhere.
    let home = shm.path().join("home");
    std::fs::create_dir_all(home.join("Documents")).unwrap();
    symlink(disk.path(), home.join(".local")).unwrap();
    let core = open(&home).await;
    write(&home.join("Documents/a.txt"), b"a");
    let transfer = core
        .delete(&[uri("lautta://user-documents/a.txt")])
        .await
        .unwrap();
    assert!(
        transfer.is_none(),
        "kept in Recently deleted, not deleted for good"
    );
    let items = core.trash.list().unwrap();
    assert_eq!(items.len(), 1);
    core.restore(items[0].id).await.unwrap();
    assert_eq!(std::fs::read(home.join("Documents/a.txt")).unwrap(), b"a");
}

#[tokio::test]
async fn restore_purge_and_empty_cover_every_bin() {
    let Some(s) = sandbox().await else { return };
    write(&s.home.join("Documents/a.txt"), b"a");
    write(&s.home.join("Downloads/b.txt"), b"bb");
    for item in ["lautta://user-documents/a.txt", "lautta://user-downloads/b.txt"] {
        assert!(s.core.delete(&[uri(item)]).await.unwrap().is_none(), "{item}");
    }
    // Trashed by an older version, into the app data folder (no bin).
    let legacy = AppPaths::new(&s.home).trash_dir();
    write(&legacy.join("old-1"), b"ccc");
    s.core
        .db
        .lock()
        .execute(
            "INSERT INTO trash_items(original_uri, trashed_at, stored_name, is_dir, size) \
             VALUES ('lautta://user-documents/c.txt', ?1, 'old-1', 0, 3)",
            [now_secs()],
        )
        .unwrap();

    let now = now_secs();
    let mut listed: Vec<String> = s
        .core
        .trash_entries(now)
        .unwrap()
        .into_iter()
        .map(|e| e.name)
        .collect();
    listed.sort();
    assert_eq!(listed, ["a.txt", "b.txt", "c.txt"]);
    let rows = s.core.browse_rows().await;
    assert_eq!(rows.iter().find(|r| r.kind == "deleted").unwrap().count, 3);
    assert_eq!(s.core.trash.bins().unwrap().len(), 3);

    // The old item goes back across mounts.
    let c = s.core.trash.list().unwrap();
    let c = c.iter().find(|i| i.stored_name == "old-1").unwrap();
    s.core.restore(c.id).await.unwrap();
    assert_eq!(std::fs::read(s.home.join("Documents/c.txt")).unwrap(), b"ccc");
    assert!(!legacy.join("old-1").exists());

    // a.txt expired; b.txt is emptied away.
    let a = s.core.trash.list().unwrap();
    let a = a
        .iter()
        .find(|i| i.original_uri.path.as_bytes() == b"a.txt")
        .unwrap();
    s.core
        .db
        .lock()
        .execute(
            "UPDATE trash_items SET trashed_at = ?1 WHERE id = ?2",
            [now - 31 * DAY, a.id],
        )
        .unwrap();
    assert_eq!(s.core.purge_trash(now).unwrap(), 1);
    assert!(!a.stored_path().exists());
    let left = s.core.trash_entries(now).unwrap();
    assert_eq!(left.len(), 1);
    assert_eq!(left[0].name, "b.txt");
    assert_eq!(s.core.empty_trash().unwrap(), 1);
    assert!(s.core.trash.list().unwrap().is_empty());
    assert!(!s.home.join("Documents/.lautta-trash").exists());
    assert!(!s.home.join("Downloads/.lautta-trash").exists());
}

#[tokio::test]
async fn the_retention_setting_decides_what_expires() {
    let Some(s) = sandbox().await else { return };
    write(&s.home.join("Documents/a.txt"), b"a");
    s.core
        .delete(&[uri("lautta://user-documents/a.txt")])
        .await
        .unwrap();
    let now = now_secs();
    let mut settings = s.core.settings();
    settings.recently_deleted_retention_days = 7;
    s.core.apply_settings(settings);
    assert_eq!(s.core.purge_trash(now + 6 * DAY).unwrap(), 0);
    assert_eq!(s.core.purge_trash(now + 8 * DAY).unwrap(), 1);
    assert!(s.core.trash.list().unwrap().is_empty());
}

#[tokio::test]
async fn clear_app_data_removes_the_bins_in_the_locations() {
    let Some(s) = sandbox().await else { return };
    write(&s.home.join("Documents/a"), b"a");
    write(&s.home.join("Downloads/b"), b"b");
    write(&s.home.join("Downloads/keep"), b"k");
    s.core
        .delete(&[uri("lautta://user-documents/a"), uri("lautta://user-downloads/b")])
        .await
        .unwrap();
    assert_eq!(s.core.trash.list().unwrap().len(), 2);
    let bins = [
        s.home.join("Documents/.lautta-trash"),
        s.home.join("Downloads/.lautta-trash"),
    ];
    assert!(bins.iter().all(|b| b.is_dir()));
    s.core.clear_app_data().unwrap();
    for bin in &bins {
        assert!(!bin.exists(), "{bin:?}");
    }
    assert!(s.home.join("Downloads/keep").is_file(), "user files stay");
    assert!(s.core.trash.list().unwrap().is_empty());
}

// ------------------------------------------------------------ volumes

struct Plain {
    _dir: tempfile::TempDir,
    media: PathBuf,
    core: Arc<Core>,
}

async fn plain() -> Plain {
    let dir = tempfile::tempdir().unwrap();
    let home = dir.path().to_path_buf();
    std::fs::create_dir_all(home.join("Documents")).unwrap();
    std::fs::create_dir_all(home.join("media")).unwrap();
    let core = open(&home).await;
    Plain {
        media: home.join("media"),
        _dir: dir,
        core,
    }
}

#[tokio::test]
async fn a_volume_mounted_while_the_app_runs_appears_and_goes() {
    let p = plain().await;
    assert!(p.core.location("vol-USB").is_none());
    std::fs::create_dir(p.media.join("USB")).unwrap();
    eventually("the volume", || p.core.location("vol-USB").is_some()).await;
    let rows = p.core.browse_rows().await;
    assert!(row(&rows, "volumes", "volume", "USB").is_some());
    std::fs::remove_dir(p.media.join("USB")).unwrap();
    eventually("the volume to go", || p.core.location("vol-USB").is_none()).await;
}

#[tokio::test]
async fn browse_finds_volumes_the_watch_has_not_seen_yet() {
    let p = plain().await;
    // Sooner than the watch's debounce: only Browse's own rescan can see it.
    std::fs::create_dir(p.media.join("SD")).unwrap();
    let rows = p.core.browse_rows().await;
    assert!(row(&rows, "volumes", "volume", "SD").is_some());
    std::fs::remove_dir(p.media.join("SD")).unwrap();
    let rows = p.core.browse_rows().await;
    assert!(row(&rows, "volumes", "volume", "SD").is_none());
}
