// SPDX-License-Identifier: LGPL-2.1-or-later

use lautta_core::app::Core;
use lautta_core::app_directory::{ListEvent, PathStep};
use lautta_core::entry::cap;
use lautta_core::foldercount::{FolderCount, COUNT_CAP};
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
    let shown: Vec<(&str, String, &str)> = steps
        .iter()
        .map(|s| (s.name.as_str(), s.uri.to_string(), s.icon))
        .collect();
    assert_eq!(
        shown,
        [
            ("Documents", "lautta://user-documents/".to_owned(), "file-folder"),
            ("Uni", "lautta://user-documents/Uni".to_owned(), "file-folder"),
            (
                "Thesis",
                "lautta://user-documents/Uni/Thesis".to_owned(),
                "file-folder"
            ),
        ]
    );
    let root: Vec<PathStep> = h.core.path_steps(&uri("lautta://user-documents/"));
    assert_eq!(root.len(), 1);
}

fn nas(h: &Home) -> std::sync::Arc<lautta_core::provider::memory::MemoryProvider> {
    use lautta_core::provider::memory::MemoryProvider;
    let loc = lautta_core::locations::Location::remote(
        "nv-nas",
        lautta_core::locations::LocationKind::AdHoc,
        "NAS",
        Some("sftp://nas.home".to_owned()),
    );
    let provider = std::sync::Arc::new(MemoryProvider::new(lautta_core::entry::Capabilities::with(&[
        cap::WRITE,
    ])));
    h.core.locations.register(loc, provider.clone());
    provider
}

fn steps_of(h: &Home, at: &Uri) -> Vec<(String, &'static str)> {
    h.core
        .path_steps(at)
        .into_iter()
        .map(|s| (s.name, s.icon))
        .collect()
}

#[tokio::test]
async fn path_steps_show_the_kind_of_location() {
    let h = home().await;
    nas(&h);
    assert_eq!(
        steps_of(&h, &uri("lautta://nv-nas/srv")),
        [("NAS".to_owned(), "server"), ("srv".to_owned(), "file-folder")]
    );
    std::fs::create_dir_all(h.root.join("media/SD/DCIM")).unwrap();
    h.core.locations.refresh();
    assert_eq!(
        steps_of(&h, &uri("lautta://vol-SD/DCIM"))[0].1,
        "image://theme/icon-m-sd-card"
    );
    assert_eq!(
        steps_of(&h, &uri("lautta://gone/a")),
        [(String::new(), "file-folder"), ("a".to_owned(), "file-folder")],
        "a location that went away still has its steps"
    );
}

fn zip_at(path: &std::path::Path, inner: &str) {
    let file = std::fs::File::create(path).unwrap();
    let mut zip = zip::ZipWriter::new(file);
    zip.start_file(inner, zip::write::SimpleFileOptions::default())
        .unwrap();
    std::io::Write::write_all(&mut zip, b"x").unwrap();
    zip.finish().unwrap();
}

#[tokio::test]
async fn path_steps_go_on_out_of_an_archive() {
    let h = home().await;
    std::fs::create_dir_all(h.root.join("Documents/Uni")).unwrap();
    zip_at(&h.root.join("Documents/Uni/data.zip"), "inner/more/readme.txt");
    let id = h
        .core
        .open_archive(&uri("lautta://user-documents/Uni/data.zip"))
        .await
        .unwrap();
    let inner = uri(&format!("lautta://{id}/inner/more"));
    let steps = h.core.path_steps(&inner);
    let shown: Vec<(&str, String, &str)> = steps
        .iter()
        .map(|s| (s.name.as_str(), s.uri.to_string(), s.icon))
        .collect();
    assert_eq!(
        shown,
        [
            ("Documents", "lautta://user-documents/".to_owned(), "file-folder"),
            ("Uni", "lautta://user-documents/Uni".to_owned(), "file-folder"),
            ("data.zip", format!("lautta://{id}/"), "file-archive"),
            ("inner", format!("lautta://{id}/inner"), "file-folder"),
            ("more", format!("lautta://{id}/inner/more"), "file-folder"),
        ]
    );
}

#[tokio::test]
async fn path_steps_follow_archives_inside_archives_up_to_a_depth() {
    let h = home().await;
    let mut holder = "lautta://user-documents/a0.zip".to_owned();
    for i in 0..10 {
        let id = format!("arc-{i}");
        let loc = lautta_core::locations::Location::remote(
            id.clone(),
            lautta_core::locations::LocationKind::Archive,
            format!("a{i}.zip"),
            Some(holder.clone()),
        );
        let provider = std::sync::Arc::new(lautta_core::provider::memory::MemoryProvider::new(
            lautta_core::entry::Capabilities::with(&[]),
        ));
        h.core.locations.register(loc, provider);
        holder = format!("lautta://{id}/a{}.zip", i + 1);
    }
    let steps = h.core.path_steps(&uri("lautta://arc-9/x"));
    let names: Vec<&str> = steps.iter().map(|s| s.name.as_str()).collect();
    assert_eq!(
        names,
        ["a2.zip", "a3.zip", "a4.zip", "a5.zip", "a6.zip", "a7.zip", "a8.zip", "a9.zip", "x"],
        "eight archives out, then it stops"
    );
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
async fn a_location_name_starts_an_address() {
    let h = home().await;
    nas(&h);
    let base = uri("lautta://user-documents/");
    let uni = uri("lautta://user-documents/Uni");
    assert_eq!(h.core.resolve_address("Documents/Uni", &base), Some(uni.clone()));
    assert_eq!(h.core.resolve_address("documents/Uni", &base), Some(uni));
    assert_eq!(
        h.core.resolve_address("Documents/", &base),
        Some(uri("lautta://user-documents/"))
    );
    assert_eq!(
        h.core.resolve_address("Documents/../Downloads", &base),
        Some(uri("lautta://user-downloads/"))
    );
    assert_eq!(
        h.core.resolve_address("NAS/srv/photos", &base),
        Some(uri("lautta://nv-nas/srv/photos"))
    );
    assert_eq!(h.core.resolve_address("NAS/../etc", &base), None);
    assert_eq!(h.core.resolve_address("Nowhere/Uni", &base), None);
    assert_eq!(
        h.core.resolve_address("Documents", &base),
        None,
        "a name alone is no address"
    );
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
async fn a_device_folder_counts_the_items_the_view_would_show() {
    let h = home().await;
    let uni = h.root.join("Documents/Uni");
    std::fs::create_dir_all(uni.join("Thesis")).unwrap();
    std::fs::write(uni.join("a.txt"), b"a").unwrap();
    std::fs::write(uni.join(".hidden"), b"h").unwrap();
    let dir = uri("lautta://user-documents/Uni");
    assert_eq!(
        h.core.count_folder(&dir, false).await,
        Some(FolderCount {
            shown: 2,
            capped: false
        })
    );
    assert_eq!(h.core.count_folder(&dir, true).await.map(|c| c.shown), Some(3));
}

#[tokio::test]
async fn a_folder_on_a_memory_card_is_counted() {
    let h = home().await;
    std::fs::create_dir_all(h.root.join("media/SD/DCIM")).unwrap();
    std::fs::write(h.root.join("media/SD/DCIM/a.jpg"), b"a").unwrap();
    h.core.locations.refresh();
    assert_eq!(
        h.core.count_folder(&uri("lautta://vol-SD/DCIM"), false).await,
        Some(FolderCount {
            shown: 1,
            capped: false
        })
    );
}

#[tokio::test]
async fn a_folder_that_cannot_be_read_has_no_count() {
    let h = home().await;
    std::fs::write(h.root.join("Documents/a.txt"), b"a").unwrap();
    assert_eq!(
        h.core
            .count_folder(&uri("lautta://user-documents/gone"), false)
            .await,
        None
    );
    assert_eq!(
        h.core
            .count_folder(&uri("lautta://user-documents/a.txt"), false)
            .await,
        None
    );
    assert_eq!(h.core.count_folder(&uri("lautta://nowhere/"), false).await, None);
}

#[tokio::test]
async fn a_server_folder_is_not_counted() {
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
    assert_eq!(
        h.core.count_folder(&uri("lautta://nv-nas/srv"), false).await,
        None
    );
    assert_eq!(h.core.count_folder(&uri("lautta://nv-nas/"), true).await, None);
}

#[tokio::test]
async fn a_huge_folder_is_counted_up_to_the_cap() {
    let h = home().await;
    let big = h.root.join("Documents/big");
    std::fs::create_dir_all(&big).unwrap();
    for i in 0..=COUNT_CAP {
        std::fs::File::create(big.join(i.to_string())).unwrap();
    }
    assert_eq!(
        h.core
            .count_folder(&uri("lautta://user-documents/big"), false)
            .await,
        Some(FolderCount {
            shown: COUNT_CAP,
            capped: true
        })
    );
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

fn hits(h: &Home, text: &str, base: &Uri) -> Vec<(String, &'static str, String)> {
    h.core
        .goto_hits(text, base)
        .into_iter()
        .map(|g| (g.name, g.source.as_str(), g.place))
        .collect()
}

fn hit(name: &str, source: &'static str, place: &str) -> (String, &'static str, String) {
    (name.to_owned(), source, place.to_owned())
}

#[tokio::test]
async fn go_to_folder_finds_folders_by_name() {
    let h = home().await;
    let docs = h.root.join("Documents");
    for d in [
        "Uni/Thesis/3 Methods",
        "Uni/Run 1",
        "Uni/Rerun",
        "Uni/Summer runs",
        "Uni/x",
    ] {
        std::fs::create_dir_all(docs.join(d)).unwrap();
    }
    std::fs::write(docs.join("Uni/run-notes.txt"), b"x").unwrap();
    let base = uri("lautta://user-documents/Uni");
    assert!(hits(&h, "run", &base).is_empty(), "nothing listed yet");
    events(&h.core, &base, true).await;
    events(&h.core, &uri("lautta://user-documents/Uni/Thesis"), true).await;
    h.core
        .recents
        .record(
            &uri("lautta://user-documents/Uni/Thesis/3%20Methods/run.pdf"),
            "run.pdf",
            lautta_core::org::RecentKind::Opened,
        )
        .unwrap();
    h.core
        .favourites
        .add(&uri("lautta://user-downloads/"), "Downloads", None, true)
        .unwrap();

    assert_eq!(
        hits(&h, " RUN ", &base),
        [
            hit("Run 1", "here", "Documents › Uni"),
            hit("Summer runs", "here", "Documents › Uni"),
            hit("Rerun", "here", "Documents › Uni"),
        ],
        "name starts with it, then a word, then anywhere; files never"
    );
    assert_eq!(
        hits(&h, "meth", &base),
        [hit("3 Methods", "recent", "Documents › Uni › Thesis")],
        "the folder holding a recent file"
    );
    assert_eq!(
        hits(&h, "down", &base),
        [hit("Downloads", "favourite", "")],
        "a favourite folder; a location has no place"
    );
    assert_eq!(
        hits(&h, "thes", &base),
        [hit("Thesis", "here", "Documents › Uni")],
        "the same folder once, from the first source"
    );
    assert_eq!(
        hits(&h, "uni", &base),
        Vec::<(String, &str, String)>::new(),
        "not the folder shown"
    );
    assert!(hits(&h, "Documents", &base).is_empty(), "never listed");
    events(&h.core, &uri("lautta://user-documents/"), true).await;
    assert_eq!(hits(&h, "Documents", &base), [hit("Documents", "cached", "")]);
    assert!(hits(&h, "  ", &base).is_empty());

    h.core.recents.set_enabled(false);
    assert!(
        hits(&h, "meth", &base).is_empty(),
        "recents off, no recent folders"
    );
}

#[tokio::test]
async fn go_to_folder_leaves_out_locations_that_are_gone() {
    let h = home().await;
    let provider = nas(&h);
    provider.add_dir("srv");
    provider.add_dir("srv/photos");
    let srv = uri("lautta://nv-nas/srv");
    events(&h.core, &srv, true).await;
    let base = uri("lautta://user-documents/");
    assert_eq!(hits(&h, "srv", &base), [hit("srv", "cached", "NAS")]);
    h.core.locations.unregister("nv-nas");
    assert!(hits(&h, "srv", &base).is_empty());
}

#[tokio::test]
async fn go_to_folder_caps_its_hits() {
    let h = home().await;
    for i in 0..40 {
        std::fs::create_dir_all(h.root.join(format!("Documents/f{i}"))).unwrap();
    }
    let base = uri("lautta://user-documents/");
    events(&h.core, &base, true).await;
    assert_eq!(h.core.goto_hits("f", &base).len(), 30);
}

fn path_of(h: &Home, text: &str, base: &Uri) -> (Option<String>, Vec<String>) {
    let p = h.core.goto_path(text, base);
    (
        p.target.map(|u| u.to_string()),
        p.folders.into_iter().map(|u| u.to_string()).collect(),
    )
}

#[tokio::test]
async fn go_to_a_typed_path() {
    let h = home().await;
    for d in ["Uni/Thesis", "Uni/Theory", "Uni/Thesis/Draft"] {
        std::fs::create_dir_all(h.root.join("Documents").join(d)).unwrap();
    }
    let base = uri("lautta://user-documents/");
    let uni = "lautta://user-documents/Uni".to_owned();
    assert_eq!(
        path_of(&h, "~/Documents/Uni", &base),
        (Some(uni.clone()), vec![]),
        "a local folder is there before it was ever listed"
    );
    events(&h.core, &uri(&uni), true).await;
    let both = vec![
        "lautta://user-documents/Uni/Theory".to_owned(),
        "lautta://user-documents/Uni/Thesis".to_owned(),
    ];
    assert_eq!(
        path_of(&h, "~/Documents/Uni", &base),
        (Some(uni.clone()), both.clone())
    );
    assert_eq!(path_of(&h, "Documents/Uni/ ", &base), (Some(uni), both));
    assert_eq!(
        path_of(&h, "~/Documents/Uni/thes", &base),
        (None, vec!["lautta://user-documents/Uni/Thesis".to_owned()]),
        "no such folder: the parent's folders that start so"
    );
    assert_eq!(path_of(&h, "~/Documents/Nope/", &base), (None, vec![]));
    assert_eq!(path_of(&h, "Documents/Uni/Thesis/Draft/x", &base), (None, vec![]));
}

#[tokio::test]
async fn go_to_a_server_path_knows_what_was_listed() {
    let h = home().await;
    let provider = nas(&h);
    provider.add_dir("srv");
    provider.add_dir("srv/photos");
    let base = uri("lautta://nv-nas/");
    let photos = "lautta://nv-nas/srv/photos".to_owned();
    assert_eq!(
        path_of(&h, "/", &base),
        (Some("lautta://nv-nas/".to_owned()), vec![]),
        "a location's root is always there"
    );
    assert_eq!(
        path_of(&h, "/srv/photos", &base),
        (None, vec![]),
        "nothing listed yet"
    );
    events(&h.core, &uri("lautta://nv-nas/srv"), true).await;
    assert_eq!(path_of(&h, "/srv/photos", &base), (Some(photos.clone()), vec![]));
    assert_eq!(
        path_of(&h, "NAS/srv/", &base).0,
        Some("lautta://nv-nas/srv".to_owned())
    );
    events(&h.core, &uri(&photos), true).await;
    assert_eq!(path_of(&h, "/srv/photos", &base).0, Some(photos));
    std::fs::create_dir_all(h.root.join("Documents/srv")).unwrap();
    assert_eq!(path_of(&h, "/srv/none", &base), (None, vec![]));
    h.core.locations.unregister("nv-nas");
    assert_eq!(
        path_of(&h, "lautta://nv-nas/srv", &base),
        (None, vec![]),
        "location gone"
    );
}

#[test]
fn go_to_takes_a_tilde_or_a_slash_for_a_path() {
    use lautta_core::app_directory::looks_like_address;
    assert!(looks_like_address("~"));
    assert!(looks_like_address("  ~/Documents"));
    assert!(looks_like_address("Documents/Uni"));
    assert!(looks_like_address("/srv"));
    assert!(!looks_like_address("run 1"));
    assert!(!looks_like_address("a~b"), "a tilde only at the start");
}
