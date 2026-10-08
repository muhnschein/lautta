// SPDX-License-Identifier: LGPL-2.1-or-later
//! The operations area on a temporary home: compress and extract through the
//! engine, remote compress through a pipe, info, permissions, links,
//! Recently deleted and the share probe.

use lautta_core::app::{Core, Started};
use lautta_core::app_operations::*;
use lautta_core::compress::ArchiveKind;
use lautta_core::entry::{cap, Capabilities};
use lautta_core::locations::{Location, LocationKind, LocationRegistry};
use lautta_core::ops::{ConflictChoice, OperationKind};
use lautta_core::paths::AppPaths;
use lautta_core::provider::local::LocalProvider;
use lautta_core::provider::memory::MemoryProvider;
use lautta_core::provider::no_progress;
use lautta_core::transfer::TransferState;
use lautta_core::{ErrorKind, Uri};
use std::path::Path;
use std::sync::atomic::AtomicBool;
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

async fn finish(h: &Home, started: Started) {
    let Started::Transfer(id) = started else {
        panic!("expected a transfer");
    };
    let s = h
        .core
        .engine
        .wait_for(id, |s| s.state.is_finished())
        .await
        .unwrap();
    assert_eq!(s.state, TransferState::Completed, "{s:?}");
}

#[tokio::test]
async fn compress_local_then_extract_round_trip() {
    let h = home().await;
    write(&h.root.join("Documents/src/a.txt"), b"alpha");
    write(&h.root.join("Documents/src/sub/b.txt"), b"beta");
    let sources = [uri("lautta://user-documents/src")];
    let dest = uri("lautta://user-downloads/");
    let m = h.core.measure(&sources).await.unwrap();
    assert_eq!((m.files, m.dirs, m.bytes), (2, 2, 9));

    let zip = h
        .core
        .compress_to(
            &sources,
            &dest,
            "bundle",
            ArchiveKind::Zip,
            no_progress(),
            Arc::new(AtomicBool::new(false)),
        )
        .await
        .unwrap();
    assert_eq!(zip, uri("lautta://user-downloads/bundle.zip"));
    assert!(h.root.join("Downloads/bundle.zip").is_file());
    let again = h
        .core
        .compress_to(
            &sources,
            &dest,
            "bundle",
            ArchiveKind::Zip,
            no_progress(),
            Arc::new(AtomicBool::new(false)),
        )
        .await
        .unwrap();
    assert_eq!(
        again,
        uri("lautta://user-downloads/bundle 2.zip"),
        "taken names are numbered"
    );
    let parts: Vec<_> = std::fs::read_dir(h.root.join("Downloads"))
        .unwrap()
        .filter_map(Result::ok)
        .filter(|e| e.file_name().to_string_lossy().ends_with(".lautta-part"))
        .collect();
    assert!(parts.is_empty(), "no partial files stay");

    let info = h.core.archive_info(&zip).await.unwrap();
    assert_eq!((info.files, info.dirs, info.bytes), (2, 2, 9));
    assert_eq!(info.folder_name, "bundle");

    let into = uri("lautta://user-documents/bundle");
    let started = h.core.extract(&zip, &into).await.unwrap();
    finish(&h, started).await;
    assert_eq!(
        std::fs::read(h.root.join("Documents/bundle/src/a.txt")).unwrap(),
        b"alpha"
    );
    assert_eq!(
        std::fs::read(h.root.join("Documents/bundle/src/sub/b.txt")).unwrap(),
        b"beta"
    );
}

#[tokio::test]
async fn compress_tar_gz_and_errors() {
    let h = home().await;
    write(&h.root.join("Documents/f.txt"), b"data");
    let dest = uri("lautta://user-downloads/");
    let src = [uri("lautta://user-documents/f.txt")];
    let out = h
        .core
        .compress_to(
            &src,
            &dest,
            "pack",
            ArchiveKind::TarGz,
            no_progress(),
            Arc::new(AtomicBool::new(false)),
        )
        .await
        .unwrap();
    assert_eq!(out.to_string(), "lautta://user-downloads/pack.tar.gz");

    let canceled = h
        .core
        .compress_to(
            &src,
            &dest,
            "stop",
            ArchiveKind::Zip,
            no_progress(),
            Arc::new(AtomicBool::new(true)),
        )
        .await
        .unwrap_err();
    assert_eq!(canceled.kind, ErrorKind::Canceled);
    assert!(!h.root.join("Downloads/stop.zip").exists());
    let leftovers = std::fs::read_dir(h.root.join("Downloads")).unwrap().count();
    assert_eq!(leftovers, 1, "a canceled run leaves nothing behind");

    let bad = h
        .core
        .compress_to(
            &src,
            &dest,
            "a/b",
            ArchiveKind::Zip,
            no_progress(),
            Arc::new(AtomicBool::new(false)),
        )
        .await
        .unwrap_err();
    assert_eq!(bad.kind, ErrorKind::InvalidName);
    let none = h
        .core
        .compress_to(
            &[],
            &dest,
            "x",
            ArchiveKind::Zip,
            no_progress(),
            Arc::new(AtomicBool::new(false)),
        )
        .await;
    assert!(none.is_err());
}

/// A server location backed by a folder (so uploads read real pipes).
fn register_remote(h: &Home) -> tempfile::TempDir {
    let dir = tempfile::tempdir().unwrap();
    let provider = LocalProvider::new(dir.path().to_path_buf());
    let location = Location::remote("nas", LocationKind::AdHoc, "NAS", Some("sftp://nas.home".into()));
    h.core.locations.register(location, Arc::new(provider));
    dir
}

#[tokio::test]
async fn compress_to_a_remote_destination_goes_through_a_pipe() {
    let h = home().await;
    write(&h.root.join("Documents/f.txt"), b"remote data");
    let mem = register_remote(&h);
    let target = h
        .core
        .compress_to(
            &[uri("lautta://user-documents/f.txt")],
            &uri("lautta://nas/"),
            "out",
            ArchiveKind::Zip,
            no_progress(),
            Arc::new(AtomicBool::new(false)),
        )
        .await
        .unwrap();
    assert_eq!(target, uri("lautta://nas/out.zip"));
    let bytes = std::fs::read(mem.path().join("out.zip")).unwrap();
    assert_eq!(&bytes[..2], b"PK");
}

#[tokio::test]
async fn failed_remote_compress_removes_the_partial_file() {
    let h = home().await;
    let mem = register_remote(&h);
    let res = h
        .core
        .compress_to(
            &[uri("lautta://user-documents/missing.txt")],
            &uri("lautta://nas/"),
            "out",
            ArchiveKind::Zip,
            no_progress(),
            Arc::new(AtomicBool::new(false)),
        )
        .await;
    assert!(res.is_err());
    assert!(!mem.path().join("out.zip").exists());
    assert_eq!(std::fs::read_dir(mem.path()).unwrap().count(), 0);
}

#[tokio::test]
async fn read_only_destinations_are_refused() {
    let h = home().await;
    write(&h.root.join("Documents/f.txt"), b"x");
    let mem = MemoryProvider::new(Capabilities::with(&[cap::READ_ONLY]));
    h.core.locations.register(
        Location::remote("ro", LocationKind::AdHoc, "RO", None),
        Arc::new(mem),
    );
    let err = h
        .core
        .compress_to(
            &[uri("lautta://user-documents/f.txt")],
            &uri("lautta://ro/"),
            "x",
            ArchiveKind::Zip,
            no_progress(),
            Arc::new(AtomicBool::new(false)),
        )
        .await
        .unwrap_err();
    assert_eq!(err.kind, ErrorKind::ReadOnlyFilesystem);
}

#[tokio::test]
async fn summary_conflicts_and_start_with_options() {
    let h = home().await;
    write(&h.root.join("Documents/dup.txt"), b"new");
    write(&h.root.join("Downloads/dup.txt"), b"old");
    write(&h.root.join("Documents/other.txt"), b"o");
    let sources = vec![
        uri("lautta://user-documents/dup.txt"),
        uri("lautta://user-documents/other.txt"),
    ];
    let dest = uri("lautta://user-downloads/");
    let mut plan = h
        .core
        .plan(OperationKind::Copy, sources, dest.clone())
        .await
        .unwrap();
    let summary = h.core.plan_summary(&plan).await;
    assert_eq!(
        (summary.kind.as_str(), summary.files, summary.conflicts),
        ("copy", 2, 1)
    );
    assert_eq!(summary.destination, dest.to_string());
    assert_eq!(summary.destination_name, "Downloads");
    assert!(summary.free_bytes.is_some());
    assert!(!summary.options.verify_checksums);
    assert!(summary.options.preserve_mtime);

    let list = h.core.unresolved_conflicts(&plan);
    assert_eq!(list.len(), 1);
    assert_eq!(list[0].name, "dup.txt");
    assert_ne!(list[0].default_choice, ConflictChoice::Replace);
    assert_eq!(list[0].folder, "Downloads");

    let bad = h
        .core
        .resolve_plan_conflict(&mut plan, list[0].index, ConflictChoice::Merge, false)
        .await;
    assert_eq!(
        bad.unwrap_err().kind,
        ErrorKind::InvalidArgument,
        "merge is for folders"
    );
    let n = h
        .core
        .resolve_plan_conflict(&mut plan, list[0].index, ConflictChoice::KeepBoth, false)
        .await
        .unwrap();
    assert_eq!(n, 1);
    assert!(h.core.unresolved_conflicts(&plan).is_empty());

    let options = StartOptions {
        verify_checksums: true,
        ..summary.options
    };
    let before = h.core.settings();
    let id = h.core.start_plan_with(plan, options).await.unwrap();
    assert_eq!(h.core.settings(), before, "settings are put back");
    let s = h
        .core
        .engine
        .wait_for(id, |s| s.state.is_finished())
        .await
        .unwrap();
    assert_eq!(s.state, TransferState::Completed);
    assert!(
        s.options.verify_checksums,
        "the sheet's option reached the transfer"
    );
    assert_eq!(std::fs::read(h.root.join("Downloads/dup 2.txt")).unwrap(), b"new");
    assert_eq!(std::fs::read(h.root.join("Downloads/dup.txt")).unwrap(), b"old");
}

#[tokio::test]
async fn info_and_links() {
    let h = home().await;
    write(&h.root.join("Documents/doc.txt"), b"abc");
    let u = uri("lautta://user-documents/doc.txt");

    let info = h.core.info(&u).await.unwrap();
    assert_eq!(info.name, "doc.txt");
    assert_eq!(info.size, Some(3));
    assert_eq!(info.category, "text");
    assert_eq!(info.folder_name, "Documents");
    assert!(info.mode_text.len() == 9 && info.mode.is_some());
    assert!(info.can_symlink);
    assert!(info.free_bytes.is_some());
    assert!(!info.is_dir);

    let dl = uri("lautta://user-downloads/");
    let hard = h.core.make_link(&u, &dl, true).await;
    // Different locations: hard links are refused rather than guessed.
    assert_eq!(hard.unwrap_err().kind, ErrorKind::CrossesDevice);
    let sym = h.core.make_link(&u, &dl, false).await.unwrap();
    assert_eq!(sym, uri("lautta://user-downloads/doc.txt"));
    assert!(std::fs::symlink_metadata(h.root.join("Downloads/doc.txt"))
        .unwrap()
        .is_symlink());
    let sym2 = h.core.make_link(&u, &dl, false).await.unwrap();
    assert_eq!(sym2, uri("lautta://user-downloads/doc 2.txt"));
    let same = uri("lautta://user-documents/");
    let hard = h.core.make_link(&u, &same, true).await.unwrap();
    assert_eq!(hard, uri("lautta://user-documents/doc 2.txt"));
    assert_eq!(std::fs::read(h.root.join("Documents/doc 2.txt")).unwrap(), b"abc");
}

#[tokio::test]
async fn set_modified_changes_the_time() {
    let h = home().await;
    write(&h.root.join("Documents/t.txt"), b"x");
    let u = uri("lautta://user-documents/t.txt");
    h.core.set_modified(&u, 1_000_000_000_000).await.unwrap();
    assert_eq!(h.core.info(&u).await.unwrap().modified, Some(1_000_000_000_000));
}

#[tokio::test]
async fn recently_deleted_entries_restore_and_empty() {
    let h = home().await;
    write(&h.root.join("Documents/Uni/old.txt"), b"1");
    write(&h.root.join("Documents/Uni/new.txt"), b"22");
    h.core
        .delete(&[uri("lautta://user-documents/Uni/old.txt")])
        .await
        .unwrap();
    h.core
        .delete(&[uri("lautta://user-documents/Uni/new.txt")])
        .await
        .unwrap();
    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_secs() as i64;
    let entries = h.core.trash_entries(now).unwrap();
    assert_eq!(entries.len(), 2);
    assert_eq!(entries[0].folder_name, "Documents › Uni");
    assert_eq!(entries[0].days_left, 30);
    assert_eq!(entries[0].folder_uri, "lautta://user-documents/Uni");
    let old = entries.iter().find(|e| e.name == "old.txt").unwrap();
    assert_eq!(old.size, Some(1));
    let far = h.core.trash_entries(now + 31 * 86_400).unwrap();
    assert!(far.iter().all(|e| e.days_left == 0));

    h.core.restore(old.id).await.unwrap();
    assert!(h.root.join("Documents/Uni/old.txt").is_file());
    assert_eq!(h.core.trash_entries(now).unwrap().len(), 1);
    assert_eq!(h.core.trash.empty().unwrap(), 1);
    assert!(h.core.trash_entries(now).unwrap().is_empty());
    assert_eq!(h.core.trash.total_size().unwrap(), 0);
}

#[tokio::test]
async fn share_probe_and_destinations() {
    let h = home().await;
    write(&h.root.join("Downloads/shared.jpg"), b"123456");
    let outside = tempfile::tempdir().unwrap();
    write(&outside.path().join("elsewhere.bin"), b"zz");
    let probed = h.core.probe_shared(&[
        h.root.join("Downloads/shared.jpg"),
        outside.path().join("elsewhere.bin"),
        h.root.join("Downloads/nope.jpg"),
    ]);
    assert!(probed[0].readable);
    assert_eq!((probed[0].size, probed[0].name.as_str()), (6, "shared.jpg"));
    assert_eq!(probed[0].uri, "lautta://user-downloads/shared.jpg");
    assert!(
        !probed[1].readable,
        "files outside every location cannot be received"
    );
    assert!(!probed[2].readable);

    let _remote = register_remote(&h);
    let dests = h.core.destinations();
    let names: Vec<&str> = dests.iter().map(|d| d.name.as_str()).collect();
    assert!(names.contains(&"Documents") && names.contains(&"NAS"));
    assert_eq!(dests.iter().find(|d| d.name == "NAS").unwrap().kind, "server");
    assert_eq!(
        dests.iter().find(|d| d.name == "Documents").unwrap().kind,
        "device"
    );
}

#[tokio::test]
async fn empty_archives_are_refused_and_display_paths() {
    let h = home().await;
    assert_eq!(
        h.core.display_path(&uri("lautta://user-documents/Uni/x")),
        "Documents › Uni › x"
    );
    assert_eq!(
        h.core.display_folder(&uri("lautta://user-documents/Uni/x")),
        "Documents › Uni"
    );
    assert_eq!(h.core.display_path(&uri("lautta://nope/a")), "nope › a");
    write(&h.root.join("Documents/not-an-archive.zip"), b"junk");
    let err = h
        .core
        .extract(
            &uri("lautta://user-documents/not-an-archive.zip"),
            &uri("lautta://user-documents/out"),
        )
        .await;
    assert!(err.is_err());
}
