// SPDX-License-Identifier: LGPL-2.1-or-later
//! The directory area's core actions on a temporary home folder: streaming
//! listings with the cache, path steps, address editing and completion.

use lautta_core::app::Core;
use lautta_core::app_directory::{ListEvent, PathStep};
use lautta_core::entry::cap;
use lautta_core::locations::LocationRegistry;
use lautta_core::paths::AppPaths;
use lautta_core::{ErrorKind, Uri};
use tokio::sync::mpsc::unbounded_channel;

struct Home {
    _dir: tempfile::TempDir,
    root: std::path::PathBuf,
    core: std::sync::Arc<Core>,
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

async fn events(core: &Core, dir: &Uri, use_cache: bool) -> Vec<ListEvent> {
    let (tx, mut rx) = unbounded_channel();
    core.list_stream(dir, use_cache, tx).await;
    let mut out = Vec::new();
    while let Some(e) = rx.recv().await {
        out.push(e);
    }
    out
}

fn names(entries: &[lautta_core::entry::Entry]) -> Vec<String> {
    let mut v: Vec<String> = entries.iter().map(|e| e.display_name()).collect();
    v.sort();
    v
}

#[tokio::test]
async fn streams_batches_then_done_and_fills_the_cache() {
    let h = home().await;
    std::fs::create_dir_all(h.root.join("Documents/Uni")).unwrap();
    std::fs::write(h.root.join("Documents/a.txt"), b"a").unwrap();
    let dir = uri("lautta://user-documents/");
    let ev = events(&h.core, &dir, true).await;
    assert!(matches!(ev.last(), Some(ListEvent::Done(Ok(())))));
    assert!(
        !ev.iter().any(|e| matches!(e, ListEvent::Cached { .. })),
        "nothing cached yet"
    );
    let fresh: Vec<_> = ev
        .iter()
        .flat_map(|e| match e {
            ListEvent::Batch(b) => b.clone(),
            _ => Vec::new(),
        })
        .collect();
    assert_eq!(names(&fresh), ["Uni", "a.txt"]);

    // The second listing shows the cache first, then the fresh batches.
    std::fs::write(h.root.join("Documents/b.txt"), b"b").unwrap();
    let ev = events(&h.core, &dir, true).await;
    match &ev[0] {
        ListEvent::Cached { entries, .. } => assert_eq!(names(entries), ["Uni", "a.txt"]),
        other => panic!("expected the cache first, got {other:?}"),
    }
    let ev = events(&h.core, &dir, false).await;
    assert!(
        !ev.iter().any(|e| matches!(e, ListEvent::Cached { .. })),
        "a refresh skips the cache"
    );
    let cached = h.core.dircache.get(&dir).unwrap().unwrap();
    assert_eq!(
        names(&cached.entries),
        ["Uni", "a.txt", "b.txt"],
        "refreshed by the listing"
    );
}

#[tokio::test]
async fn listing_errors_arrive_as_done() {
    let h = home().await;
    let ev = events(&h.core, &uri("lautta://user-documents/missing"), true).await;
    assert_eq!(ev.len(), 1);
    match &ev[0] {
        ListEvent::Done(Err(e)) => assert_eq!(e.kind, ErrorKind::NotFound),
        other => panic!("{other:?}"),
    }
    let ev = events(&h.core, &uri("lautta://no-such-location/"), true).await;
    assert!(matches!(&ev[0], ListEvent::Done(Err(_))));
}

#[tokio::test]
async fn path_steps_start_at_the_location() {
    let h = home().await;
    let steps = h.core.path_steps(&uri("lautta://user-documents/Uni/Thesis"));
    let shown: Vec<(&str, String)> = steps
        .iter()
        .map(|s| (s.name.as_str(), s.uri.to_string()))
        .collect();
    assert_eq!(
        shown,
        [
            ("Documents", "lautta://user-documents/".to_owned()),
            ("Uni", "lautta://user-documents/Uni".to_owned()),
            ("Thesis", "lautta://user-documents/Uni/Thesis".to_owned()),
        ]
    );
    let root: Vec<PathStep> = h.core.path_steps(&uri("lautta://user-documents/"));
    assert_eq!(root.len(), 1);
}

#[tokio::test]
async fn edit_address_is_home_relative_for_local_folders() {
    let h = home().await;
    assert_eq!(
        h.core.edit_address(&uri("lautta://user-documents/Uni")),
        "~/Documents/Uni"
    );
    assert_eq!(
        h.core.edit_address(&uri("lautta://user-documents/")),
        "~/Documents"
    );
}

#[tokio::test]
async fn typed_addresses_resolve() {
    let h = home().await;
    let base = uri("lautta://user-documents/");
    let uni = uri("lautta://user-documents/Uni");
    assert_eq!(
        h.core.resolve_address("~/Documents/Uni", &base),
        Some(uni.clone())
    );
    assert_eq!(
        h.core.resolve_address("  ~/Documents/Uni/ ", &base),
        Some(uni.clone())
    );
    let abs = format!("{}/Documents/Uni", h.root.display());
    assert_eq!(h.core.resolve_address(&abs, &base), Some(uni.clone()));
    assert_eq!(
        h.core.resolve_address(&format!("file://{abs}"), &base),
        Some(uni.clone())
    );
    assert_eq!(
        h.core.resolve_address("lautta://user-documents/Uni", &base),
        Some(uni)
    );
    assert_eq!(
        h.core.resolve_address("~", &base),
        None,
        "the home folder itself is no location"
    );
    assert_eq!(h.core.resolve_address("/nowhere/at/all", &base), None);
    assert_eq!(
        h.core.resolve_address("Uni", &base),
        None,
        "relative text is not an address"
    );
    assert_eq!(
        h.core.resolve_address("~/Documents/../Downloads", &base),
        Some(uri("lautta://user-downloads/"))
    );
}

#[tokio::test]
async fn remote_addresses_are_paths_inside_the_location() {
    use lautta_core::provider::memory::MemoryProvider;
    use std::sync::Arc;
    let h = home().await;
    let loc = lautta_core::locations::Location::remote(
        "nv-nas",
        lautta_core::locations::LocationKind::AdHoc,
        "NAS",
        Some("sftp://nas.home".to_owned()),
    );
    h.core.locations.register(
        loc,
        Arc::new(MemoryProvider::new(lautta_core::entry::Capabilities::with(&[
            cap::WRITE,
        ]))),
    );
    let base = uri("lautta://nv-nas/srv");
    assert_eq!(
        h.core.resolve_address("/srv/photos", &base),
        Some(uri("lautta://nv-nas/srv/photos"))
    );
    assert_eq!(
        h.core.edit_address(&uri("lautta://nv-nas/srv/photos")),
        "/srv/photos"
    );
    assert_eq!(h.core.edit_address(&uri("lautta://nv-nas/")), "/");
}

#[tokio::test]
async fn completion_comes_from_cached_listings() {
    let h = home().await;
    let base = uri("lautta://user-documents/");
    for d in ["Thesis", "Theory notes", "Other"] {
        std::fs::create_dir_all(h.root.join("Documents/Uni").join(d)).unwrap();
    }
    std::fs::write(h.root.join("Documents/Uni/Thesis.txt"), b"x").unwrap();
    let uni = uri("lautta://user-documents/Uni");
    assert!(
        h.core.complete_address("~/Documents/Uni/The", &base).is_empty(),
        "nothing cached yet"
    );
    events(&h.core, &uni, true).await;
    let found = h.core.complete_address("~/Documents/Uni/the", &base);
    let found: Vec<String> = found.iter().map(|u| u.to_string()).collect();
    assert_eq!(
        found,
        [
            "lautta://user-documents/Uni/Theory%20notes",
            "lautta://user-documents/Uni/Thesis"
        ],
        "folders only, case-insensitive, sorted"
    );
    assert_eq!(h.core.complete_address("~/Documents/Uni/", &base).len(), 3);
    assert!(h.core.complete_address("no slash", &base).is_empty());
}

#[tokio::test]
async fn capabilities_and_picker_locations() {
    let h = home().await;
    assert!(h
        .core
        .capability_flags("user-documents")
        .contains(&cap::WRITE.to_owned()));
    assert!(h.core.capability_flags("nope").is_empty());
    let ids: Vec<String> = h.core.picker_locations().into_iter().map(|l| l.id).collect();
    assert!(ids.contains(&"user-documents".to_owned()));
    assert!(ids.contains(&"user-downloads".to_owned()));
}

#[test]
fn file_urls_escape_what_breaks_urls() {
    let u = lautta_core::app_directory::file_url(std::path::Path::new("/home/a b/c#d%e?.png"));
    assert_eq!(u, "file:///home/a%20b/c%23d%25e%3F.png");
}
