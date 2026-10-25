// SPDX-License-Identifier: LGPL-2.1-or-later
//! The viewers' core actions end to end: text, Markdown, EXIF, the folder's
//! images, remote thumbnails (PRV-2/3), playback copies and Recents (ORG-2).

use lautta_core::app::Core;
use lautta_core::locations::{Location, LocationKind, LocationRegistry};
use lautta_core::org::recents::{RecentKind, RecentsFilter};
use lautta_core::paths::AppPaths;
use lautta_core::provider::memory::MemoryProvider;
use lautta_core::{ErrorKind, Uri};
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
async fn local_text_is_loaded_with_lf_breaks() {
    let h = home().await;
    std::fs::write(h.root.join("Documents/notes.txt"), b"one\r\ntwo\r\n").unwrap();
    let loaded = h
        .core
        .load_text(&uri("lautta://user-documents/notes.txt"))
        .await
        .unwrap();
    assert_eq!(loaded.doc.text, "one\ntwo");
    assert_eq!(loaded.size, Some(10));
}

#[tokio::test]
async fn large_and_binary_files_are_flagged() {
    let h = home().await;
    h.nas.add_file("big.txt", &vec![b'a'; 1024 * 1024 + 10], 0);
    let l = h.core.load_text(&uri("lautta://nv-nas/big.txt")).await.unwrap();
    assert!(l.doc.truncated, "1 MiB notice (PRV-4)");

    h.nas.add_file("bin.dat", &[0xff, 0xfe, 0x00, 0x80], 0);
    let l = h.core.load_text(&uri("lautta://nv-nas/bin.dat")).await.unwrap();
    assert!(!l.doc.valid_utf8);

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

/// The viewer's lines once the colouring is done.
async fn colours_of(
    viewed: lautta_core::app_viewers::ViewedText,
) -> lautta_core::preview::highlight::ColouredText {
    match viewed.colouring {
        Some(job) => job.finish().await.unwrap(),
        None => viewed.lines,
    }
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn the_text_viewer_gets_colours_by_name_or_first_line() {
    let h = home().await;
    h.nas
        .add_file("configure", b"#!/bin/sh\r\necho \"<img src=x>\"\r\n", 0);
    h.nas.add_file("notes.txt", b"#!/bin/sh\necho <b>\n", 0);
    let v = h.core.view_text(&uri("lautta://nv-nas/configure")).await.unwrap();
    assert_eq!((v.size, v.truncated, v.valid_utf8), (Some(31), false, true));
    let lines = colours_of(v).await;
    assert_eq!(lines.syntax(), Some("Bourne Again Shell (bash)"));
    assert_eq!((lines.line_count(), lines.coloured_lines()), (2, 2));
    let palette = lautta_core::preview::highlight::Palette::parse("string=#00ff00");
    let styled = lines.styled_row(1, 0, lines.line(1).unwrap(), &palette);
    assert!(
        styled.contains("<font color=\"#00ff00\">&quot;&lt;img src=x&gt;&quot;</font>"),
        "{styled}"
    );

    let plain = colours_of(h.core.view_text(&uri("lautta://nv-nas/notes.txt")).await.unwrap()).await;
    assert_eq!((plain.syntax(), plain.coloured_lines()), (None, 0));
    assert_eq!(plain.styled_row(1, 0, "echo <b>", &palette), "echo &lt;b&gt;");
    let err = h
        .core
        .view_text(&uri("lautta://nv-nas/none.sh"))
        .await
        .unwrap_err();
    assert_eq!(err.kind, ErrorKind::NotFound);
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_long_file_comes_plain_and_is_coloured_later() {
    let h = home().await;
    let script = "for f in *; do echo \"$f\"; done # loop\n".repeat(15_000);
    h.nas.add_file("long.sh", script.as_bytes(), 0);
    let v = h.core.view_text(&uri("lautta://nv-nas/long.sh")).await.unwrap();
    assert!(v.colouring.is_some(), "not done within the first wait");
    assert_eq!((v.lines.line_count(), v.lines.coloured_lines()), (15_000, 0));
    assert_eq!(
        v.lines.line(14_999),
        Some("for f in *; do echo \"$f\"; done # loop")
    );
    let lines = colours_of(v).await;
    assert!(lines.coloured_lines() > 0, "the first lines at least");
    assert_eq!(lines.line_count(), 15_000);
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
async fn remote_thumbnails_follow_the_size_setting() {
    let h = home().await;
    // 1.5 MB that only decode whole: no EXIF thumbnail to fall back on.
    let mut big = jpeg(64, 64);
    big.resize(1_500_000, 0);
    h.nas.add_file("big.jpg", &big, 0);
    let previews = h.core.previews(&tokio::runtime::Handle::current()).unwrap();
    let u = uri("lautta://nv-nas/big.jpg");
    let mut s = h.core.settings();
    s.thumbnail_max_remote_mb = 1;
    h.core.apply_settings(s.clone());
    let t = previews.request(h.core.clone(), u.clone(), 32);
    assert_eq!(
        t.rx.await.unwrap().unwrap_err().kind,
        ErrorKind::Unsupported,
        "larger than the 1 MB the setting allows"
    );
    s.thumbnail_max_remote_mb = 2;
    h.core.apply_settings(s);
    let t = previews.request(h.core.clone(), u, 32);
    assert!(t.rx.await.unwrap().is_ok(), "within 2 MB it is made");
}

/// A PDF of two pages: a red square in the top left corner of the first,
/// the second all blue.
fn pdf() -> Vec<u8> {
    use lautta_pdf::sample::{build, page};
    build(&[
        page(595, 842, "1 0 0 rg 0 792 50 50 re f"),
        page(595, 842, "0 0 1 rg 0 0 595 842 re f"),
    ])
}

fn thumb_pixels(bytes: &[u8]) -> image::RgbImage {
    image::load_from_memory(bytes).unwrap().to_rgb8()
}

async fn thumbnail(
    h: &Home,
    previews: &Arc<lautta_core::app_viewers::Previews>,
    u: &str,
    size: u32,
) -> Result<Vec<u8>, lautta_core::Error> {
    let t = previews.request(h.core.clone(), uri(u), size);
    t.rx.await.unwrap().map(|b| b.to_vec())
}

#[tokio::test]
async fn a_remote_pdf_thumbnail_is_its_first_page_made_once_per_version() {
    let h = home().await;
    h.nas.add_file("d/a.pdf", &pdf(), 9_000);
    let previews = h.core.previews(&tokio::runtime::Handle::current()).unwrap();
    let u = "lautta://nv-nas/d/a.pdf";

    let bytes = thumbnail(&h, &previews, u, 64).await.unwrap();
    let img = thumb_pixels(&bytes);
    assert_eq!(img.dimensions(), (64, 64), "the size asked for (PRV-14)");
    let corner = img.get_pixel(2, 2).0;
    assert!(
        corner[0] > 200 && corner[1] < 60 && corner[2] < 60,
        "page 1's red corner: {corner:?}"
    );
    let inside = img.get_pixel(40, 40).0;
    assert!(
        inside.iter().all(|c| *c > 235),
        "page 2 is blue and not drawn: {inside:?}"
    );
    assert_eq!(previews.cache().len(), 1);

    let before = open_reads(&h.nas);
    assert_eq!(thumbnail(&h, &previews, u, 64).await.unwrap(), bytes);
    assert_eq!(before, open_reads(&h.nas), "the second one comes from the cache");
    assert_eq!(previews.cache().len(), 1);

    thumbnail(&h, &previews, u, 100).await.unwrap();
    assert_eq!(previews.cache().len(), 2, "another size is another thumbnail");
    h.nas.add_file("d/a.pdf", &pdf(), 10_000);
    thumbnail(&h, &previews, u, 64).await.unwrap();
    assert_eq!(previews.cache().len(), 3, "a changed file is drawn again");
}

#[tokio::test]
async fn a_local_pdf_is_made_by_the_app_whatever_the_server_switch_says() {
    let h = home().await;
    std::fs::write(h.root.join("Documents/l.pdf"), pdf()).unwrap();
    let previews = h.core.previews(&tokio::runtime::Handle::current()).unwrap();
    let bytes = thumbnail(&h, &previews, "lautta://user-documents/l.pdf", 48)
        .await
        .unwrap();
    assert_eq!(thumb_pixels(&bytes).dimensions(), (48, 48));

    let mut s = h.core.settings();
    s.thumbnails_remote = false;
    h.core.apply_settings(s);
    thumbnail(&h, &previews, "lautta://user-documents/l.pdf", 40)
        .await
        .expect("Thumbnails from servers is for servers");
}

#[tokio::test]
async fn server_pdfs_follow_the_server_thumbnail_switch() {
    let h = home().await;
    h.nas.add_file("a.pdf", &pdf(), 1);
    std::fs::write(h.root.join("Documents/l.pdf"), pdf()).unwrap();
    let previews = h.core.previews(&tokio::runtime::Handle::current()).unwrap();
    let (remote, local) = ("lautta://nv-nas/a.pdf", "lautta://user-documents/l.pdf");

    let mut s = h.core.settings();
    s.thumbnails_remote = false;
    h.core.apply_settings(s.clone());
    assert_eq!(
        thumbnail(&h, &previews, remote, 32).await.unwrap_err().kind,
        ErrorKind::Unsupported,
        "a PDF on a server needs Thumbnails from servers"
    );
    assert_eq!(open_reads(&h.nas), 0, "refused before reading");
    assert!(previews.cache().is_empty());
    assert!(thumbnail(&h, &previews, local, 32).await.is_ok());

    s.thumbnails_remote = true;
    h.core.apply_settings(s);
    assert!(
        thumbnail(&h, &previews, remote, 32).await.is_ok(),
        "switched on: made"
    );
}

/// Moves `Documents/<name>` to the Trash and gives the id of its item and
/// the URI its thumbnail is asked by.
async fn trash_document(h: &Home, name: &str) -> (i64, String) {
    h.core
        .delete(&[uri(&format!("lautta://user-documents/{name}"))])
        .await
        .unwrap();
    let rows = h.core.trash_entries(lautta_core::trash::now_secs()).unwrap();
    let id = rows.iter().find(|r| r.name == name).unwrap().id;
    let stored = h.core.trash.get(id).unwrap().stored_name;
    (id, format!("lautta://trash/{id}/{stored}"))
}

#[tokio::test]
async fn a_trashed_pdf_gets_its_thumbnail_from_the_stored_copy() {
    let h = home().await;
    std::fs::write(h.root.join("Documents/t.pdf"), pdf()).unwrap();
    let (_, u) = trash_document(&h, "t.pdf").await;
    let previews = h.core.previews(&tokio::runtime::Handle::current()).unwrap();

    let bytes = thumbnail(&h, &previews, &u, 64).await.unwrap();
    let img = thumb_pixels(&bytes);
    assert_eq!(img.dimensions(), (64, 64));
    let corner = img.get_pixel(2, 2).0;
    assert!(
        corner[0] > 200 && corner[1] < 60 && corner[2] < 60,
        "page 1's red corner: {corner:?}"
    );
    assert_eq!(previews.cache().len(), 1);
    assert_eq!(thumbnail(&h, &previews, &u, 64).await.unwrap(), bytes);
    assert_eq!(previews.cache().len(), 1, "the second one is the cached one");

    let mut s = h.core.settings();
    s.thumbnails_remote = false;
    h.core.apply_settings(s);
    assert!(
        thumbnail(&h, &previews, &u, 48).await.is_ok(),
        "the Trash is on this device: the servers' switch is not its"
    );
}

#[tokio::test]
async fn trash_thumbnails_are_for_regular_pdfs_that_are_still_there() {
    let h = home().await;
    std::fs::write(h.root.join("Documents/t.pdf"), pdf()).unwrap();
    std::fs::write(h.root.join("Documents/p.jpg"), jpeg(8, 8)).unwrap();
    std::fs::write(h.root.join("Documents/n.txt"), b"x").unwrap();
    let (pdf_id, pdf_uri) = trash_document(&h, "t.pdf").await;
    let (_, jpg_uri) = trash_document(&h, "p.jpg").await;
    let (_, txt_uri) = trash_document(&h, "n.txt").await;
    let previews = h.core.previews(&tokio::runtime::Handle::current()).unwrap();
    for u in [&jpg_uri, &txt_uri] {
        let err = thumbnail(&h, &previews, u, 32).await.unwrap_err();
        assert_eq!(
            err.message, "no app thumbnail for this file",
            "the system does pictures"
        );
    }
    let err = thumbnail(&h, &previews, "lautta://trash/9999/x", 32)
        .await
        .unwrap_err();
    assert_eq!(err.kind, ErrorKind::NotFound, "no such item");
    let err = thumbnail(&h, &previews, "lautta://trash/abc", 32)
        .await
        .unwrap_err();
    assert_eq!(err.kind, ErrorKind::NotFound, "not a Trash item's URI");
    // The id of a gone item may be another's by now: the stored name tells.
    let err = thumbnail(&h, &previews, &format!("lautta://trash/{pdf_id}/other"), 32)
        .await
        .unwrap_err();
    assert_eq!(err.kind, ErrorKind::NotFound, "the id of another stored copy");

    // A stored copy swapped for a link is not followed.
    let item = h.core.trash.get(pdf_id).unwrap();
    std::fs::remove_file(item.stored_path()).unwrap();
    std::os::unix::fs::symlink(h.root.join("Documents"), item.stored_path()).unwrap();
    let err = thumbnail(&h, &previews, &pdf_uri, 32).await.unwrap_err();
    assert_eq!(err.kind, ErrorKind::Unsupported);
    std::fs::remove_file(item.stored_path()).unwrap();
    let err = thumbnail(&h, &previews, &pdf_uri, 32).await.unwrap_err();
    assert_eq!(err.kind, ErrorKind::NotFound, "the copy is gone");
}

#[tokio::test]
async fn pdfs_over_the_size_limit_are_refused_before_they_are_read() {
    let h = home().await;
    // Valid, with spaces after the end marker.
    let mut big = pdf();
    big.resize(1_500_000, b' ');
    h.nas.add_file("big.pdf", &big, 0);
    let previews = h.core.previews(&tokio::runtime::Handle::current()).unwrap();
    let mut s = h.core.settings();
    s.thumbnail_max_remote_mb = 1;
    h.core.apply_settings(s.clone());
    let err = thumbnail(&h, &previews, "lautta://nv-nas/big.pdf", 32)
        .await
        .unwrap_err();
    assert_eq!(err.kind, ErrorKind::Unsupported);
    assert_eq!(open_reads(&h.nas), 0, "the server limit holds PDFs too");
    s.thumbnail_max_remote_mb = 2;
    h.core.apply_settings(s);
    assert!(thumbnail(&h, &previews, "lautta://nv-nas/big.pdf", 32)
        .await
        .is_ok());

    // A local file of 20 MB and a byte: its size is enough to refuse it.
    let local = std::fs::File::create(h.root.join("Documents/huge.pdf")).unwrap();
    local
        .set_len(lautta_core::thumbs::LOCAL_PDF_MAX_BYTES + 1)
        .unwrap();
    let err = thumbnail(&h, &previews, "lautta://user-documents/huge.pdf", 32)
        .await
        .unwrap_err();
    assert_eq!(
        err.message, "no app thumbnail for this file",
        "not a PDF it tried to read"
    );
}

#[tokio::test]
async fn pdfs_that_cannot_be_drawn_are_refused_and_not_read_again() {
    use lautta_pdf::sample::{build, LOCKED_AES256};
    let h = home().await;
    h.nas.add_file("locked.pdf", LOCKED_AES256, 1);
    h.nas.add_file("broken.pdf", b"%PDF-1.4 and then nothing", 1);
    h.nas.add_file("empty.pdf", &build(&[]), 1);
    h.nas.add_file("zero.pdf", b"", 1);
    let previews = h.core.previews(&tokio::runtime::Handle::current()).unwrap();
    for (n, name) in ["locked", "broken", "empty", "zero"].into_iter().enumerate() {
        let u = format!("lautta://nv-nas/{name}.pdf");
        for _ in 0..3 {
            let err = thumbnail(&h, &previews, &u, 32).await.unwrap_err();
            assert_eq!(err.kind, ErrorKind::Unsupported, "{name}");
        }
        assert_eq!(open_reads(&h.nas), n + 1, "{name} was read once");
    }
    assert!(previews.cache().is_empty());
    let leftovers: Vec<_> = std::fs::read_dir(previews.cache().dir()).unwrap().collect();
    assert!(leftovers.is_empty(), "no markers stay behind");
}

#[tokio::test]
async fn a_pdf_request_that_is_cancelled_while_queued_never_runs() {
    let h = home().await;
    h.nas.add_file("a.pdf", &pdf(), 0);
    let previews = h.core.previews(&tokio::runtime::Handle::current()).unwrap();
    let gate = Arc::new(tokio::sync::Notify::new());
    for i in 0..2 {
        let g = gate.clone();
        previews
            .jobs()
            .request(&format!("block{i}"), "nv-nas", async move { g.notified().await });
    }
    let t = previews.request(h.core.clone(), uri("lautta://nv-nas/a.pdf"), 32);
    assert_eq!(
        previews.jobs().queued("nv-nas"),
        1,
        "PDFs wait in the same queue (PRV-3)"
    );
    t.cancel();
    assert_eq!(
        previews.jobs().queued("nv-nas"),
        0,
        "a delegate that left takes its request along"
    );
    gate.notify_waiters();
    assert_eq!(open_reads(&h.nas), 0);
}

#[tokio::test]
async fn a_smaller_thumbnail_cache_is_trimmed_at_once() {
    let h = home().await;
    let previews = h.core.previews(&tokio::runtime::Handle::current()).unwrap();
    let cache = previews.cache();
    for i in 0..12 {
        cache.put(&format!("k{i:02}"), &vec![7u8; 1_000_000]).unwrap();
    }
    assert_eq!(cache.total_bytes(), 12_000_000, "within the 200 MB default");
    let mut s = h.core.settings();
    s.thumbnail_cache_mb = 10;
    h.core.apply_settings(s);
    // The files go off the caller's thread: wait for them.
    for _ in 0..300 {
        if cache.total_bytes() <= 10_000_000 {
            break;
        }
        tokio::time::sleep(std::time::Duration::from_millis(10)).await;
    }
    assert_eq!(cache.max_bytes(), 10_000_000);
    assert_eq!(cache.total_bytes(), 10_000_000, "trimmed to the new size");
    assert!(
        !cache.contains("k00") && !cache.contains("k01") && cache.contains("k02"),
        "the oldest go first"
    );
    assert!(!cache.dir().join("k00.thumb").exists(), "from disk too");
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

fn sample_pdf() -> Vec<u8> {
    use lautta_pdf::sample::{build, page, RED_SQUARE};
    build(&[page(100, 200, RED_SQUARE), page(300, 150, "")])
}

#[tokio::test]
async fn pdfs_open_and_render_from_any_location() {
    use lautta_core::app_pdf::PageSize;
    let h = home().await;
    std::fs::write(h.root.join("Documents/local.pdf"), sample_pdf()).unwrap();
    h.nas.add_file("docs/remote.pdf", &sample_pdf(), 1);
    for u in [
        "lautta://user-documents/local.pdf",
        "lautta://nv-nas/docs/remote.pdf",
    ] {
        let opened = h.core.open_pdf(&uri(u), None).await.unwrap();
        assert_eq!(
            opened.pages,
            [
                PageSize {
                    width: 100.0,
                    height: 200.0
                },
                PageSize {
                    width: 300.0,
                    height: 150.0
                }
            ],
            "{u}"
        );
        assert_eq!(opened.size, Some(sample_pdf().len() as u64));
        let page = h.core.pdf_page(&uri(u), 0, 50).await.unwrap();
        assert_eq!((page.width, page.height), (50, 100));
        // The red square fills the bottom left quarter: 25 px of 50.
        let at = ((90 * 50 + 5) * 4) as usize;
        assert_eq!(page.rgba[at..at + 4], [255, 0, 0, 255], "{u}");
    }
    let r = h.core.recents.list(&RecentsFilter::default()).unwrap();
    assert_eq!(r.len(), 2, "opened PDFs are noted in Recents");
    assert_eq!(r[0].kind, RecentKind::Previewed);
}

#[tokio::test]
async fn a_page_is_rendered_without_opening_first_and_sees_no_stale_copy() {
    use lautta_core::app_pdf::PdfFailure;
    use lautta_pdf::sample::{build, page};
    let h = home().await;
    h.nas.add_file("docs/fresh.pdf", &sample_pdf(), 1);
    let u = uri("lautta://nv-nas/docs/fresh.pdf");
    let p = h.core.pdf_page(&u, 1, 60).await.unwrap();
    assert_eq!((p.width, p.height), (60, 30), "opened on demand");
    assert_eq!(
        h.core.pdf_page(&u, 2, 60).await.unwrap_err(),
        PdfFailure::Pdf(lautta_pdf::PdfError::NoSuchPage)
    );
    // Opening again reads the file again: the changed file is seen.
    h.nas.add_file("docs/fresh.pdf", &build(&[page(10, 10, "")]), 2);
    assert_eq!(h.core.open_pdf(&u, None).await.unwrap().pages.len(), 1);
    assert_eq!(
        h.core.pdf_page(&u, 1, 60).await.unwrap_err(),
        PdfFailure::Pdf(lautta_pdf::PdfError::NoSuchPage)
    );
}

/// What `open_pdf` of `u` fails with, as "file <kind>" or "pdf <problem>".
async fn pdf_failure(h: &Home, u: &str) -> String {
    use lautta_core::app_pdf::PdfFailure;
    match h.core.open_pdf(&uri(u), None).await.unwrap_err() {
        PdfFailure::File(e) => format!("file {:?}", e.kind),
        PdfFailure::Pdf(p) => format!("pdf {p:?}"),
    }
}

#[tokio::test]
async fn unreadable_pdfs_say_why() {
    let h = home().await;
    h.nas.add_file("docs/fake.pdf", b"%PDF-1.4 but nothing else", 1);
    h.nas.add_dir("docs/folder.pdf");
    assert_eq!(
        pdf_failure(&h, "lautta://nv-nas/docs/fake.pdf").await,
        "pdf Invalid"
    );
    assert_eq!(
        pdf_failure(&h, "lautta://nv-nas/docs/folder.pdf").await,
        "file IsADirectory"
    );
    assert_eq!(
        pdf_failure(&h, "lautta://nv-nas/docs/missing.pdf").await,
        "file NotFound"
    );
    assert_eq!(pdf_failure(&h, "lautta://nowhere/x.pdf").await, "file NotFound");
    let page = h
        .core
        .pdf_page(&uri("lautta://nv-nas/docs/missing.pdf"), 0, 10)
        .await
        .unwrap_err();
    assert!(
        matches!(page, lautta_core::app_pdf::PdfFailure::File(_)),
        "{page:?}"
    );
}

#[tokio::test]
async fn oversized_pdfs_are_refused_without_reading_them() {
    use lautta_core::app_pdf::PdfFailure;
    let h = home().await;
    let big = vec![0u8; lautta_pdf::MAX_PDF_BYTES as usize + 1];
    h.nas.add_file("docs/huge.pdf", &big, 1);
    drop(big);
    let err = h
        .core
        .open_pdf(&uri("lautta://nv-nas/docs/huge.pdf"), None)
        .await
        .unwrap_err();
    assert_eq!(err, PdfFailure::Pdf(lautta_pdf::PdfError::TooLarge));
    assert!(
        !h.nas.calls().iter().any(|c| c.starts_with("open_read")),
        "{:?}",
        h.nas.calls()
    );
}

#[tokio::test]
async fn encrypted_pdfs_open_with_their_password_only() {
    use lautta_core::app_pdf::PdfFailure;
    use lautta_pdf::sample::{LOCKED_AES256, LOCKED_PASSWORD};
    use lautta_pdf::PdfError;
    let h = home().await;
    h.nas.add_file("docs/locked.pdf", LOCKED_AES256, 1);
    let u = uri("lautta://nv-nas/docs/locked.pdf");
    for wrong in ["", "nope"] {
        assert_eq!(
            h.core.open_pdf(&u, Some(wrong)).await.unwrap_err(),
            PdfFailure::Pdf(PdfError::Encrypted),
            "{wrong:?}"
        );
    }
    assert_eq!(
        h.core
            .open_pdf(&u, Some(LOCKED_PASSWORD))
            .await
            .unwrap()
            .pages
            .len(),
        2
    );
    let p = h.core.pdf_page(&u, 0, 50).await.unwrap();
    assert_eq!((p.width, p.height), (50, 100), "the unlocked document is kept");
}

#[tokio::test]
async fn pdfs_reopen_where_reading_stopped() {
    let h = home().await;
    h.nas.add_file("docs/book.pdf", &sample_pdf(), 1);
    let u = uri("lautta://nv-nas/docs/book.pdf");
    assert_eq!(h.core.open_pdf(&u, None).await.unwrap().last_page, None);
    h.core.remember_pdf_page(&u, 1);
    assert_eq!(h.core.open_pdf(&u, None).await.unwrap().last_page, Some(1));
    h.core.remember_pdf_page(&u, 5);
    assert_eq!(
        h.core.open_pdf(&u, None).await.unwrap().last_page,
        None,
        "a page the document no longer has is ignored"
    );
    h.core.remember_pdf_page(&u, 0);
    assert_eq!(h.core.open_pdf(&u, None).await.unwrap().last_page, Some(0));
}

/// Four pages; "lautta" is on pages 0, 2 (twice, the second across two
/// lines) and 3.
fn searchable_pdf() -> Vec<u8> {
    use lautta_pdf::sample::{build, page, text};
    build(&[
        page(200, 100, &text(10, 50, "Lautta ferry")),
        page(200, 100, &text(10, 50, "nothing here")),
        page(
            200,
            100,
            &(text(10, 70, "LAUTTA and the") + &text(10, 50, "lautta")),
        ),
        page(200, 100, &text(10, 50, "one lautta")),
    ])
}

/// Every page a search of `u` sends, with its hits' area counts, and how
/// the search ended.
async fn search_pdf(
    h: &Home,
    u: &str,
    query: &str,
    from: usize,
    cancel: bool,
) -> (
    Vec<(usize, Vec<usize>)>,
    Result<(), lautta_core::app_pdf::PdfFailure>,
) {
    let (tx, mut rx) = tokio::sync::mpsc::channel(2);
    let flag = Arc::new(std::sync::atomic::AtomicBool::new(cancel));
    let core = h.core.clone();
    let (u, query) = (uri(u), query.to_owned());
    let search = tokio::spawn(async move { core.search_pdf(&u, &query, from, flag, tx).await });
    let mut pages = Vec::new();
    while let Some(p) = rx.recv().await {
        pages.push((p.page, p.hits.iter().map(|hit| hit.areas.len()).collect()));
    }
    (pages, search.await.unwrap())
}

#[tokio::test]
async fn pdfs_are_searched_page_by_page_from_the_page_being_read() {
    let h = home().await;
    h.nas.add_file("docs/ferry.pdf", &searchable_pdf(), 1);
    let u = "lautta://nv-nas/docs/ferry.pdf";
    let (pages, end) = search_pdf(&h, u, " Lautta ", 2, false).await;
    assert_eq!(end, Ok(()));
    assert_eq!(
        pages,
        [(2, vec![1, 1]), (3, vec![1]), (0, vec![1]), (1, vec![]),],
        "opened on demand, searched round from page 2"
    );
    let (pages, _) = search_pdf(&h, u, "the lautta", 0, false).await;
    assert_eq!(pages[2], (2, vec![2]), "a phrase across lines has two areas");
}

#[tokio::test]
async fn pdf_searches_stop_when_cancelled_and_blank_ones_never_start() {
    let h = home().await;
    h.nas.add_file("docs/stop.pdf", &searchable_pdf(), 1);
    let (pages, end) = search_pdf(&h, "lautta://nv-nas/docs/stop.pdf", "lautta", 0, true).await;
    assert_eq!((pages.len(), end), (0, Ok(())), "cancelled before the first page");
    for blank in ["", " \t "] {
        let (pages, end) = search_pdf(&h, "lautta://nv-nas/docs/missing.pdf", blank, 0, false).await;
        assert_eq!((pages.len(), end), (0, Ok(())), "{blank:?} reads no file");
    }
    assert!(
        h.nas.calls().iter().all(|c| !c.contains("missing")),
        "{:?}",
        h.nas.calls()
    );
}

#[tokio::test]
async fn a_pdf_search_stops_at_the_hit_limit() {
    use lautta_core::app_pdf::MAX_SEARCH_HITS;
    use lautta_pdf::sample::{build, page, text};
    use lautta_pdf::MAX_PAGE_HITS;
    let h = home().await;
    // Every page has as many hits as a page may; the pages after those that
    // fill the limit are not searched.
    let full = "x ".repeat(MAX_PAGE_HITS);
    let pages: Vec<_> = (0..MAX_SEARCH_HITS / MAX_PAGE_HITS + 2)
        .map(|_| page(200, 100, &text(0, 50, &full)))
        .collect();
    let bytes = build(&pages);
    h.nas.add_file("docs/many.pdf", &bytes, 1);
    let (pages, end) = search_pdf(&h, "lautta://nv-nas/docs/many.pdf", "x", 0, false).await;
    assert_eq!(end, Ok(()));
    let hits: usize = pages.iter().map(|(_, h)| h.len()).sum();
    assert_eq!(hits, MAX_SEARCH_HITS);
    assert_eq!(
        pages.len(),
        MAX_SEARCH_HITS / MAX_PAGE_HITS,
        "no page after the limit"
    );
}

#[tokio::test]
async fn an_encrypted_pdf_is_searched_while_it_is_kept() {
    use lautta_core::app_pdf::PdfFailure;
    use lautta_pdf::sample::{LOCKED_AES256, LOCKED_PASSWORD};
    use lautta_pdf::PdfError;
    let h = home().await;
    h.nas.add_file("docs/sealed.pdf", LOCKED_AES256, 1);
    let u = "lautta://nv-nas/docs/sealed.pdf";
    let (pages, end) = search_pdf(&h, u, "lautta", 0, false).await;
    assert_eq!((pages.len(), end), (0, Err(PdfFailure::Pdf(PdfError::Encrypted))));
    let (_, end) = search_pdf(&h, "lautta://nv-nas/docs/gone.pdf", "lautta", 0, false).await;
    assert!(matches!(end, Err(PdfFailure::File(_))), "{end:?}");
    h.core.open_pdf(&uri(u), Some(LOCKED_PASSWORD)).await.unwrap();
    let (pages, end) = search_pdf(&h, u, "lautta", 1, false).await;
    assert_eq!(end, Ok(()));
    assert_eq!(
        pages,
        [(1, vec![]), (0, vec![])],
        "both pages searched, nothing found"
    );
}

#[tokio::test]
async fn pdfs_have_their_outline_and_links() {
    use lautta_core::app_pdf::{LinkTarget, OutlineItem, PdfFailure};
    use lautta_pdf::sample::{link, page, page_ref, Doc};
    let mut doc = Doc::new(vec![page(100, 200, ""), page(100, 200, "")]);
    doc.outline(&[
        ("(Start)", 0, &format!("/Dest [{} /Fit]", page_ref(0))),
        ("(Later)", 1, &format!("/Dest [{} /Fit]", page_ref(1))),
    ])
    .page_entry(
        0,
        &format!(
            "/Annots [{} {}]",
            link([0, 100, 50, 200], &format!("/Dest [{} /Fit]", page_ref(1))),
            link([50, 0, 100, 100], "/A << /S /URI /URI (https://example.org/x) >>"),
        ),
    );
    let h = home().await;
    h.nas.add_file("docs/linked.pdf", &doc.build(), 1);
    let u = uri("lautta://nv-nas/docs/linked.pdf");
    let links = h.core.pdf_links(&u, 0).await.unwrap();
    assert_eq!(
        links.iter().map(|l| l.target.clone()).collect::<Vec<_>>(),
        [
            LinkTarget::Page { index: 1, top: None },
            LinkTarget::Uri("https://example.org/x".into())
        ],
        "opened on demand"
    );
    assert_eq!((links[1].area.x, links[1].area.y), (0.5, 0.5));
    assert!(h.core.pdf_links(&u, 1).await.unwrap().is_empty());
    assert_eq!(
        h.core.pdf_links(&u, 2).await.unwrap_err(),
        PdfFailure::Pdf(lautta_pdf::PdfError::NoSuchPage)
    );
    assert_eq!(
        h.core.open_pdf(&u, None).await.unwrap().outline,
        [
            OutlineItem {
                title: "Start".into(),
                level: 0,
                page: Some(0)
            },
            OutlineItem {
                title: "Later".into(),
                level: 1,
                page: Some(1)
            }
        ]
    );
    h.nas.add_file("docs/plain.pdf", &sample_pdf(), 1);
    let plain = h
        .core
        .open_pdf(&uri("lautta://nv-nas/docs/plain.pdf"), None)
        .await
        .unwrap();
    assert!(plain.outline.is_empty());
}
