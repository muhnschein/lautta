// SPDX-License-Identifier: LGPL-2.1-or-later

use lautta_core::app::Core;
use lautta_core::app_search::{ScopeEvent, SearchRequest, SearchScope};
use lautta_core::entry::Capabilities;
use lautta_core::index::sched::Mode;
use lautta_core::index::store::RootState;
use lautta_core::locations::{Location, LocationKind, LocationRegistry};
use lautta_core::paths::AppPaths;
use lautta_core::provider::memory::MemoryProvider;
use lautta_core::search::MatchMode;
use lautta_core::settings::LocationPrefs;
use lautta_core::vpath::VPath;
use std::collections::BTreeMap;
use std::path::Path;
use std::sync::atomic::AtomicBool;
use std::sync::Arc;
use std::time::Duration;
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

fn write(path: &Path) {
    std::fs::create_dir_all(path.parent().unwrap()).unwrap();
    std::fs::write(path, b"x").unwrap();
}

async fn index_everything(h: &Home) {
    let index = h.core.index.as_ref().unwrap();
    for t in h.core.index_targets() {
        let root = index.store.root(&t.location, &VPath::root()).unwrap();
        h.core
            .index_refresh_root(&root, &AtomicBool::new(false))
            .await
            .unwrap();
    }
}

#[derive(Default, Debug)]
struct Seen {
    batches: Vec<(usize, Vec<String>)>,
    gone: Vec<String>,
    hits: BTreeMap<usize, u64>,
}

async fn search(
    h: &Home,
    scope: SearchScope,
    folder: Option<&lautta_core::Uri>,
    text: &str,
    hidden: bool,
) -> Seen {
    let groups = h.core.search_groups(&scope, folder);
    let req = SearchRequest {
        text: text.into(),
        mode: MatchMode::for_text(text),
        include_hidden: hidden,
        ..Default::default()
    };
    let (tx, mut rx) = mpsc::channel(8);
    let core = h.core.clone();
    let task = tokio::spawn(async move {
        core.search_scope(&groups, &req, "", Arc::new(AtomicBool::new(false)), tx)
            .await
    });
    let mut seen = Seen::default();
    while let Some(e) = rx.recv().await {
        match e {
            ScopeEvent::Hits { group, hits, .. } => {
                let mut names: Vec<String> = hits.iter().map(|x| x.uri.to_string()).collect();
                names.sort();
                seen.batches.push((group, names));
            }
            ScopeEvent::Gone { uris, .. } => seen.gone.extend(uris.iter().map(ToString::to_string)),
            ScopeEvent::Done { group, result } => {
                seen.hits.insert(group, result.unwrap().hits);
            }
            _ => {}
        }
    }
    task.await.unwrap();
    seen
}

#[tokio::test]
async fn device_folders_and_cards_are_indexed_by_default_servers_only_when_asked() {
    let h = home().await;
    let nas = Location::remote(
        "nv-nas",
        LocationKind::Server {
            provider: "sftp".into(),
        },
        "NAS",
        None,
    );
    h.core
        .locations
        .register(nas, Arc::new(MemoryProvider::new(Capabilities::with(&[]))));
    let targets: Vec<(String, bool)> = h
        .core
        .index_targets()
        .into_iter()
        .map(|t| (t.location, t.local))
        .collect();
    assert!(
        targets.contains(&("user-documents".to_owned(), true)),
        "{targets:?}"
    );
    assert!(
        !targets.iter().any(|(l, _)| l == "nv-nas"),
        "servers wait for their opt-in"
    );
    h.core
        .location_prefs
        .set(&LocationPrefs {
            index_names: true,
            ..LocationPrefs::new("nv-nas")
        })
        .unwrap();
    assert!(h
        .core
        .index_targets()
        .iter()
        .any(|t| t.location == "nv-nas" && !t.local));
}

#[tokio::test]
async fn a_search_answers_from_the_index_then_corrects_what_changed() {
    let h = home().await;
    write(&h.root.join("Documents/Finance/budget-2026.ods"));
    write(&h.root.join("Documents/old-budget.txt"));
    index_everything(&h).await;
    std::fs::remove_file(h.root.join("Documents/old-budget.txt")).unwrap();
    write(&h.root.join("Downloads/budget-new.pdf"));
    let seen = search(&h, SearchScope::Device, None, "budget", false).await;
    let first = &seen.batches[0].1;
    assert!(
        first.contains(&"lautta://user-documents/old-budget.txt".to_owned()),
        "the first answer comes from the index, made before the delete: {first:?}"
    );
    assert!(
        !first.iter().any(|u| u.contains("budget-new")),
        "the index knew nothing of the new file yet"
    );
    assert_eq!(
        seen.gone,
        vec!["lautta://user-documents/old-budget.txt"],
        "the check drops the deleted file"
    );
    assert!(
        seen.batches
            .iter()
            .skip(1)
            .any(|(_, b)| b.contains(&"lautta://user-downloads/budget-new.pdf".to_owned())),
        "and adds the new one: {:?}",
        seen.batches
    );
    assert_eq!(seen.hits[&0], 2);
}

#[tokio::test]
async fn index_answers_respect_hidden_folders_the_folder_scope_and_accents() {
    let h = home().await;
    write(&h.root.join("Documents/.private/secret-notes.txt"));
    write(&h.root.join("Documents/Uni/Café notes.md"));
    write(&h.root.join("Documents/notes.txt"));
    index_everything(&h).await;
    let all = search(&h, SearchScope::Device, None, "notes", false).await;
    let names: Vec<&String> = all.batches.iter().flat_map(|(_, b)| b).collect();
    assert!(
        !names.iter().any(|u| u.contains("secret")),
        "hidden folders stay out: {names:?}"
    );
    let hidden = search(&h, SearchScope::Device, None, "notes", true).await;
    assert!(hidden
        .batches
        .iter()
        .flat_map(|(_, b)| b)
        .any(|u| u.contains("secret")));
    let uni = lautta_core::Uri::parse("lautta://user-documents/Uni").unwrap();
    let folder = search(&h, SearchScope::Folder, Some(&uni), "notes", false).await;
    let names: Vec<&String> = folder.batches.iter().flat_map(|(_, b)| b).collect();
    assert_eq!(
        names,
        vec!["lautta://user-documents/Uni/Caf%C3%A9%20notes.md"],
        "only below the folder"
    );
    let accent = search(&h, SearchScope::Folder, Some(&uni), "cafe", false).await;
    assert_eq!(accent.hits[&0], 1, "accents don't matter");
}

#[tokio::test]
async fn an_unindexed_location_is_walked_as_before() {
    let h = home().await;
    write(&h.root.join("Documents/budget.ods"));
    let seen = search(&h, SearchScope::Device, None, "budget", false).await;
    assert_eq!(seen.hits[&0], 1);
    assert!(seen.gone.is_empty());
    let status = h.core.index_status();
    assert!(
        status.iter().all(|s| s.state == RootState::Pending),
        "searching builds no index by itself"
    );
}

#[tokio::test]
async fn the_worker_builds_the_device_index_while_the_app_is_open_and_pauses_behind() {
    let h = home().await;
    write(&h.root.join("Documents/a/b/c/deep-note.txt"));
    let index = h.core.index.as_ref().unwrap();
    let mut changes = index.subscribe();
    h.core.index_set_conditions(|c| c.app_active = true);
    let ready = async {
        loop {
            let local: Vec<_> = h.core.index_status().into_iter().filter(|s| s.local).collect();
            if !local.is_empty() && local.iter().all(|s| s.state == RootState::Ready) {
                break;
            }
            changes.changed().await.unwrap();
        }
    };
    tokio::time::timeout(Duration::from_secs(20), ready)
        .await
        .expect("device index built");
    let docs = index.store.root("user-documents", &VPath::root()).unwrap();
    assert_eq!(h.core.index_query(&docs, "deep-note", false).len(), 1);
    h.core.index_set_conditions(|c| c.app_active = false);
    assert_eq!(index.running(), None, "no worker in the background");
    h.core.index_set_conditions(|c| {
        c.index_on_charger = true;
        c.charging = true;
        c.battery = Some(80);
    });
    assert_eq!(
        index.running(),
        Some(Mode::Idle),
        "on the charger it works in the background"
    );
}
