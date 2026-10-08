// SPDX-License-Identifier: LGPL-2.1-or-later
//! The viewers' core actions end to end: text load/save with the EDT-2
//! conflict check, Markdown, EXIF, the folder's images, remote
//! thumbnails (PRV-2/3), playback copies and Recents (ORG-2).

use lautta_core::app::Core;
use lautta_core::app_viewers::{SaveMode, SaveOutcome};
use lautta_core::entry::Capabilities;
use lautta_core::locations::{Location, LocationKind, LocationRegistry};
use lautta_core::org::recents::{RecentKind, RecentsFilter};
use lautta_core::paths::AppPaths;
use lautta_core::provider::memory::MemoryProvider;
use lautta_core::{Error, ErrorKind, Uri};
use std::sync::Arc;

struct Home {
    _dir: tempfile::TempDir,
    root: std::path::PathBuf,
    core: Arc<Core>,
    nas: MemoryProvider,
}

async fn home() -> Home {
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path().to_path_buf();
    for d in ["Documents", "Pictures"] {
        std::fs::create_dir_all(root.join(d)).unwrap();
    }
    let paths = AppPaths::new(&root);
    let registry = LocationRegistry::with_media_root(paths.clone(), root.join("media"));
    let core = Core::open_with(paths, registry).await.unwrap();
    let nas = MemoryProvider::default();
    let loc = Location::remote(
        "nv-nas",
        LocationKind::Server {
            provider: "sftp".into(),
        },
        "NAS",
        Some("sftp://nas.home".into()),
    );
    core.locations.register(loc, Arc::new(nas.clone()));
    Home {
        _dir: dir,
        root,
        core,
        nas,
    }
}

fn uri(s: &str) -> Uri {
    Uri::parse(s).unwrap()
}

fn jpeg(w: u32, h: u32) -> Vec<u8> {
    let img = image::DynamicImage::ImageRgb8(image::RgbImage::from_pixel(w, h, image::Rgb([200, 30, 30])));
    let mut out = std::io::Cursor::new(Vec::new());
    img.write_to(&mut out, image::ImageOutputFormat::Jpeg(80))
        .unwrap();
    out.into_inner()
}

/// A JPEG start, an APP1 block with Make and Orientation, an end marker.
fn exif_jpeg(orientation: u16) -> Vec<u8> {
    let mut tiff = vec![b'I', b'I', 0x2a, 0, 8, 0, 0, 0, 2, 0];
    tiff.extend_from_slice(&[0x0f, 0x01, 2, 0, 6, 0, 0, 0, 38, 0, 0, 0]);
    tiff.extend_from_slice(&[0x12, 0x01, 3, 0, 1, 0, 0, 0]);
    tiff.extend_from_slice(&orientation.to_le_bytes());
    tiff.extend_from_slice(&[0, 0, 0, 0, 0, 0]);
    tiff.extend_from_slice(b"Jolla\0");
    let mut out = vec![0xff, 0xd8, 0xff, 0xe1];
    let len = u16::try_from(2 + 6 + tiff.len()).unwrap();
    out.extend_from_slice(&len.to_be_bytes());
    out.extend_from_slice(b"Exif\0\0");
    out.extend_from_slice(&tiff);
    out.extend_from_slice(&[0xff, 0xd9]);
    out
}

#[tokio::test]
async fn local_text_round_trip_keeps_line_endings() {
    let h = home().await;
    let path = h.root.join("Documents/notes.txt");
    std::fs::write(&path, b"one\r\ntwo\r\n").unwrap();
    let u = uri("lautta://user-documents/notes.txt");
    let loaded = h.core.load_text(&u).await.unwrap();
    assert_eq!(loaded.doc.text, "one\ntwo");
    assert!(loaded.editable());
    assert_eq!(loaded.size, Some(10));

    let out = h
        .core
        .save_text(
            &u,
            "one\ntwo\nthree",
            &loaded.doc.meta,
            SaveMode::Checked(loaded.stamp.clone()),
        )
        .await
        .unwrap();
    assert!(matches!(out, SaveOutcome::Saved { .. }));
    assert_eq!(std::fs::read(&path).unwrap(), b"one\r\ntwo\r\nthree\r\n");
    let names: Vec<_> = std::fs::read_dir(h.root.join("Documents"))
        .unwrap()
        .map(|e| e.unwrap().file_name())
        .collect();
    assert_eq!(names.len(), 1, "no temporary file stays: {names:?}");
    let recents = h.core.recents.list(&RecentsFilter::default()).unwrap();
    assert_eq!(recents[0].kind, RecentKind::Edited, "ORG-2");
}

#[tokio::test]
async fn local_save_keeps_the_mode() {
    use std::os::unix::fs::PermissionsExt;
    let h = home().await;
    let path = h.root.join("Documents/run.sh");
    std::fs::write(&path, b"echo\n").unwrap();
    std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o750)).unwrap();
    let u = uri("lautta://user-documents/run.sh");
    let l = h.core.load_text(&u).await.unwrap();
    h.core
        .save_text(&u, "echo hi", &l.doc.meta, SaveMode::Replace)
        .await
        .unwrap();
    let mode = std::fs::metadata(&path).unwrap().permissions().mode() & 0o777;
    assert_eq!(mode, 0o750);
}

#[tokio::test]
async fn a_changed_file_is_a_conflict_and_nothing_is_written() {
    let h = home().await;
    let path = h.root.join("Documents/a.txt");
    std::fs::write(&path, b"mine\n").unwrap();
    let u = uri("lautta://user-documents/a.txt");
    let l = h.core.load_text(&u).await.unwrap();
    std::fs::write(&path, b"someone else wrote a longer text\n").unwrap();

    let out = h
        .core
        .save_text(&u, "edited", &l.doc.meta, SaveMode::Checked(l.stamp.clone()))
        .await
        .unwrap();
    let SaveOutcome::Conflict(c) = out else {
        panic!("expected a conflict")
    };
    assert!(!c.deleted);
    assert_eq!(c.size, Some(33));
    assert_eq!(
        std::fs::read(&path).unwrap(),
        b"someone else wrote a longer text\n"
    );

    // Save mine as copy: next to the original under a free name.
    let SaveOutcome::Saved { uri: copy, .. } = h
        .core
        .save_text(&u, "edited", &l.doc.meta, SaveMode::AsCopy)
        .await
        .unwrap()
    else {
        panic!("copy not saved")
    };
    assert_eq!(copy, uri("lautta://user-documents/a%202.txt"));
    assert_eq!(
        std::fs::read(h.root.join("Documents/a 2.txt")).unwrap(),
        b"edited\n"
    );

    // Upload mine and replace.
    h.core
        .save_text(&u, "final", &l.doc.meta, SaveMode::Replace)
        .await
        .unwrap();
    assert_eq!(std::fs::read(&path).unwrap(), b"final\n");
}

#[tokio::test]
async fn a_deleted_file_is_a_conflict_too() {
    let h = home().await;
    let path = h.root.join("Documents/gone.txt");
    std::fs::write(&path, b"x").unwrap();
    let u = uri("lautta://user-documents/gone.txt");
    let l = h.core.load_text(&u).await.unwrap();
    std::fs::remove_file(&path).unwrap();
    let out = h
        .core
        .save_text(&u, "y", &l.doc.meta, SaveMode::Checked(l.stamp))
        .await
        .unwrap();
    assert!(matches!(out, SaveOutcome::Conflict(c) if c.deleted));
    assert!(!path.exists());
}

#[tokio::test]
async fn remote_text_saves_through_a_temporary_name() {
    let h = home().await;
    h.nas.add_file("docs/n.md", b"# Title\n", 1_000);
    let u = uri("lautta://nv-nas/docs/n.md");
    let l = h.core.load_text(&u).await.unwrap();
    assert!(l.writable && l.editable());
    let out = h
        .core
        .save_text(
            &u,
            "# Title\nmore",
            &l.doc.meta,
            SaveMode::Checked(l.stamp.clone()),
        )
        .await
        .unwrap();
    assert!(matches!(out, SaveOutcome::Saved { .. }));
    assert_eq!(h.nas.read_file("docs/n.md").unwrap(), b"# Title\nmore\n");
    assert!(!h.nas.exists("docs/.n.md.lautta-save"), "temporary name is gone");

    // A failing upload leaves the original untouched and no temporary file.
    h.nas.fail_next(
        "upload_from",
        "docs/.n.md.lautta-save",
        Error::new(ErrorKind::ConnectionLost, "gone"),
    );
    let l = h.core.load_text(&u).await.unwrap();
    let err = h
        .core
        .save_text(&u, "lost", &l.doc.meta, SaveMode::Checked(l.stamp))
        .await
        .unwrap_err();
    assert_eq!(err.kind, ErrorKind::ConnectionLost);
    assert_eq!(h.nas.read_file("docs/n.md").unwrap(), b"# Title\nmore\n");
    assert!(!h.nas.exists("docs/.n.md.lautta-save"));
}

#[tokio::test]
async fn read_only_locations_and_large_files_are_not_editable() {
    let h = home().await;
    let ro = MemoryProvider::new(Capabilities::default());
    ro.add_file("a.txt", b"hi", 0);
    let loc = Location::remote("nv-ro", LocationKind::AdHoc, "RO", None);
    h.core.locations.register(loc, Arc::new(ro));
    let l = h.core.load_text(&uri("lautta://nv-ro/a.txt")).await.unwrap();
    assert!(!l.writable && !l.editable());

    h.nas.add_file("big.txt", &vec![b'a'; 1024 * 1024 + 10], 0);
    let l = h.core.load_text(&uri("lautta://nv-nas/big.txt")).await.unwrap();
    assert!(l.doc.truncated, "1 MiB notice (PRV-4)");
    assert!(!l.editable());

    h.nas.add_file("bin.dat", &[0xff, 0xfe, 0x00, 0x80], 0);
    let l = h.core.load_text(&uri("lautta://nv-nas/bin.dat")).await.unwrap();
    assert!(!l.doc.valid_utf8 && !l.editable());

    let err = h.core.load_text(&uri("lautta://nv-nas/")).await.unwrap_err();
    assert_eq!(err.kind, ErrorKind::IsADirectory);
}

#[tokio::test]
async fn markdown_is_rendered_and_truncation_is_reported() {
    let h = home().await;
    h.nas.add_file("n.md", b"# Hi\n\n<script>x</script> *there*\n", 0);
    let r = h
        .core
        .render_markdown(&uri("lautta://nv-nas/n.md"))
        .await
        .unwrap();
    assert!(r.html.contains("Hi</h1>"));
    assert!(r.html.contains("&lt;script&gt;"), "raw HTML is not trusted");
    assert!(!r.truncated);
}

#[tokio::test]
async fn exif_report_reads_the_head_of_the_file() {
    let h = home().await;
    h.nas.add_file("p/a.jpg", &exif_jpeg(6), 5_000);
    h.nas.add_file("p/b.jpg", &jpeg(8, 8), 5_000);
    let r = h.core.exif_report(&uri("lautta://nv-nas/p/a.jpg")).await.unwrap();
    let info = r.info.unwrap();
    assert_eq!(info.make.as_deref(), Some("Jolla"));
    assert_eq!(info.orientation, Some(6));
    assert_eq!(r.name, "a.jpg");
    assert_eq!(r.modified_ms, Some(5_000));
    let plain = h.core.exif_report(&uri("lautta://nv-nas/p/b.jpg")).await.unwrap();
    assert!(plain.info.is_none());
}

#[tokio::test]
async fn the_folders_images_come_in_the_folders_order() {
    let h = home().await;
    for (n, d) in [
        ("b.jpg", jpeg(2, 2)),
        ("a10.png", jpeg(2, 2)),
        ("a2.png", jpeg(2, 2)),
        (".hidden.jpg", jpeg(2, 2)),
        ("notes.txt", b"x".to_vec()),
    ] {
        std::fs::write(h.root.join("Pictures").join(n), d).unwrap();
    }
    std::fs::create_dir(h.root.join("Pictures/sub.jpg")).unwrap();
    let list = h.core.list_images(&uri("lautta://user-pictures/")).await.unwrap();
    let names: Vec<_> = list.iter().map(|i| i.name.as_str()).collect();
    assert_eq!(
        names,
        ["a2.png", "a10.png", "b.jpg"],
        "natural order, no folders or hidden"
    );
    assert_eq!(list[0].uri, uri("lautta://user-pictures/a2.png"));
    assert!(list[0].size.is_some());
}

#[tokio::test]
async fn full_images_carry_their_orientation() {
    let h = home().await;
    h.nas.add_file("a.jpg", &exif_jpeg(8), 0);
    h.nas.add_file("b.jpg", &jpeg(4, 4), 0);
    let a = h.core.full_image(&uri("lautta://nv-nas/a.jpg")).await.unwrap();
    assert_eq!(a.orientation, 8);
    assert_eq!(a.bytes, exif_jpeg(8));
    let b = h.core.full_image(&uri("lautta://nv-nas/b.jpg")).await.unwrap();
    assert_eq!(b.orientation, 1);
    let err = h
        .core
        .full_image(&uri("lautta://nv-nas/none.jpg"))
        .await
        .unwrap_err();
    assert_eq!(err.kind, ErrorKind::NotFound);
}

fn open_reads(nas: &MemoryProvider) -> usize {
    nas.calls().iter().filter(|c| c.starts_with("open_read")).count()
}

#[tokio::test]
async fn remote_thumbnails_are_cached_and_shared() {
    let h = home().await;
    h.nas.add_file("p/a.jpg", &jpeg(200, 100), 9_000);
    let rt = tokio::runtime::Handle::current();
    let previews = h.core.previews(&rt).unwrap();
    let u = uri("lautta://nv-nas/p/a.jpg");

    let t1 = previews.request(h.core.clone(), u.clone(), 64);
    let t2 = previews.request(h.core.clone(), u.clone(), 64);
    let bytes = t1.rx.await.unwrap().unwrap();
    let again = t2.rx.await.unwrap().unwrap();
    assert_eq!(bytes, again, "waiters of one job all get the result");
    let img = image::load_from_memory(&bytes).unwrap();
    assert_eq!(
        (img.width(), img.height()),
        (64, 32),
        "decoded at the target size"
    );
    assert_eq!(previews.cache().len(), 1);

    let before = open_reads(&h.nas);
    let t3 = previews.request(h.core.clone(), u.clone(), 64);
    t3.rx.await.unwrap().unwrap();
    assert_eq!(
        before,
        open_reads(&h.nas),
        "second thumbnail comes from the cache (PRV-2)"
    );

    // The cache key includes the mtime: a changed file is rendered again.
    h.nas.add_file("p/a.jpg", &jpeg(100, 50), 10_000);
    let t4 = previews.request(h.core.clone(), u, 64);
    t4.rx.await.unwrap().unwrap();
    assert_eq!(previews.cache().len(), 2);
}

#[tokio::test]
async fn local_files_and_switched_off_thumbnails_are_refused() {
    let h = home().await;
    std::fs::write(h.root.join("Pictures/l.jpg"), jpeg(8, 8)).unwrap();
    let previews = h.core.previews(&tokio::runtime::Handle::current()).unwrap();
    let t = previews.request(h.core.clone(), uri("lautta://user-pictures/l.jpg"), 32);
    let err = t.rx.await.unwrap().unwrap_err();
    assert_eq!(
        err.kind,
        ErrorKind::Unsupported,
        "PRV-1: Nemo.Thumbnailer does local files"
    );

    h.nas.add_file("x.jpg", &jpeg(8, 8), 0);
    let mut s = h.core.settings();
    s.thumbnails_remote = false;
    h.core.apply_settings(s);
    let t = previews.request(h.core.clone(), uri("lautta://nv-nas/x.jpg"), 32);
    assert_eq!(t.rx.await.unwrap().unwrap_err().kind, ErrorKind::Unsupported);
}

#[tokio::test]
async fn cancelling_the_last_waiter_cancels_the_job() {
    let h = home().await;
    h.nas.add_file("x.jpg", &jpeg(8, 8), 0);
    let previews = h.core.previews(&tokio::runtime::Handle::current()).unwrap();
    // Fill both slots of the location so the next job stays queued (PRV-3).
    let gate = Arc::new(tokio::sync::Notify::new());
    for i in 0..2 {
        let g = gate.clone();
        previews
            .jobs()
            .request(&format!("block{i}"), "nv-nas", async move { g.notified().await });
    }
    let t = previews.request(h.core.clone(), uri("lautta://nv-nas/x.jpg"), 32);
    assert_eq!(previews.jobs().queued("nv-nas"), 1);
    t.cancel();
    assert_eq!(previews.jobs().queued("nv-nas"), 0, "canceled on scroll");
    gate.notify_waiters();
}

#[tokio::test]
async fn playback_copies_report_progress() {
    let h = home().await;
    h.nas.add_file("m/song.mp3", &vec![7u8; 4096], 0);
    let seen = Arc::new(std::sync::Mutex::new(Vec::new()));
    let s2 = seen.clone();
    let sink: lautta_core::provider::ProgressSink = Arc::new(move |d, t| s2.lock().unwrap().push((d, t)));
    let path = h
        .core
        .download_for_playback(&uri("lautta://nv-nas/m/song.mp3"), sink)
        .await
        .unwrap();
    assert_eq!(std::fs::read(&path).unwrap(), vec![7u8; 4096]);
    assert_eq!(seen.lock().unwrap().last(), Some(&(4096, Some(4096))));
    h.core.clear_playback_cache();
    assert!(!path.exists());

    let err = h
        .core
        .download_for_playback(
            &uri("lautta://nv-nas/m/none.mp3"),
            lautta_core::provider::no_progress(),
        )
        .await
        .unwrap_err();
    assert_eq!(err.kind, ErrorKind::NotFound);
}

#[tokio::test]
async fn viewing_is_noted_in_recents() {
    let h = home().await;
    let u = uri("lautta://nv-nas/docs/a.txt");
    h.core.note_viewed(&u, RecentKind::Previewed);
    let r = h.core.recents.list(&RecentsFilter::default()).unwrap();
    assert_eq!((r[0].name.as_str(), r[0].kind), ("a.txt", RecentKind::Previewed));
    let mut s = h.core.settings();
    s.recents_enabled = false;
    h.core.apply_settings(s);
    h.core
        .note_viewed(&uri("lautta://nv-nas/docs/b.txt"), RecentKind::Opened);
    assert_eq!(
        h.core.recents.list(&RecentsFilter::default()).unwrap().len(),
        1,
        "recents can be off"
    );
}
