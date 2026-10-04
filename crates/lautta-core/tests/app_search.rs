// SPDX-License-Identifier: LGPL-2.1-or-later
//! The search area's core actions on a temporary home folder: streaming
//! search with recents and depth, compare + sync + re-compare, cache and
//! app-data clearing, per-location preferences.

use lautta_core::app::Core;
use lautta_core::app_search::SearchRequest;
use lautta_core::compare::{CompareOptions, ItemStatus, SyncMode};
use lautta_core::locations::LocationRegistry;
use lautta_core::ops::OperationKind;
use lautta_core::paths::AppPaths;
use lautta_core::search::MatchMode;
use lautta_core::settings::{LocationPrefs, Settings};
use lautta_core::transfer::TransferState;
use lautta_core::{ErrorKind, Uri, VPath};
use std::collections::BTreeSet;
use std::path::Path;
use std::sync::atomic::AtomicBool;
use std::sync::Arc;
use tokio::sync::mpsc;

struct Home {
    _dir: tempfile::TempDir,
    root: std::path::PathBuf,
    core: Arc<Core>,
}

async fn home() -> Home {
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path().to_path_buf();
    for d in ["Documents", "Downloads", "Pictures"] {
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

async fn search_all(h: &Home, root: &Uri, req: &SearchRequest) -> Vec<String> {
    let (tx, mut rx) = mpsc::channel(8);
    let core = h.core.clone();
    let (root2, req2) = (root.clone(), req.clone());
    let task = tokio::spawn(async move {
        core.search_tree(root2, &req2, Arc::new(AtomicBool::new(false)), tx)
            .await
    });
    let mut names = Vec::new();
    while let Some(batch) = rx.recv().await {
        names.extend(batch.iter().map(|h| h.uri.to_string()));
    }
    task.await.unwrap().unwrap();
    names.sort();
    names
}

#[tokio::test]
async fn search_streams_hits_and_remembers_the_query() {
    let h = home().await;
    write(&h.root.join("Documents/Uni/thesis_draft.odt"), b"d");
    write(&h.root.join("Documents/Uni/Thesis/final.pdf"), b"f");
    write(&h.root.join("Documents/Uni/Thesis/Thesis_notes.md"), b"n");
    write(&h.root.join("Documents/other.txt"), b"o");
    let docs = uri("lautta://user-documents/");
    let req = SearchRequest {
        text: "thesis".to_owned(),
        ..SearchRequest::default()
    };
    let hits = search_all(&h, &docs, &req).await;
    assert_eq!(
        hits,
        [
            "lautta://user-documents/Uni/Thesis",
            "lautta://user-documents/Uni/Thesis/Thesis_notes.md",
            "lautta://user-documents/Uni/thesis_draft.odt"
        ]
    );
    assert_eq!(h.core.recent_searches_of(&docs), ["thesis"]);
    assert!(
        h.core
            .recent_searches_of(&uri("lautta://user-downloads/"))
            .is_empty(),
        "recents are per location"
    );

    let glob = SearchRequest {
        text: "*.pdf".to_owned(),
        mode: MatchMode::Glob,
        ..SearchRequest::default()
    };
    assert_eq!(
        search_all(&h, &docs, &glob).await,
        ["lautta://user-documents/Uni/Thesis/final.pdf"]
    );
    assert_eq!(h.core.recent_searches_of(&docs), ["*.pdf", "thesis"]);

    let docs_only = SearchRequest {
        types: ["document".to_owned()].into_iter().collect(),
        ..SearchRequest::default()
    };
    assert_eq!(
        search_all(&h, &docs, &docs_only).await,
        [
            "lautta://user-documents/Uni/Thesis/final.pdf",
            "lautta://user-documents/Uni/thesis_draft.odt"
        ]
    );
    assert_eq!(
        h.core.recent_searches_of(&docs).len(),
        2,
        "a search without text adds no recent entry"
    );
}

#[tokio::test]
async fn search_depth_is_unlimited_locally_and_a_setting_remotely() {
    let h = home().await;
    h.core.apply_settings(Settings {
        search_remote_depth: 3,
        ..Settings::default()
    });
    assert_eq!(h.core.search_depth(&uri("lautta://user-documents/")), None);
    assert_eq!(h.core.search_depth(&uri("lautta://nv-nas/srv")), Some(3));
}

async fn finish(h: &Home, ids: Vec<lautta_core::transfer::TransferId>) {
    for id in ids {
        let done = h
            .core
            .engine
            .wait_for(id, |s| s.state.is_finished())
            .await
            .unwrap();
        assert_eq!(done.state, TransferState::Completed);
    }
}

#[tokio::test]
async fn compare_then_sync_makes_both_sides_the_same() {
    let h = home().await;
    write(&h.root.join("Pictures/a.jpg"), b"aaa");
    write(&h.root.join("Pictures/sub/b.jpg"), b"bbb");
    write(&h.root.join("Pictures/skip.tmp"), b"t");
    write(&h.root.join("Downloads/c.jpg"), b"ccc");
    let (l, r) = (uri("lautta://user-pictures/"), uri("lautta://user-downloads/"));
    let opts = CompareOptions {
        excludes: vec!["*.tmp".to_owned()],
        ..CompareOptions::default()
    };
    let cancel = AtomicBool::new(false);
    let result = h.core.compare_folders(&l, &r, &opts, &cancel).await.unwrap();
    let c = result.counts();
    assert_eq!((c.left_only, c.right_only, c.same), (3, 1, 0));
    let p = Core::sync_preview(&result, SyncMode::UpdateBoth, &BTreeSet::new());
    assert_eq!((p.copy_files, p.new_dirs, p.deletes), (3, 1, 0));
    let excluded: BTreeSet<VPath> = [VPath::parse(b"sub").unwrap()].into_iter().collect();
    let p = Core::sync_preview(&result, SyncMode::UpdateBoth, &excluded);
    assert_eq!(
        (p.copy_files, p.new_dirs),
        (2, 0),
        "an excluded folder skips its children"
    );

    let ids = h
        .core
        .run_sync(&result, SyncMode::UpdateBoth, &BTreeSet::new())
        .await
        .unwrap();
    assert_eq!(ids.len(), 2, "one plan per destination side");
    finish(&h, ids).await;
    let again = h.core.compare_folders(&l, &r, &opts, &cancel).await.unwrap();
    assert!(
        again.items.iter().all(|i| i.status == ItemStatus::Same),
        "{:?}",
        again.items
    );
    assert_eq!(again.items.len(), 4);
    assert!(h.root.join("Downloads/sub/b.jpg").is_file());
    assert!(h.root.join("Pictures/c.jpg").is_file());
    assert!(!h.root.join("Downloads/skip.tmp").exists(), "excluded stays out");
}

#[tokio::test]
async fn mirror_deletes_extras_on_the_target() {
    let h = home().await;
    h.core.apply_settings(Settings {
        recently_deleted: false,
        ..Settings::default()
    });
    write(&h.root.join("Pictures/keep"), b"k");
    write(&h.root.join("Downloads/extra"), b"e");
    let (l, r) = (uri("lautta://user-pictures/"), uri("lautta://user-downloads/"));
    let cancel = AtomicBool::new(false);
    let opts = CompareOptions::default();
    let result = h.core.compare_folders(&l, &r, &opts, &cancel).await.unwrap();
    let p = Core::sync_preview(&result, SyncMode::MirrorLeftToRight, &BTreeSet::new());
    assert_eq!((p.copy_files, p.deletes), (1, 1));
    let ids = h
        .core
        .run_sync(&result, SyncMode::MirrorLeftToRight, &BTreeSet::new())
        .await
        .unwrap();
    assert_eq!(ids.len(), 2, "a copy plan and a delete plan");
    finish(&h, ids).await;
    assert!(!h.root.join("Downloads/extra").exists());
    assert!(h.root.join("Downloads/keep").is_file());
}

#[tokio::test]
async fn clear_cache_empties_thumbnails_listings_and_archives_only() {
    let h = home().await;
    let paths = AppPaths::new(&h.root);
    write(&paths.thumbs_dir().join("k.thumb"), &[0u8; 100]);
    write(&paths.cache_dir().join("archives/x/file"), &[0u8; 50]);
    write(&paths.crash_dir().join("crash-1.txt"), b"report");
    write(&h.root.join("Documents/keep.txt"), b"mine");
    let docs = uri("lautta://user-documents/");
    h.core.list(&docs, |_| {}).await.unwrap();
    let sizes = h.core.cache_sizes();
    assert_eq!((sizes.thumbnails, sizes.archives), (100, 50));
    assert!(sizes.listings > 0);
    assert_eq!(sizes.total(), 150 + sizes.listings);

    let freed = h.core.clear_cache().unwrap();
    assert_eq!(freed, sizes.total());
    assert_eq!(h.core.cache_sizes().total(), 0);
    assert!(paths.thumbs_dir().is_dir(), "the folder stays");
    assert!(
        paths.crash_dir().join("crash-1.txt").is_file(),
        "crash reports stay"
    );
    assert!(h.root.join("Documents/keep.txt").is_file());
    assert!(h.core.dircache.get(&docs).unwrap().is_none());
}

#[tokio::test]
async fn clear_app_data_removes_everything_the_app_keeps() {
    let h = home().await;
    let paths = AppPaths::new(&h.root);
    h.core
        .favourites
        .add(&uri("lautta://user-documents/"), "Docs", None)
        .unwrap();
    h.core.recent_searches.add("user-documents", "q").unwrap();
    write(&paths.data_dir().join("netvfs/bridge.sock"), b"s");
    write(&paths.thumbs_dir().join("k.thumb"), b"x");
    write(&h.root.join("Documents/mine.txt"), b"mine");
    h.core.clear_app_data().unwrap();
    assert!(h.core.favourites.list().unwrap().is_empty());
    assert!(h.core.recent_searches.list("user-documents").unwrap().is_empty());
    assert!(!paths.data_dir().join("netvfs").exists(), "DAT-3");
    assert!(!paths.thumbs_dir().join("k.thumb").exists());
    assert!(h.root.join("Documents/mine.txt").is_file(), "user files stay");
    h.core
        .favourites
        .add(&uri("lautta://user-documents/"), "Again", None)
        .unwrap();
}

#[tokio::test]
async fn clear_app_data_refuses_while_transfers_are_unfinished() {
    let h = home().await;
    let big = h.root.join("Downloads/big");
    std::fs::File::create(&big).unwrap().set_len(4 << 30).unwrap();
    let plan = h
        .core
        .plan(
            OperationKind::Copy,
            vec![uri("lautta://user-downloads/big")],
            uri("lautta://user-documents/"),
        )
        .await
        .unwrap();
    let id = h.core.start_plan(plan).await.unwrap();
    let err = h.core.clear_app_data().unwrap_err();
    assert_eq!(err.kind, ErrorKind::Locked);
    h.core.engine.cancel(id).unwrap();
    h.core
        .engine
        .wait_for(id, |s| s.state.is_finished())
        .await
        .unwrap();
    h.core.clear_app_data().unwrap();
}

#[tokio::test]
async fn location_prefs_apply_the_listing_opt_out() {
    let h = home().await;
    let docs = uri("lautta://user-documents/");
    write(&h.root.join("Documents/a"), b"a");
    let mut prefs = LocationPrefs::new("user-documents");
    prefs.no_listing_cache = true;
    prefs.display_name = Some("Papers".to_owned());
    h.core.set_location_prefs(&prefs).unwrap();
    assert!(h.core.dircache.is_disabled("user-documents"));
    assert_eq!(
        h.core
            .location_prefs
            .get("user-documents")
            .unwrap()
            .display_name
            .as_deref(),
        Some("Papers")
    );
    h.core.list(&docs, |_| {}).await.unwrap();
    assert!(h.core.dircache.get(&docs).unwrap().is_none(), "opted out");

    h.core
        .set_location_prefs(&LocationPrefs::new("user-documents"))
        .unwrap();
    assert!(!h.core.dircache.is_disabled("user-documents"));
    assert!(
        h.core.location_prefs.list().unwrap().is_empty(),
        "all-default preferences leave no row"
    );
}
