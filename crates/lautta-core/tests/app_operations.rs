// SPDX-License-Identifier: LGPL-2.1-or-later

use lautta_core::app::{Core, Started};
use lautta_core::app_archives::ArchiveHold;
use lautta_core::app_operations::*;
use lautta_core::compress::{ArchiveKind, CompressOptions};
use lautta_core::entry::{cap, Capabilities};
use lautta_core::locations::{Location, LocationKind, LocationRegistry};
use lautta_core::ops::{ConflictChoice, OperationKind};
use lautta_core::password::Password;
use lautta_core::paths::AppPaths;
use lautta_core::provider::local::LocalProvider;
use lautta_core::provider::memory::MemoryProvider;
use lautta_core::settings::Settings;
use lautta_core::transfer::{TransferState, WaitReason};
use lautta_core::{Error, ErrorKind, Uri};
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
        .compress_to(&sources, &dest, "bundle", CompressOptions::new(ArchiveKind::Zip))
        .await
        .unwrap();
    assert_eq!(zip, uri("lautta://user-downloads/bundle.zip"));
    assert!(h.root.join("Downloads/bundle.zip").is_file());
    let again = h
        .core
        .compress_to(&sources, &dest, "bundle", CompressOptions::new(ArchiveKind::Zip))
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

    let started = h.core.extract(&zip, None).await.unwrap();
    finish(&h, started).await;
    assert_eq!(
        std::fs::read(h.root.join("Downloads/src/a.txt")).unwrap(),
        b"alpha",
        "one top folder comes out as is, next to the archive"
    );
    assert_eq!(
        std::fs::read(h.root.join("Downloads/src/sub/b.txt")).unwrap(),
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
        .compress_to(&src, &dest, "pack", CompressOptions::new(ArchiveKind::TarGz))
        .await
        .unwrap();
    assert_eq!(out.to_string(), "lautta://user-downloads/pack.tar.gz");

    let canceled = h
        .core
        .compress_to(
            &src,
            &dest,
            "stop",
            CompressOptions {
                cancel: Arc::new(AtomicBool::new(true)),
                ..CompressOptions::new(ArchiveKind::Zip)
            },
        )
        .await
        .unwrap_err();
    assert_eq!(canceled.kind, ErrorKind::Canceled);
    assert!(!h.root.join("Downloads/stop.zip").exists());
    let leftovers = std::fs::read_dir(h.root.join("Downloads")).unwrap().count();
    assert_eq!(leftovers, 1, "a canceled run leaves nothing behind");

    let bad = h
        .core
        .compress_to(&src, &dest, "a/b", CompressOptions::new(ArchiveKind::Zip))
        .await
        .unwrap_err();
    assert_eq!(bad.kind, ErrorKind::InvalidName);
    let none = h
        .core
        .compress_to(&[], &dest, "x", CompressOptions::new(ArchiveKind::Zip))
        .await;
    assert!(none.is_err());
}

fn register_remote(h: &Home) -> tempfile::TempDir {
    let dir = tempfile::tempdir().unwrap();
    let provider = LocalProvider::new(dir.path().to_path_buf());
    let location = Location::remote(
        "nas",
        LocationKind::Server {
            provider: "sftp".into(),
        },
        "NAS",
        Some("sftp://nas.home".into()),
    );
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
            CompressOptions::new(ArchiveKind::Zip),
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
            CompressOptions::new(ArchiveKind::Zip),
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
        Location::remote(
            "ro",
            LocationKind::Server {
                provider: "sftp".into(),
            },
            "RO",
            None,
        ),
        Arc::new(mem),
    );
    let err = h
        .core
        .compress_to(
            &[uri("lautta://user-documents/f.txt")],
            &uri("lautta://ro/"),
            "x",
            CompressOptions::new(ArchiveKind::Zip),
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
async fn summary_shows_origin_and_destination_as_tiles() {
    let h = home().await;
    write(&h.root.join("Documents/pics/pic.jpg"), b"new");
    write(&h.root.join("Documents/pics/deep/more.txt"), b"m");
    write(&h.root.join("Documents/notes.txt"), b"n");
    write(&h.root.join("Downloads/in/pic.jpg"), b"old");
    write(&h.root.join("Downloads/in/deep/x.txt"), b"x");

    let one = h
        .core
        .plan(
            OperationKind::Copy,
            vec![uri("lautta://user-documents/pics/pic.jpg")],
            uri("lautta://user-downloads/in"),
        )
        .await
        .unwrap();
    let summary = h.core.plan_summary(&one).await;
    assert_eq!(
        (
            summary.destination_name.as_str(),
            summary.destination_folder.as_str()
        ),
        ("in", "Downloads")
    );
    let origin = summary.origin.expect("a copy has an origin");
    assert_eq!(
        (
            origin.name.as_str(),
            origin.folder.as_str(),
            origin.more,
            origin.is_dir
        ),
        ("pic.jpg", "pics", 0, false)
    );
    assert_eq!(origin.uri, "lautta://user-documents/pics/pic.jpg");
    assert_eq!(origin.look.category, "image");
    assert!(origin.look.thumbnail_source.starts_with("file://"));
    assert!(origin.look.thumbnail_source.ends_with("/Documents/pics/pic.jpg"));

    let list = h.core.unresolved_conflicts(&one);
    assert_eq!(list.len(), 1);
    assert!(list[0]
        .source_look
        .thumbnail_source
        .ends_with("/Documents/pics/pic.jpg"));
    assert!(list[0]
        .destination_look
        .thumbnail_source
        .ends_with("/Downloads/in/pic.jpg"));

    let many = h
        .core
        .plan(
            OperationKind::Copy,
            vec![
                uri("lautta://user-documents/pics"),
                uri("lautta://user-documents/notes.txt"),
            ],
            uri("lautta://user-downloads/"),
        )
        .await
        .unwrap();
    let summary = h.core.plan_summary(&many).await;
    assert_eq!(
        (
            summary.destination_name.as_str(),
            summary.destination_folder.as_str()
        ),
        ("Downloads", "")
    );
    let origin = summary.origin.expect("a copy has an origin");
    assert_eq!(
        (
            origin.name.as_str(),
            origin.folder.as_str(),
            origin.more,
            origin.is_dir
        ),
        ("pics", "Documents", 1, true),
        "items inside a copied folder are not counted as more"
    );
    assert_eq!(origin.look, Default::default(), "a folder shows its glyph");

    std::fs::create_dir_all(h.root.join("Downloads/pics/deep")).unwrap();
    let dirs = h
        .core
        .plan(
            OperationKind::Copy,
            vec![uri("lautta://user-documents/pics")],
            uri("lautta://user-downloads/"),
        )
        .await
        .unwrap();
    let list = h.core.unresolved_conflicts(&dirs);
    assert!(list[0].conflict.dst_is_dir);
    assert_eq!(list[0].source_look, Default::default());
    assert_eq!(list[0].destination_look, Default::default());
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
async fn shared_files_are_probed() {
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
}

#[tokio::test]
async fn share_starts_in_downloads_then_where_it_last_saved() {
    let h = home().await;
    let first = h.core.share_places();
    assert_eq!(first.start, "lautta://user-downloads/");
    assert!(first.recent.is_empty());

    let _remote = register_remote(&h);
    std::fs::create_dir_all(h.root.join("Documents/Uni/Thesis")).unwrap();
    let saved_to = |folders: &[&str]| {
        let mut settings = h.core.settings();
        settings.share_folders = folders.iter().map(|f| (*f).to_owned()).collect();
        h.core.apply_settings(settings);
        let places = h.core.share_places();
        let shown: Vec<(String, String, String, bool)> = places
            .recent
            .into_iter()
            .map(|f| (f.uri, f.name, f.place, f.remote))
            .collect();
        (places.start, shown)
    };
    let row = |uri: &str, name: &str, place: &str, remote: bool| {
        (uri.to_owned(), name.to_owned(), place.to_owned(), remote)
    };

    let (start, shown) = saved_to(&[
        "lautta://user-documents/Uni/Thesis",
        "lautta://user-documents/Gone",
        "lautta://nas/photos/2026",
    ]);
    assert_eq!(start, "lautta://user-documents/Uni/Thesis");
    assert_eq!(
        shown,
        [
            row(
                "lautta://user-documents/Uni/Thesis",
                "Thesis",
                "Documents \u{203a} Uni",
                false
            ),
            row("lautta://nas/photos/2026", "2026", "NAS \u{203a} /photos", true),
        ],
        "a deleted folder is left out, a server folder is taken on trust"
    );

    let (start, shown) = saved_to(&["lautta://removed/x", "junk", "lautta://user-documents/"]);
    assert_eq!(start, "lautta://user-documents/");
    assert_eq!(shown, [row("lautta://user-documents/", "Documents", "", false)]);

    write(&h.root.join("Documents/Uni/notes.txt"), b"x");
    let (start, shown) = saved_to(&[
        "lautta://user-documents/Gone",
        "lautta://user-documents/Uni/notes.txt",
    ]);
    assert_eq!(start, "lautta://user-downloads/");
    assert!(shown.is_empty(), "only folders that are still there");
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
        .extract(&uri("lautta://user-documents/not-an-archive.zip"), None)
        .await;
    assert!(err.is_err());
}

#[tokio::test]
async fn extract_runs_as_extract_titled_by_its_archive() {
    let h = home().await;
    write(&h.root.join("Documents/src/a.txt"), b"alpha");
    write(&h.root.join("Documents/src/b.txt"), b"beta");
    let zip = h
        .core
        .compress_to(
            &[uri("lautta://user-documents/src")],
            &uri("lautta://user-downloads/"),
            "bundle",
            CompressOptions::new(ArchiveKind::Zip),
        )
        .await
        .unwrap();
    let done = |id| {
        let core = h.core.clone();
        async move { core.engine.wait_for(id, |s| s.state.is_finished()).await.unwrap() }
    };

    let Started::Transfer(id) = h.core.extract(&zip, None).await.unwrap() else {
        panic!("a small extract starts at once");
    };
    let s = done(id).await;
    assert_eq!(
        (s.state, s.kind, s.title.as_str()),
        (TransferState::Completed, OperationKind::Extract, "bundle.zip")
    );

    std::fs::create_dir(h.root.join("Documents/copied")).unwrap();
    let Started::Transfer(id) = h
        .core
        .copy_or_move(
            OperationKind::Copy,
            vec![uri("lautta://user-documents/src")],
            uri("lautta://user-documents/copied"),
        )
        .await
        .unwrap()
    else {
        panic!("a small copy starts at once");
    };
    assert_eq!(done(id).await.title, "src", "a copy is titled by its first item");

    h.core.apply_settings(Settings {
        large_op_items: 1,
        ..Settings::default()
    });
    let Started::NeedsSummary(plan) = h.core.extract(&zip, None).await.unwrap() else {
        panic!("a large extract needs the summary");
    };
    assert_eq!(plan.kind, OperationKind::Extract);
    let summary = h.core.plan_summary(&plan).await;
    assert_eq!(summary.kind, "extract");
    let origin = summary.origin.expect("an extract has its archive as origin");
    assert_eq!(
        (
            origin.name.as_str(),
            origin.folder.as_str(),
            origin.more,
            origin.is_dir
        ),
        ("bundle.zip", "Downloads", 0, false)
    );
    assert_eq!(origin.uri, zip.to_string());
    assert_eq!(origin.look.category, "archive");
    assert_eq!(
        h.core.plan_summary(&plan).await.conflicts,
        0,
        "a taken name is no question"
    );
    let s = done(h.core.start_plan(*plan).await.unwrap()).await;
    assert_eq!(
        (s.state, s.kind, s.title.as_str()),
        (TransferState::Completed, OperationKind::Extract, "bundle.zip")
    );
    assert_eq!(
        std::fs::read(h.root.join("Downloads/src 2/b.txt")).unwrap(),
        b"beta"
    );
}

#[tokio::test]
async fn extract_summary_left_unstarted_leaves_no_folder() {
    let h = home().await;
    write(&h.root.join("Documents/a.txt"), b"alpha");
    write(&h.root.join("Documents/b.txt"), b"beta");
    let zip = h
        .core
        .compress_to(
            &[
                uri("lautta://user-documents/a.txt"),
                uri("lautta://user-documents/b.txt"),
            ],
            &uri("lautta://user-documents/"),
            "bundle",
            CompressOptions::new(ArchiveKind::Zip),
        )
        .await
        .unwrap();
    h.core.apply_settings(Settings {
        large_op_items: 1,
        ..Settings::default()
    });
    let summary = || {
        let (core, zip) = (h.core.clone(), zip.clone());
        async move {
            match core.extract(&zip, None).await.unwrap() {
                Started::NeedsSummary(plan) => *plan,
                Started::Transfer(_) => panic!("a large extract needs the summary"),
            }
        }
    };

    let new_folder = summary().await;
    assert_eq!(new_folder.destination, uri("lautta://user-documents/bundle"));
    assert!(
        !h.root.join("Documents/bundle").exists(),
        "no folder while the summary is open, none after Cancel"
    );
    assert!(
        h.core.plan_summary(&new_folder).await.free_bytes.is_some(),
        "free space of the folder the new one goes in"
    );

    let documents = uri("lautta://user-documents/");
    h.core.list(&documents, |_| {}).await.unwrap();
    let id = h.core.start_plan(new_folder).await.unwrap();
    let s = h
        .core
        .engine
        .wait_for(id, |s| s.state.is_finished())
        .await
        .unwrap();
    assert_eq!(s.state, TransferState::Completed, "{s:?}");
    assert_eq!(
        std::fs::read(h.root.join("Documents/bundle/a.txt")).unwrap(),
        b"alpha"
    );
    let mut stale = false;
    h.core
        .list(&documents, |cached| {
            stale = !cached.iter().any(|e| e.name == b"bundle");
        })
        .await
        .unwrap();
    assert!(!stale, "cached listing of the parent shows the made folder");

    let numbered = summary().await;
    assert_eq!(numbered.destination, uri("lautta://user-documents/bundle 2"));
    assert!(!h.root.join("Documents/bundle 2").exists());
    assert_eq!(
        std::fs::read(h.root.join("Documents/bundle/b.txt")).unwrap(),
        b"beta",
        "the folder that was there stays as it was"
    );
}

#[tokio::test]
async fn extract_follows_the_macos_rule() {
    let h = home().await;
    write(&h.root.join("Documents/one.txt"), b"one");
    write(&h.root.join("Documents/two.txt"), b"two");
    let docs = uri("lautta://user-documents/");
    let pack = |sources: Vec<&'static str>, name: &'static str| {
        let (core, docs) = (h.core.clone(), docs.clone());
        async move {
            let sources: Vec<Uri> = sources.into_iter().map(uri).collect();
            core.compress_to(&sources, &docs, name, CompressOptions::new(ArchiveKind::Zip))
                .await
                .unwrap()
        }
    };
    let file_zip = pack(vec!["lautta://user-documents/one.txt"], "file").await;
    let many_zip = pack(
        vec![
            "lautta://user-documents/one.txt",
            "lautta://user-documents/two.txt",
        ],
        "many",
    )
    .await;

    finish(&h, h.core.extract(&file_zip, None).await.unwrap()).await;
    assert_eq!(
        std::fs::read(h.root.join("Documents/one 2.txt")).unwrap(),
        b"one",
        "one top file comes out as is; taken name gets a number"
    );
    assert_eq!(std::fs::read(h.root.join("Documents/one.txt")).unwrap(), b"one");

    std::fs::create_dir(h.root.join("Documents/many")).unwrap();
    finish(&h, h.core.extract(&many_zip, None).await.unwrap()).await;
    assert_eq!(
        std::fs::read(h.root.join("Documents/many 2/two.txt")).unwrap(),
        b"two",
        "several top items go in a new folder named after the archive"
    );
    assert_eq!(
        std::fs::read_dir(h.root.join("Documents/many")).unwrap().count(),
        0,
        "nothing merges into a folder that was there"
    );
}

#[tokio::test]
async fn extract_never_merges_into_a_folder_made_meanwhile() {
    let h = home().await;
    write(&h.root.join("Documents/one.txt"), b"one");
    write(&h.root.join("Documents/two.txt"), b"two");
    let mem = Arc::new(MemoryProvider::new(Capabilities::with(&[cap::WRITE])));
    h.core.locations.register(
        Location::remote(
            "nas",
            LocationKind::Server {
                provider: "sftp".into(),
            },
            "NAS",
            None,
        ),
        mem.clone(),
    );
    h.core
        .compress_to(
            &[
                uri("lautta://user-documents/one.txt"),
                uri("lautta://user-documents/two.txt"),
            ],
            &uri("lautta://user-documents/"),
            "many",
            CompressOptions::new(ArchiveKind::Zip),
        )
        .await
        .unwrap();
    let bytes = std::fs::read(h.root.join("Documents/many.zip")).unwrap();
    mem.add_file("many.zip", &bytes, 1_700_000_000_000);
    let zip = uri("lautta://nas/many.zip");
    // Not listed yet, there by the time it is made: a race, or a name differing only in case.
    mem.fail_next("make_dir", "many", Error::kind(ErrorKind::AlreadyExists));
    let err = h.core.extract(&zip, None).await.err().map(|e| e.kind);
    assert_eq!(err, Some(ErrorKind::AlreadyExists));
    assert_eq!(mem.paths(), ["many.zip"], "nothing lands in the other folder");
}

#[tokio::test]
async fn extract_needs_a_folder_it_may_add_to() {
    let h = home().await;
    write(&h.root.join("Documents/inner/x.txt"), b"x");
    let inner = h
        .core
        .compress_to(
            &[uri("lautta://user-documents/inner")],
            &uri("lautta://user-documents/"),
            "inner",
            CompressOptions::new(ArchiveKind::Zip),
        )
        .await
        .unwrap();
    let outer = h
        .core
        .compress_to(
            &[inner],
            &uri("lautta://user-documents/"),
            "outer",
            CompressOptions::new(ArchiveKind::Zip),
        )
        .await
        .unwrap();
    let (location, _hold) = h.core.open_archive(&outer, None).await.unwrap();
    let nested = Uri::root(location).join(b"inner.zip").unwrap();
    let err = h.core.extract(&nested, None).await.unwrap_err();
    assert_eq!(err.kind, ErrorKind::ReadOnlyFilesystem, "{err:?}");
}

#[tokio::test]
async fn compress_and_extract_need_a_folder_whose_permissions_allow_adding() {
    use std::os::unix::fs::PermissionsExt;
    let h = home().await;
    write(&h.root.join("Documents/locked/a.txt"), b"a");
    let locked = uri("lautta://user-documents/locked");
    let zip = h
        .core
        .compress_to(
            &[uri("lautta://user-documents/locked/a.txt")],
            &locked,
            "a",
            CompressOptions::new(ArchiveKind::Zip),
        )
        .await
        .unwrap();
    let dir = h.root.join("Documents/locked");
    std::fs::set_permissions(&dir, std::fs::Permissions::from_mode(0o555)).unwrap();
    // root passes every permission check.
    let allowed = lautta_core::sys::effective_ids().0 == 0;
    let may_add = h.core.may_add_to(&locked);
    let compressed = h
        .core
        .compress_to(
            std::slice::from_ref(&zip),
            &locked,
            "b",
            CompressOptions::new(ArchiveKind::Zip),
        )
        .await
        .map_err(|e| e.kind);
    let extracted = h.core.extract(&zip, None).await.map(|_| ()).map_err(|e| e.kind);
    std::fs::set_permissions(&dir, std::fs::Permissions::from_mode(0o755)).unwrap();
    assert!(h.core.may_add_to(&uri("lautta://user-documents/")));
    assert_eq!(may_add, allowed);
    let refused = (!allowed).then_some(ErrorKind::ReadOnlyFilesystem);
    assert_eq!(compressed.err(), refused);
    assert_eq!(extracted.err(), refused);
}

async fn locked_zip(h: &Home, password: &str) -> Uri {
    write(&h.root.join("Documents/src/a.txt"), b"alpha");
    write(&h.root.join("Documents/src/b.txt"), b"beta");
    h.core
        .compress_to(
            &[uri("lautta://user-documents/src")],
            &uri("lautta://user-documents/"),
            "locked",
            CompressOptions {
                password: Password::new(password),
                ..CompressOptions::new(ArchiveKind::Zip)
            },
        )
        .await
        .unwrap()
}

fn pw(text: &str) -> Option<Password> {
    Password::new(text)
}

/// Closing takes milliseconds; the timeout only guards against a hang.
async fn until_closed(h: &Home, id: &str, mut changed: tokio::sync::watch::Receiver<u64>) {
    let closed = async {
        while h.core.location(id).is_some() {
            changed.changed().await.unwrap();
        }
    };
    tokio::time::timeout(std::time::Duration::from_secs(10), closed)
        .await
        .expect("archive closes");
}

#[tokio::test]
async fn a_locked_archive_stays_open_while_held_and_forgets_its_password_after() {
    let h = home().await;
    let zip = locked_zip(&h, "pw").await;
    let kind = |r: Result<(String, ArchiveHold), Error>| r.err().map(|e| e.kind);
    assert_eq!(
        kind(h.core.open_archive(&zip, None).await),
        Some(ErrorKind::PasswordRequired)
    );
    assert_eq!(
        kind(h.core.open_archive(&zip, pw("PW")).await),
        Some(ErrorKind::WrongPassword)
    );
    let (id, first) = h.core.open_archive(&zip, pw("pw")).await.unwrap();
    assert!(!first.is_empty());
    let second = h.core.hold_archive(&id);
    let (again, third) = h.core.open_archive(&zip, None).await.unwrap();
    assert_eq!(again, id, "an open archive needs no password again");
    drop(first);
    drop(third);
    assert!(h.core.location(&id).is_some(), "one hold left");
    let listed = h.core.list(&Uri::root(id.clone()), |_| {}).await.unwrap();
    assert_eq!(listed.len(), 1);
    drop(second);
    assert!(h.core.location(&id).is_none(), "last hold gone: closed");
    assert_eq!(
        kind(h.core.open_archive(&zip, None).await),
        Some(ErrorKind::PasswordRequired),
        "password forgotten with it"
    );
    assert!(h.core.hold_archive(&id).is_empty(), "nothing to hold once closed");
}

#[tokio::test]
async fn a_hold_prints_and_compares_without_its_archive() {
    let h = home().await;
    let zip = locked_zip(&h, "pw").await;
    let (_, held) = h.core.open_archive(&zip, pw("pw")).await.unwrap();
    let none = ArchiveHold::default();
    assert_eq!(format!("{held:?}"), "ArchiveHold(held)");
    assert_eq!(format!("{none:?}"), "ArchiveHold(none)");
    assert_eq!(held, none, "plans compare equal whatever they hold");
}

#[tokio::test]
async fn an_unlocked_archive_stays_open_for_the_session() {
    let h = home().await;
    write(&h.root.join("Documents/src/a.txt"), b"alpha");
    let zip = h
        .core
        .compress_to(
            &[uri("lautta://user-documents/src")],
            &uri("lautta://user-documents/"),
            "plain",
            CompressOptions::new(ArchiveKind::Zip),
        )
        .await
        .unwrap();
    let (id, hold) = h.core.open_archive(&zip, pw("unused")).await.unwrap();
    assert!(hold.is_empty());
    drop(hold);
    assert!(h.core.location(&id).is_some());
}

#[tokio::test]
async fn a_locked_archive_stays_open_until_a_transfer_from_it_ends() {
    let h = home().await;
    let zip = locked_zip(&h, "pw").await;
    write(&h.root.join("Downloads/a.txt"), b"old");
    let (id, hold) = h.core.open_archive(&zip, pw("pw")).await.unwrap();
    let Started::Transfer(t) = h
        .core
        .copy_or_move(
            OperationKind::Copy,
            vec![uri(&format!("lautta://{id}/src/a.txt"))],
            uri("lautta://user-downloads/"),
        )
        .await
        .unwrap()
    else {
        panic!("small copies start at once")
    };
    h.core
        .engine
        .wait_for(t, |s| s.state == TransferState::Waiting(WaitReason::Question))
        .await
        .unwrap();
    let changed = h.core.locations.subscribe();
    drop(hold);
    assert!(h.core.location(&id).is_some(), "the waiting copy still reads it");
    h.core.answer_question(t, 0, "Replace", false).unwrap();
    let s = h
        .core
        .engine
        .wait_for(t, |s| s.state.is_finished())
        .await
        .unwrap();
    assert_eq!(s.state, TransferState::Completed);
    assert_eq!(std::fs::read(h.root.join("Downloads/a.txt")).unwrap(), b"alpha");
    until_closed(&h, &id, changed).await;
}

#[tokio::test]
async fn extract_of_a_locked_archive_takes_its_password_then_closes_it() {
    let h = home().await;
    let zip = locked_zip(&h, "pw").await;
    let kind = |r: Result<Started, Error>| r.err().map(|e| e.kind);
    assert_eq!(
        kind(h.core.extract(&zip, None).await),
        Some(ErrorKind::PasswordRequired)
    );
    assert_eq!(
        kind(h.core.extract(&zip, pw("nope")).await),
        Some(ErrorKind::WrongPassword)
    );
    let id = lautta_core::provider::archive::archive_location_id(&zip);
    let changed = h.core.locations.subscribe();
    finish(&h, h.core.extract(&zip, pw("pw")).await.unwrap()).await;
    assert_eq!(
        std::fs::read(h.root.join("Documents/src 2/b.txt")).unwrap(),
        b"beta"
    );
    until_closed(&h, &id, changed).await;
}

#[tokio::test]
async fn a_locked_extract_waiting_for_its_summary_keeps_the_archive_open() {
    let h = home().await;
    let zip = locked_zip(&h, "pw").await;
    h.core.apply_settings(Settings {
        large_op_items: 1,
        ..Settings::default()
    });
    let id = lautta_core::provider::archive::archive_location_id(&zip);
    let Started::NeedsSummary(plan) = h.core.extract(&zip, pw("pw")).await.unwrap() else {
        panic!("a large extract needs the summary");
    };
    assert!(h.core.location(&id).is_some(), "open while the summary shows");
    drop(plan);
    assert!(h.core.location(&id).is_none(), "summary dismissed: closed");

    let Started::NeedsSummary(plan) = h.core.extract(&zip, pw("pw")).await.unwrap() else {
        panic!("a large extract needs the summary");
    };
    let changed = h.core.locations.subscribe();
    finish(&h, Started::Transfer(h.core.start_plan(*plan).await.unwrap())).await;
    assert_eq!(
        std::fs::read(h.root.join("Documents/src 2/a.txt")).unwrap(),
        b"alpha"
    );
    until_closed(&h, &id, changed).await;
}
