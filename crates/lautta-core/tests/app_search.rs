// SPDX-License-Identifier: LGPL-2.1-or-later

use lautta_core::app::Core;
use lautta_core::app_search::SearchRequest;
use lautta_core::locations::LocationRegistry;
use lautta_core::ops::OperationKind;
use lautta_core::paths::AppPaths;
use lautta_core::search::MatchMode;
use lautta_core::settings::{LocationPrefs, Settings};
use lautta_core::{ErrorKind, Uri};
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

#[tokio::test]
async fn clear_cache_empties_thumbnails_listings_and_archives_only() {
    let h = home().await;
    let paths = AppPaths::new(&h.root);
    write(&paths.thumbs_dir().join("k.thumb"), &[0u8; 100]);
    write(&paths.cache_dir().join("archives/x/file"), &[0u8; 50]);
    write(&paths.cache_dir().join("other/keep"), b"other");
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
        paths.cache_dir().join("other/keep").is_file(),
        "other cache files stay"
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
        .add(&uri("lautta://user-documents/"), "Docs", None, true)
        .unwrap();
    h.core.recent_searches.add("user-documents", "q").unwrap();
    write(&paths.data_dir().join("netvfs/bridge.sock"), b"s");
    write(&paths.thumbs_dir().join("k.thumb"), b"x");
    write(&h.root.join("Documents/mine.txt"), b"mine");
    h.core.clear_app_data().unwrap();
    assert!(h.core.favourites.list().unwrap().is_empty());
    assert!(h.core.recent_searches.list("user-documents").unwrap().is_empty());
    assert!(
        !paths.data_dir().join("netvfs").exists(),
        "netvfs data dir is cleared"
    );
    assert!(!paths.thumbs_dir().join("k.thumb").exists());
    assert!(h.root.join("Documents/mine.txt").is_file(), "user files stay");
    h.core
        .favourites
        .add(&uri("lautta://user-documents/"), "Again", None, true)
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

mod scope {
    use super::*;
    use lautta_core::app_search::{GroupKind, ScopeEvent, SearchScope, PROGRESS_EVERY_FOLDERS};
    use lautta_core::entry::Capabilities;
    use lautta_core::locations::{Location, LocationKind};
    use lautta_core::provider::memory::MemoryProvider;
    use lautta_core::search::SearchSummary;
    use lautta_core::Error;
    use std::collections::BTreeMap;

    fn nas(h: &Home, id: &str, name: &str) -> MemoryProvider {
        let m = MemoryProvider::new(Capabilities::with(&[]));
        let loc = Location::remote(
            id,
            LocationKind::Server {
                provider: "sftp".into(),
            },
            name,
            Some(format!("sftp://{id}")),
        );
        h.core.locations.register(loc, Arc::new(m.clone()));
        m
    }

    #[derive(Default, Debug)]
    struct Seen {
        hits: BTreeMap<usize, Vec<(usize, String)>>,
        progress: BTreeMap<usize, Vec<u64>>,
        done: BTreeMap<usize, Result<SearchSummary, ErrorKind>>,
        after_done: bool,
    }

    async fn run(
        h: &Home,
        scope: SearchScope,
        folder: Option<&Uri>,
        text: &str,
    ) -> (Vec<lautta_core::app_search::SearchGroup>, Seen) {
        let groups = h.core.search_groups(scope, folder);
        let req = SearchRequest {
            text: text.into(),
            mode: MatchMode::for_text(text),
            ..Default::default()
        };
        let (tx, mut rx) = mpsc::channel(4);
        let core = h.core.clone();
        let (g2, key) = (groups.clone(), scope.recents_key(folder));
        let task = tokio::spawn(async move {
            core.search_scope(&g2, &req, &key, Arc::new(AtomicBool::new(false)), tx)
                .await
        });
        let mut seen = Seen::default();
        while let Some(event) = rx.recv().await {
            match event {
                ScopeEvent::Hits { group, root, hits } => {
                    seen.after_done |= seen.done.contains_key(&group);
                    seen.hits
                        .entry(group)
                        .or_default()
                        .extend(hits.iter().map(|x| (root, x.uri.to_string())));
                }
                ScopeEvent::Progress { group, folders } => {
                    seen.after_done |= seen.done.contains_key(&group);
                    seen.progress.entry(group).or_default().push(folders);
                }
                ScopeEvent::Done { group, result } => {
                    assert!(
                        seen.done.insert(group, result.map_err(|e| e.kind)).is_none(),
                        "one Done per group"
                    );
                }
            }
        }
        task.await.unwrap();
        for v in seen.hits.values_mut() {
            v.sort();
        }
        (groups, seen)
    }

    #[tokio::test]
    async fn device_scope_searches_every_user_folder_as_one_group() {
        let h = home().await;
        write(&h.root.join("Documents/Finance/budget_2026.ods"), b"x");
        write(&h.root.join("Downloads/Household budget.pdf"), b"x");
        write(&h.root.join("Pictures/holiday.jpg"), b"x");
        nas(&h, "nv-nas", "NAS").add_file("budget.txt", b"x", 0);
        let (groups, seen) = run(&h, SearchScope::Device, None, "budget").await;
        let kinds: Vec<GroupKind> = groups.iter().map(|g| g.kind).collect();
        assert_eq!(kinds, vec![GroupKind::Device], "servers stay out of This device");
        let roots: Vec<String> = groups[0].roots.iter().map(|r| r.uri.to_string()).collect();
        assert!(
            roots.contains(&"lautta://user-documents/".to_owned()),
            "{roots:?}"
        );
        assert!(
            roots.contains(&"lautta://user-downloads/".to_owned()),
            "{roots:?}"
        );
        let found: Vec<&str> = seen.hits[&0].iter().map(|(_, u)| u.as_str()).collect();
        assert_eq!(
            found,
            vec![
                "lautta://user-documents/Finance/budget_2026.ods",
                "lautta://user-downloads/Household%20budget.pdf"
            ]
        );
        for (root, uri) in &seen.hits[&0] {
            assert!(
                uri.starts_with(&groups[0].roots[*root].uri.to_string()),
                "hit {uri} names its root"
            );
        }
        let done = seen.done[&0].as_ref().unwrap();
        assert_eq!(done.hits, 2);
        assert!(done.folders >= 4, "every folder of every root walked: {done:?}");
        assert!(!seen.after_done, "nothing comes after a group's Done");
        assert_eq!(h.core.recent_searches.list("@device").unwrap(), vec!["budget"]);
    }

    #[tokio::test]
    async fn everywhere_adds_servers_and_one_failing_server_spoils_nothing_else() {
        let h = home().await;
        write(&h.root.join("Documents/budget.ods"), b"x");
        let good = nas(&h, "nv-nas", "NAS");
        good.add_file("shared/Plans/Budget 2027 draft.odt", b"x", 0);
        good.add_file("shared/other.txt", b"x", 0);
        let bad = nas(&h, "nv-office", "Office");
        bad.fail_next("list", "", Error::kind(ErrorKind::ConnectionLost));
        let (groups, seen) = run(&h, SearchScope::Everywhere, None, "budget").await;
        let names: Vec<(GroupKind, &str)> = groups.iter().map(|g| (g.kind, g.name.as_str())).collect();
        assert_eq!(
            names,
            vec![
                (GroupKind::Device, ""),
                (GroupKind::Server, "NAS"),
                (GroupKind::Server, "Office")
            ]
        );
        assert_eq!(
            seen.hits[&1],
            vec![(
                0,
                "lautta://nv-nas/shared/Plans/Budget%202027%20draft.odt".to_owned()
            )]
        );
        assert_eq!(seen.done[&1].as_ref().unwrap().hits, 1);
        assert_eq!(seen.done[&2].as_ref().unwrap_err(), &ErrorKind::ConnectionLost);
        assert_eq!(seen.done[&0].as_ref().unwrap().hits, 1);
        assert!(!seen.hits.contains_key(&2));
        assert_eq!(
            h.core.recent_searches.list("@everywhere").unwrap(),
            vec!["budget"]
        );
        assert!(h.core.recent_searches.list("@device").unwrap().is_empty());
    }

    #[tokio::test]
    async fn server_search_keeps_the_depth_setting() {
        let h = home().await;
        h.core.apply_settings(Settings {
            search_remote_depth: 1,
            ..h.core.settings()
        });
        let m = nas(&h, "nv-nas", "NAS");
        m.add_file("a/budget-1.txt", b"x", 0);
        m.add_file("a/b/budget-2.txt", b"x", 0);
        let (_, seen) = run(&h, SearchScope::Everywhere, None, "budget").await;
        assert_eq!(
            seen.hits[&1],
            vec![(0, "lautta://nv-nas/a/budget-1.txt".to_owned())]
        );
    }

    #[tokio::test]
    async fn folder_scope_is_one_group_named_after_the_folder() {
        let h = home().await;
        write(&h.root.join("Documents/Uni/Thesis/thesis.pdf"), b"x");
        write(&h.root.join("Downloads/thesis.pdf"), b"x");
        let folder = uri("lautta://user-documents/Uni");
        let (groups, seen) = run(&h, SearchScope::Folder, Some(&folder), "thesis").await;
        assert_eq!(groups.len(), 1);
        assert_eq!(
            (groups[0].kind, groups[0].name.as_str()),
            (GroupKind::Folder, "Uni")
        );
        assert_eq!(
            seen.hits[&0],
            vec![
                (0, "lautta://user-documents/Uni/Thesis".to_owned()),
                (0, "lautta://user-documents/Uni/Thesis/thesis.pdf".to_owned())
            ]
        );
        assert_eq!(
            h.core.recent_searches.list("user-documents").unwrap(),
            vec!["thesis"]
        );
    }

    #[tokio::test]
    async fn progress_counts_folders_in_steps_across_roots() {
        let h = home().await;
        let per_root = PROGRESS_EVERY_FOLDERS as usize + 5;
        for top in ["Documents", "Downloads"] {
            for i in 0..per_root {
                std::fs::create_dir_all(h.root.join(top).join(format!("d{i}"))).unwrap();
            }
        }
        let (_, seen) = run(&h, SearchScope::Device, None, "nothing-matches").await;
        let steps = &seen.progress[&0];
        let total = seen.done[&0].as_ref().unwrap().folders;
        assert!(total >= 2 * (per_root as u64 + 1), "folders walked: {total}");
        assert!(
            steps.len() >= 2,
            "a step per {PROGRESS_EVERY_FOLDERS} folders in each root: {steps:?}"
        );
        assert!(
            steps.windows(2).all(|w| w[0] < w[1]),
            "counts only grow: {steps:?}"
        );
        assert!(*steps.last().unwrap() <= total);
        assert!(
            steps.iter().any(|s| *s > per_root as u64),
            "the second root counts on from the first: {steps:?}"
        );
    }

    #[tokio::test]
    async fn a_pattern_in_the_text_matches_as_a_pattern() {
        let h = home().await;
        write(&h.root.join("Documents/a.pdf"), b"x");
        write(&h.root.join("Documents/a.pdf.txt"), b"x");
        let (_, seen) = run(&h, SearchScope::Device, None, "*.pdf").await;
        let found: Vec<&str> = seen.hits[&0].iter().map(|(_, u)| u.as_str()).collect();
        assert_eq!(found, vec!["lautta://user-documents/a.pdf"]);
    }
}
