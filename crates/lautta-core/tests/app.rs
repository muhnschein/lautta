// SPDX-License-Identifier: LGPL-2.1-or-later
//! The app context end to end on a temporary home folder: user folders,
//! rename + undo, Recently deleted + undo, copy/move through the engine,
//! listings with the cache, archives as locations.

use lautta_core::app::{Core, Started};
use lautta_core::locations::LocationRegistry;
use lautta_core::ops::OperationKind;
use lautta_core::paths::AppPaths;
use lautta_core::settings::Settings;
use lautta_core::transfer::TransferState;
use lautta_core::{ErrorKind, Uri};
use std::path::Path;
use std::sync::Arc;

struct Home {
    _dir: tempfile::TempDir,
    root: std::path::PathBuf,
    core: Arc<Core>,
}

async fn home() -> Home {
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path().to_path_buf();
    for d in ["Documents", "Downloads"] {
        std::fs::create_dir_all(root.join(d)).unwrap();
    }
    let paths = AppPaths::new(&root);
    let registry = LocationRegistry::with_media_root(paths.clone(), root.join("media"));
    let core = Core::open_with(paths, registry).await.unwrap();
    Home {
        _dir: dir,
        root,
        core,
    }
}

fn uri(s: &str) -> Uri {
    Uri::parse(s).unwrap()
}

fn write(path: &Path, data: &[u8]) {
    std::fs::create_dir_all(path.parent().unwrap()).unwrap();
    std::fs::write(path, data).unwrap();
}

#[tokio::test]
async fn user_folders_are_locations() {
    let h = home().await;
    assert!(h.core.location("user-documents").is_some());
    assert!(h.core.location("user-downloads").is_some());
    assert!(
        h.core.location("user-music").is_none(),
        "missing folders are not shown (LOC-1)"
    );
}

#[tokio::test]
async fn make_rename_and_undo() {
    let h = home().await;
    let docs = uri("lautta://user-documents/");
    let folder = h.core.make_folder(&docs, b"Plans").await.unwrap();
    assert!(h.root.join("Documents/Plans").is_dir());
    let err = h.core.make_folder(&docs, b"Plans").await.unwrap_err();
    assert_eq!(err.kind, ErrorKind::AlreadyExists);
    let file = h.core.make_file(&folder, b"a.txt").await.unwrap();
    assert!(h.root.join("Documents/Plans/a.txt").is_file());

    let renamed = h.core.rename(&file, b"b.txt").await.unwrap();
    assert_eq!(renamed, uri("lautta://user-documents/Plans/b.txt"));
    assert!(h.root.join("Documents/Plans/b.txt").is_file());
    assert!(h.core.can_undo());
    assert!(h.core.undo().await.unwrap());
    assert!(h.root.join("Documents/Plans/a.txt").is_file());
    assert!(!h.root.join("Documents/Plans/b.txt").exists());
    assert!(!h.core.undo().await.unwrap(), "an undo is used once");
}

#[tokio::test]
async fn rename_never_replaces() {
    let h = home().await;
    write(&h.root.join("Documents/a"), b"a");
    write(&h.root.join("Documents/b"), b"b");
    let err = h
        .core
        .rename(&uri("lautta://user-documents/a"), b"b")
        .await
        .unwrap_err();
    assert_eq!(err.kind, ErrorKind::AlreadyExists);
    assert_eq!(std::fs::read(h.root.join("Documents/b")).unwrap(), b"b");
}

#[tokio::test]
async fn favourites_follow_renames() {
    let h = home().await;
    std::fs::create_dir_all(h.root.join("Documents/x")).unwrap();
    let x = uri("lautta://user-documents/x");
    h.core.favourites.add(&x, "x", None).unwrap();
    let y = h.core.rename(&x, b"y").await.unwrap();
    let favourites = h.core.favourites.list().unwrap();
    assert_eq!(favourites.len(), 1);
    assert_eq!(favourites[0].uri, y);
}

#[tokio::test]
async fn delete_to_recently_deleted_and_undo() {
    let h = home().await;
    write(&h.root.join("Documents/old.txt"), b"old");
    let item = uri("lautta://user-documents/old.txt");
    let transfer = h.core.delete(&[item.clone()]).await.unwrap();
    assert!(transfer.is_none(), "trashed locally, no transfer");
    assert!(!h.root.join("Documents/old.txt").exists());
    assert_eq!(h.core.trash.list().unwrap().len(), 1);
    assert!(h.core.undo().await.unwrap());
    assert_eq!(std::fs::read(h.root.join("Documents/old.txt")).unwrap(), b"old");
    assert!(h.core.trash.list().unwrap().is_empty());
}

#[tokio::test]
async fn delete_permanently_when_recently_deleted_is_off() {
    let h = home().await;
    h.core.apply_settings(Settings {
        recently_deleted: false,
        ..Settings::default()
    });
    write(&h.root.join("Documents/gone/f"), b"f");
    let id = h
        .core
        .delete(&[uri("lautta://user-documents/gone")])
        .await
        .unwrap()
        .expect("a delete transfer");
    let done = h
        .core
        .engine
        .wait_for(id, |s| s.state.is_finished())
        .await
        .unwrap();
    assert_eq!(done.state, TransferState::Completed);
    assert!(!h.root.join("Documents/gone").exists());
    assert!(h.core.trash.list().unwrap().is_empty());
}

#[tokio::test]
async fn copy_between_locations() {
    let h = home().await;
    write(&h.root.join("Downloads/pics/1.jpg"), b"one");
    write(&h.root.join("Downloads/pics/2.jpg"), b"two");
    let started = h
        .core
        .copy_or_move(
            OperationKind::Copy,
            vec![uri("lautta://user-downloads/pics")],
            uri("lautta://user-documents/"),
        )
        .await
        .unwrap();
    let Started::Transfer(id) = started else {
        panic!("small plans start at once")
    };
    let done = h
        .core
        .engine
        .wait_for(id, |s| s.state.is_finished())
        .await
        .unwrap();
    assert_eq!(done.state, TransferState::Completed);
    assert_eq!(
        std::fs::read(h.root.join("Documents/pics/2.jpg")).unwrap(),
        b"two"
    );
    assert!(
        h.root.join("Downloads/pics/1.jpg").exists(),
        "copy keeps the source"
    );
}

#[tokio::test]
async fn large_plans_need_the_summary() {
    let h = home().await;
    h.core.apply_settings(Settings {
        large_op_items: 1,
        ..Settings::default()
    });
    write(&h.root.join("Downloads/a"), b"a");
    write(&h.root.join("Downloads/b"), b"b");
    let started = h
        .core
        .copy_or_move(
            OperationKind::Move,
            vec![uri("lautta://user-downloads/a"), uri("lautta://user-downloads/b")],
            uri("lautta://user-documents/"),
        )
        .await
        .unwrap();
    let Started::NeedsSummary(plan) = started else {
        panic!("over the threshold")
    };
    assert!(
        h.root.join("Downloads/a").exists(),
        "nothing happens before confirming"
    );
    let id = h.core.start_plan(*plan).await.unwrap();
    let done = h
        .core
        .engine
        .wait_for(id, |s| s.state.is_finished())
        .await
        .unwrap();
    assert_eq!(done.state, TransferState::Completed);
    assert!(h.root.join("Documents/a").exists() && h.root.join("Documents/b").exists());
    assert!(!h.root.join("Downloads/a").exists());
}

#[tokio::test]
async fn listing_uses_the_cache_first() {
    let h = home().await;
    write(&h.root.join("Documents/one"), b"1");
    let docs = uri("lautta://user-documents/");
    let mut first_cached = None;
    let fresh = h
        .core
        .list(&docs, |c| first_cached = Some(c.len()))
        .await
        .unwrap();
    assert_eq!(fresh.len(), 1);
    assert_eq!(first_cached, None, "nothing cached yet");
    write(&h.root.join("Documents/two"), b"2");
    let mut cached = None;
    let fresh = h.core.list(&docs, |c| cached = Some(c.len())).await.unwrap();
    assert_eq!(cached, Some(1), "the cached listing is shown first");
    assert_eq!(fresh.len(), 2);
    h.core.make_folder(&docs, b"three").await.unwrap();
    let mut after = None;
    h.core.list(&docs, |c| after = Some(c.len())).await.unwrap();
    assert_eq!(after, None, "changes made by the app drop the cached listing");
}

#[tokio::test]
async fn archives_open_as_locations() {
    let h = home().await;
    let path = h.root.join("Documents/a.zip");
    {
        let file = std::fs::File::create(&path).unwrap();
        let mut zip = zip::ZipWriter::new(file);
        zip.start_file("inner/readme.txt", zip::write::FileOptions::default())
            .unwrap();
        std::io::Write::write_all(&mut zip, b"hello").unwrap();
        zip.finish().unwrap();
    }
    let id = h
        .core
        .open_archive(&uri("lautta://user-documents/a.zip"))
        .await
        .unwrap();
    assert!(id.starts_with("arc-"));
    assert_eq!(
        h.core
            .open_archive(&uri("lautta://user-documents/a.zip"))
            .await
            .unwrap(),
        id
    );
    let inner = h
        .core
        .list(&Uri::parse(&format!("lautta://{id}/inner")).unwrap(), |_| {})
        .await
        .unwrap();
    assert_eq!(inner.len(), 1);
    assert_eq!(inner[0].name, b"readme.txt");
}
