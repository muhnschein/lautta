// SPDX-License-Identifier: LGPL-2.1-or-later
//! Sailjail layout: every permitted folder and app data folder is its own bind mount, so
//! `rename(2)` between them fails with `EXDEV`. Without root, two file systems (temp folder and
//! `/dev/shm`) joined by symlinks stand in; hosts without a separate `/dev/shm` skip those tests.

use lautta_core::app::Core;
use lautta_core::app_browse::Row;
use lautta_core::locations::LocationRegistry;
use lautta_core::paths::AppPaths;
use lautta_core::trash::now_secs;
use lautta_core::Uri;
use std::os::unix::fs::{symlink, MetadataExt, PermissionsExt};
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::{Duration, SystemTime};

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

fn row<'a>(rows: &'a [Row], section: &str, kind: &str, name: &str) -> Option<&'a Row> {
    rows.iter()
        .find(|r| r.section == section && r.kind == kind && r.name == name)
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

fn project(dir: &Path) -> SystemTime {
    write(&dir.join("thesis.odt"), b"draft");
    std::fs::set_permissions(dir.join("thesis.odt"), std::fs::Permissions::from_mode(0o640)).unwrap();
    symlink("thesis.odt", dir.join("latest")).unwrap();
    let when = SystemTime::UNIX_EPOCH + Duration::from_secs(1_500_000_000);
    let f = std::fs::File::options()
        .write(true)
        .open(dir.join("thesis.odt"))
        .unwrap();
    f.set_modified(when).unwrap();
    when
}

fn assert_project(dir: &Path, when: SystemTime) {
    let meta = std::fs::metadata(dir.join("thesis.odt")).unwrap();
    assert_eq!(std::fs::read(dir.join("thesis.odt")).unwrap(), b"draft");
    assert_eq!(meta.permissions().mode() & 0o777, 0o640);
    assert_eq!(meta.modified().unwrap(), when);
    assert_eq!(
        std::fs::read_link(dir.join("latest")).unwrap(),
        Path::new("thesis.odt")
    );
}

#[tokio::test]
async fn deletes_go_to_the_app_data_trash_across_mounts() {
    let Some(s) = sandbox().await else { return };
    let when = project(&s.home.join("Documents/Uni"));
    let docs = uri("lautta://user-documents/");
    let item = uri("lautta://user-documents/Uni");
    assert!(s.core.deletes_to_trash(&item));
    let transfer = s.core.delete(std::slice::from_ref(&item)).await.unwrap();
    assert!(transfer.is_none(), "kept in the Trash, not deleted for good");
    assert!(!s.home.join("Documents/Uni").exists());
    let items = s.core.trash.list().unwrap();
    assert_eq!(items.len(), 1);
    assert_eq!(items[0].dir, AppPaths::new(&s.home).trash_dir());
    assert_project(&items[0].stored_path(), when);

    assert!(s.core.list(&docs, |_| {}).await.unwrap().is_empty());
    assert_eq!(std::fs::read_dir(s.home.join("Documents")).unwrap().count(), 0);

    s.core.restore(items[0].id).await.unwrap();
    assert_project(&s.home.join("Documents/Uni"), when);
    assert!(s.core.trash.list().unwrap().is_empty());
    assert_eq!(
        std::fs::read_dir(AppPaths::new(&s.home).trash_dir())
            .unwrap()
            .count(),
        0
    );
}

#[tokio::test]
async fn deletes_are_kept_when_the_app_data_folder_is_a_mount_of_its_own() {
    let Some(shm) = other_fs() else { return };
    let disk = tempfile::tempdir().unwrap();
    let home = shm.path().join("home");
    std::fs::create_dir_all(home.join("Documents")).unwrap();
    symlink(disk.path(), home.join(".local")).unwrap();
    let core = open(&home).await;
    write(&home.join("Documents/a.txt"), b"a");
    let transfer = core
        .delete(&[uri("lautta://user-documents/a.txt")])
        .await
        .unwrap();
    assert!(transfer.is_none(), "kept in the Trash, not deleted for good");
    let items = core.trash.list().unwrap();
    assert_eq!(items.len(), 1);
    core.restore(items[0].id).await.unwrap();
    assert_eq!(std::fs::read(home.join("Documents/a.txt")).unwrap(), b"a");
}

#[tokio::test]
async fn restore_goes_back_across_mounts_into_a_folder_that_is_gone() {
    let Some(s) = sandbox().await else { return };
    write(&s.home.join("Documents/Uni/2024/notes.txt"), b"n");
    s.core
        .delete(&[uri("lautta://user-documents/Uni/2024/notes.txt")])
        .await
        .unwrap();
    std::fs::remove_dir_all(s.home.join("Documents/Uni")).unwrap();
    let id = s.core.trash.list().unwrap()[0].id;
    let back = s.core.restore(id).await.unwrap();
    assert_eq!(back, uri("lautta://user-documents/Uni/2024/notes.txt"));
    assert_eq!(
        std::fs::read(s.home.join("Documents/Uni/2024/notes.txt")).unwrap(),
        b"n"
    );
}

#[tokio::test]
async fn restore_purge_and_empty_cover_every_item() {
    let Some(s) = sandbox().await else { return };
    write(&s.home.join("Documents/a.txt"), b"a");
    write(&s.home.join("Downloads/b.txt"), b"bb");
    for item in ["lautta://user-documents/a.txt", "lautta://user-downloads/b.txt"] {
        assert!(s.core.delete(&[uri(item)]).await.unwrap().is_none(), "{item}");
    }
    let trash_dir = AppPaths::new(&s.home).trash_dir();
    write(&trash_dir.join("old-1"), b"ccc");
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

    let c = s.core.trash.list().unwrap();
    let c = c.iter().find(|i| i.stored_name == "old-1").unwrap();
    s.core.restore(c.id).await.unwrap();
    assert_eq!(std::fs::read(s.home.join("Documents/c.txt")).unwrap(), b"ccc");
    assert!(!trash_dir.join("old-1").exists());

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
    assert_eq!(std::fs::read_dir(&trash_dir).unwrap().count(), 0);
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
async fn clear_app_data_removes_the_trash_and_nothing_else_in_the_folders() {
    let Some(s) = sandbox().await else { return };
    write(&s.home.join("Documents/a"), b"a");
    write(&s.home.join("Downloads/keep"), b"k");
    s.core.delete(&[uri("lautta://user-documents/a")]).await.unwrap();
    assert_eq!(s.core.trash.list().unwrap().len(), 1);
    s.core.clear_app_data().unwrap();
    assert!(s.home.join("Downloads/keep").is_file(), "user files stay");
    assert!(s.core.trash.list().unwrap().is_empty());
    assert_eq!(
        std::fs::read_dir(AppPaths::new(&s.home).trash_dir())
            .unwrap()
            .count(),
        0
    );
}

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
    std::fs::create_dir(p.media.join("SD")).unwrap();
    let rows = p.core.browse_rows().await;
    assert!(row(&rows, "volumes", "volume", "SD").is_some());
    std::fs::remove_dir(p.media.join("SD")).unwrap();
    let rows = p.core.browse_rows().await;
    assert!(row(&rows, "volumes", "volume", "SD").is_none());
}
