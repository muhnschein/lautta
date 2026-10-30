// SPDX-License-Identifier: LGPL-2.1-or-later

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

    let moved = h.root.join("Documents/bundle.zip");
    std::fs::rename(h.root.join("Downloads/bundle.zip"), &moved).unwrap();
    let (into, started) = h
        .core
        .extract_here(&uri("lautta://user-documents/bundle.zip"))
        .await
        .unwrap();
    assert_eq!(
        into,
        uri("lautta://user-documents/src%202"),
        "one top folder lands as itself"
    );
    finish(&h, started).await;
    assert_eq!(
        std::fs::read(h.root.join("Documents/src 2/a.txt")).unwrap(),
        b"alpha"
    );
    assert_eq!(
        std::fs::read(h.root.join("Documents/src 2/sub/b.txt")).unwrap(),
        b"beta"
    );
}

async fn pack(h: &Home, sources: &[&str], dest: &str, name: &str, kind: ArchiveKind) -> Uri {
    let sources: Vec<Uri> = sources.iter().map(|s| uri(s)).collect();
    h.core
        .compress_to(
            &sources,
            &uri(dest),
            name,
            kind,
            no_progress(),
            Arc::new(AtomicBool::new(false)),
        )
        .await
        .unwrap()
}

async fn extract_here(h: &Home, archive: &Uri) -> Uri {
    let (into, started) = h.core.extract_here(archive).await.unwrap();
    finish(h, started).await;
    into
}

#[tokio::test]
async fn extract_here_puts_several_items_in_a_folder_named_after_the_archive() {
    let h = home().await;
    write(&h.root.join("Documents/a.txt"), b"alpha");
    write(&h.root.join("Documents/b/c.txt"), b"gamma");
    let archive = pack(
        &h,
        &["lautta://user-documents/a.txt", "lautta://user-documents/b"],
        "lautta://user-downloads/",
        "dataset",
        ArchiveKind::TarGz,
    )
    .await;
    assert_eq!(archive, uri("lautta://user-downloads/dataset.tar.gz"));

    let into = extract_here(&h, &archive).await;
    assert_eq!(into, uri("lautta://user-downloads/dataset"));
    assert_eq!(
        std::fs::read(h.root.join("Downloads/dataset/a.txt")).unwrap(),
        b"alpha"
    );
    assert_eq!(
        std::fs::read(h.root.join("Downloads/dataset/b/c.txt")).unwrap(),
        b"gamma"
    );

    let again = extract_here(&h, &archive).await;
    assert_eq!(
        again,
        uri("lautta://user-downloads/dataset%202"),
        "a taken name gets a number"
    );
    assert_eq!(
        std::fs::read(h.root.join("Downloads/dataset 2/a.txt")).unwrap(),
        b"alpha"
    );
}

#[tokio::test]
async fn extract_here_does_not_double_a_single_top_folder() {
    let h = home().await;
    write(&h.root.join("Documents/dataset/x.txt"), b"x");
    let archive = pack(
        &h,
        &["lautta://user-documents/dataset"],
        "lautta://user-downloads/",
        "release",
        ArchiveKind::Zip,
    )
    .await;
    let into = extract_here(&h, &archive).await;
    assert_eq!(
        into,
        uri("lautta://user-downloads/dataset"),
        "top folder keeps its own name"
    );
    assert_eq!(
        std::fs::read(h.root.join("Downloads/dataset/x.txt")).unwrap(),
        b"x"
    );
    assert!(!h.root.join("Downloads/release").exists());
    assert!(!h.root.join("Downloads/dataset/dataset").exists());
}

#[tokio::test]
async fn extract_here_wraps_a_single_file_and_an_empty_top_folder() {
    let h = home().await;
    write(&h.root.join("Documents/one.txt"), b"1");
    std::fs::create_dir_all(h.root.join("Documents/hollow")).unwrap();
    let file = pack(
        &h,
        &["lautta://user-documents/one.txt"],
        "lautta://user-downloads/",
        "single",
        ArchiveKind::Zip,
    )
    .await;
    assert_eq!(
        extract_here(&h, &file).await,
        uri("lautta://user-downloads/single")
    );
    assert_eq!(
        std::fs::read(h.root.join("Downloads/single/one.txt")).unwrap(),
        b"1"
    );

    let empty = pack(
        &h,
        &["lautta://user-documents/hollow"],
        "lautta://user-downloads/",
        "shell",
        ArchiveKind::Zip,
    )
    .await;
    assert_eq!(
        extract_here(&h, &empty).await,
        uri("lautta://user-downloads/shell")
    );
    assert!(h.root.join("Downloads/shell/hollow").is_dir());
}

#[tokio::test]
async fn extract_here_refuses_a_read_only_folder() {
    let h = home().await;
    write(&h.root.join("Documents/pack/f.txt"), b"f");
    pack(
        &h,
        &["lautta://user-documents/pack/f.txt"],
        "lautta://user-documents/pack/",
        "inner",
        ArchiveKind::Zip,
    )
    .await;
    let outer = pack(
        &h,
        &["lautta://user-documents/pack"],
        "lautta://user-documents/",
        "outer",
        ArchiveKind::Zip,
    )
    .await;
    let location = h.core.open_archive(&outer).await.unwrap();
    let inner = Uri::parse(&format!("lautta://{location}/pack/inner.zip")).unwrap();
    let err = h.core.extract_here(&inner).await.unwrap_err();
    assert_eq!(err.kind, ErrorKind::ReadOnlyFilesystem);
    let root = Uri::parse(&format!("lautta://{location}/")).unwrap();
    assert!(
        h.core.extract_here(&root).await.is_err(),
        "an archive root has no folder"
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
    assert_eq!(h.core.empty_trash().unwrap(), 1);
    assert!(h.core.trash_entries(now).unwrap().is_empty());
    assert_eq!(h.core.trash.total_size().unwrap(), 0);
}

#[tokio::test]
async fn the_trash_at_start_loses_the_expired_and_the_half_made_copies() {
    let h = home().await;
    write(&h.root.join("Documents/old.txt"), b"1");
    write(&h.root.join("Documents/new.txt"), b"22");
    h.core
        .delete(&[uri("lautta://user-documents/old.txt")])
        .await
        .unwrap();
    let now = lautta_core::trash::now_secs();
    h.core
        .delete(&[uri("lautta://user-documents/new.txt")])
        .await
        .unwrap();
    let partial = h.core.trash.dir().join(".lautta-partial-1");
    write(&partial.join("x"), b"half");
    let mut settings = h.core.settings();
    settings.recently_deleted_retention_days = 40;
    h.core.apply_settings(settings);
    assert_eq!(h.core.tidy_trash(now + 31 * 86_400).unwrap(), 0);
    assert!(!partial.exists(), "the half-made copy goes");
    assert_eq!(h.core.trash.list().unwrap().len(), 2);
    assert_eq!(h.core.tidy_trash(now + 41 * 86_400).unwrap(), 2);
    assert!(h.core.trash.list().unwrap().is_empty());
}

async fn trashed(h: &Home, names: &[&str]) -> std::collections::HashMap<String, TrashEntry> {
    let uris: Vec<Uri> = names
        .iter()
        .map(|n| uri(&format!("lautta://user-documents/{n}")))
        .collect();
    h.core.delete(&uris).await.unwrap();
    h.core
        .trash_entries(lautta_core::trash::now_secs())
        .unwrap()
        .into_iter()
        .map(|e| (e.name.clone(), e))
        .collect()
}

fn stored_url(h: &Home, row: &TrashEntry) -> String {
    let item = h
        .core
        .trash
        .list()
        .unwrap()
        .into_iter()
        .find(|i| i.id == row.id)
        .unwrap();
    lautta_core::app_directory::file_url(&item.stored_path())
}

#[tokio::test]
async fn trashed_images_and_videos_carry_what_the_thumbnailer_needs() {
    let h = home().await;
    write(&h.root.join("Documents/photo.JPG"), b"jpeg");
    write(&h.root.join("Documents/clip.mp4"), b"video");
    write(&h.root.join("Documents/notes.txt"), b"text");
    write(&h.root.join("Documents/no extension"), b"jpeg");
    write(&h.root.join("Documents/album.png/inner.txt"), b"x");
    std::os::unix::fs::symlink("photo.JPG", h.root.join("Documents/link.png")).unwrap();
    let rows = trashed(
        &h,
        &[
            "photo.JPG",
            "clip.mp4",
            "notes.txt",
            "no extension",
            "album.png",
            "link.png",
        ],
    )
    .await;
    assert_eq!(rows.len(), 6);

    let photo = &rows["photo.JPG"];
    assert_eq!(photo.mime_type, "image/jpeg");
    assert_eq!(photo.thumbnail_source, stored_url(&h, photo));
    assert!(photo.thumbnail_source.starts_with("file://"));
    let clip = &rows["clip.mp4"];
    assert_eq!(clip.mime_type, "video/mp4");
    assert_eq!(clip.thumbnail_source, stored_url(&h, clip));
    assert_eq!(rows["notes.txt"].mime_type, "text/plain");
    assert_eq!(rows["notes.txt"].thumbnail_source, "");
    assert_eq!(rows["no extension"].mime_type, "");
    assert_eq!(rows["no extension"].thumbnail_source, "");
    assert_eq!(rows["album.png"].mime_type, "", "a folder has no file type");
    assert_eq!(rows["album.png"].thumbnail_source, "");
    assert_eq!(rows["link.png"].thumbnail_source, "");
}

fn pdf() -> Vec<u8> {
    use lautta_pdf::sample::{build, page};
    build(&[page(100, 200, "1 0 0 rg 0 0 50 50 re f")])
}

#[tokio::test]
async fn trashed_pdfs_carry_what_the_thumbnailer_needs() {
    let h = home().await;
    write(&h.root.join("Documents/doc.PDF"), &pdf());
    write(&h.root.join("Documents/dir.pdf/inner.txt"), b"x");
    write(&h.root.join("Documents/huge.pdf"), b"");
    std::fs::File::options()
        .write(true)
        .open(h.root.join("Documents/huge.pdf"))
        .unwrap()
        .set_len(lautta_pdf::MAX_PDF_BYTES + 1)
        .unwrap();
    std::os::unix::fs::symlink("doc.PDF", h.root.join("Documents/link.pdf")).unwrap();
    let rows = trashed(&h, &["doc.PDF", "dir.pdf", "huge.pdf", "link.pdf"]).await;
    assert_eq!(rows.len(), 4);

    let doc = &rows["doc.PDF"];
    assert_eq!(
        doc.mime_type, "application/pdf",
        "the type comes from the original name: the thumbnailer draws PDFs by it"
    );
    assert_eq!(
        doc.thumbnail_source,
        stored_url(&h, doc),
        "the system thumbnailer draws the stored copy"
    );
    assert_eq!(rows["dir.pdf"].thumbnail_source, "", "a folder");
    let huge = &rows["huge.pdf"];
    assert_eq!(
        huge.thumbnail_source,
        stored_url(&h, huge),
        "no size limit: the app reads none of it"
    );
    assert_eq!(
        rows["link.pdf"].thumbnail_source, "",
        "a link would show its target"
    );
}

#[tokio::test]
async fn trashed_pdfs_follow_the_local_thumbnail_switch() {
    let h = home().await;
    write(&h.root.join("Documents/doc.pdf"), &pdf());
    let rows = trashed(&h, &["doc.pdf"]).await;
    assert!(!rows["doc.pdf"].thumbnail_source.is_empty());
    let source = |h: &Home| {
        h.core.trash_entries(lautta_core::trash::now_secs()).unwrap()[0]
            .thumbnail_source
            .clone()
    };

    let mut settings = h.core.settings();
    settings.thumbnails_remote = false;
    h.core.apply_settings(settings.clone());
    assert!(!source(&h).is_empty(), "the servers' switch is for servers");
    settings.thumbnails_local = false;
    h.core.apply_settings(settings);
    assert_eq!(source(&h), "", "thumbnails off");

    let mut settings = h.core.settings();
    settings.thumbnails_local = true;
    h.core.apply_settings(settings);
    let item = h.core.trash.list().unwrap().remove(0);
    std::fs::remove_file(item.stored_path()).unwrap();
    assert_eq!(source(&h), "");
}

#[tokio::test]
async fn the_thumbnails_setting_decides_whether_trashed_media_show_them() {
    let h = home().await;
    write(&h.root.join("Documents/photo.png"), b"png");
    let rows = trashed(&h, &["photo.png"]).await;
    assert!(!rows["photo.png"].thumbnail_source.is_empty());

    let mut settings = h.core.settings();
    settings.thumbnails_local = false;
    h.core.apply_settings(settings);
    let off = h.core.trash_entries(lautta_core::trash::now_secs()).unwrap();
    assert_eq!(off[0].thumbnail_source, "", "thumbnails switched off");
    assert_eq!(off[0].mime_type, "image/png", "the type does not depend on it");

    let mut settings = h.core.settings();
    settings.thumbnails_local = true;
    h.core.apply_settings(settings);
    let item = h.core.trash.list().unwrap().remove(0);
    std::fs::remove_file(item.stored_path()).unwrap();
    let gone = h.core.trash_entries(lautta_core::trash::now_secs()).unwrap();
    assert_eq!(gone[0].thumbnail_source, "");
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
        .extract_here(&uri("lautta://user-documents/not-an-archive.zip"))
        .await;
    assert!(err.is_err());
    assert!(!h.root.join("Documents/not-an-archive").exists());
}
