// SPDX-License-Identifier: LGPL-2.1-or-later

use lautta_core::app::Core;
use lautta_core::locations::{Location, LocationKind, LocationRegistry};
use lautta_core::org::recents::{RecentKind, RecentsFilter};
use lautta_core::paths::AppPaths;
use lautta_core::preview::markdown::Style;
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
    assert!(l.doc.truncated, "1 MiB notice");

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
    let style = Style::parse("md-heading=#123456");
    let r = h
        .core
        .render_reading(&uri("lautta://nv-nas/n.md"), style)
        .await
        .unwrap();
    let html: String = r.blocks.iter().map(|b| b.html.as_str()).collect();
    assert!(html.contains("color: #123456; font-weight: normal"), "{html}");
    assert!(html.contains("Hi</h1>"));
    assert!(html.contains("&lt;script&gt;"), "raw HTML is not trusted");
    assert!(!r.truncated);
}

#[tokio::test]
async fn text_files_are_read_as_prose_not_markdown() {
    let h = home().await;
    h.nas.add_file("n.TXT", b"# not a heading\n  <b>kept</b>\n", 0);
    let r = h
        .core
        .render_reading(&uri("lautta://nv-nas/n.TXT"), Style::default())
        .await
        .unwrap();
    assert_eq!(r.blocks.len(), 1);
    assert_eq!(
        r.blocks[0].html,
        "<p style=\"white-space: pre-wrap; margin: 0px\"># not a heading\n  &lt;b&gt;kept&lt;/b&gt;</p>"
    );
}

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

fn camera_jpeg(orientation: u16, thumb: &[u8], image: &[u8]) -> Vec<u8> {
    let mut tiff = vec![b'I', b'I', 0x2a, 0, 8, 0, 0, 0, 1, 0];
    tiff.extend_from_slice(&[0x12, 0x01, 3, 0, 1, 0, 0, 0]);
    tiff.extend_from_slice(&orientation.to_le_bytes());
    tiff.extend_from_slice(&[0, 0, 26, 0, 0, 0]);
    tiff.extend_from_slice(&[2, 0, 0x01, 0x02, 4, 0, 1, 0, 0, 0, 56, 0, 0, 0]);
    tiff.extend_from_slice(&[0x02, 0x02, 4, 0, 1, 0, 0, 0]);
    tiff.extend_from_slice(&u32::try_from(thumb.len()).unwrap().to_le_bytes());
    tiff.extend_from_slice(&[0, 0, 0, 0]);
    tiff.extend_from_slice(thumb);
    let mut out = vec![0xff, 0xd8, 0xff, 0xe1];
    out.extend_from_slice(&u16::try_from(2 + 6 + tiff.len()).unwrap().to_be_bytes());
    out.extend_from_slice(b"Exif\0\0");
    out.extend_from_slice(&tiff);
    out.extend_from_slice(&image[2..]);
    out
}

fn dims(bytes: &[u8]) -> (u32, u32) {
    let img = image::load_from_memory(bytes).unwrap();
    (img.width(), img.height())
}

#[tokio::test]
async fn camera_thumbnails_come_upright_and_at_most_the_size_asked() {
    let h = home().await;
    let mut file = camera_jpeg(6, &jpeg(40, 30), &jpeg(8, 8));
    file.extend(vec![0u8; 2_000_000]);
    std::fs::write(h.root.join("Pictures/cam.jpg"), &file).unwrap();
    let u = uri("lautta://user-pictures/cam.jpg");
    assert_eq!(dims(&h.core.viewer_preview(&u, 1024).await.unwrap()), (30, 40));
    assert_eq!(dims(&h.core.viewer_preview(&u, 16).await.unwrap()), (12, 16));
}

fn png(w: u32, h: u32) -> Vec<u8> {
    let img = image::DynamicImage::ImageRgba8(image::RgbaImage::new(w, h));
    let mut out = std::io::Cursor::new(Vec::new());
    img.write_to(&mut out, image::ImageOutputFormat::Png).unwrap();
    out.into_inner()
}

fn exif_photo(orientation: u16, image: &[u8]) -> Vec<u8> {
    let mut out = exif_jpeg(orientation);
    out.truncate(out.len() - 2);
    out.extend_from_slice(&image[2..]);
    out
}

fn with_segments(image: &[u8], segments: &[u8]) -> Vec<u8> {
    let mut out = image[..2].to_vec();
    out.extend_from_slice(segments);
    out.extend_from_slice(&image[2..]);
    out
}

fn clear_dims(bytes: &[u8]) -> (u32, u32) {
    let img = image::load_from_memory(bytes).unwrap().to_rgba8();
    assert!(img.pixels().all(|p| p.0[3] == 0), "not clear");
    img.dimensions()
}

#[tokio::test]
async fn without_a_camera_thumbnail_the_preview_is_a_clear_image_of_the_upright_shape() {
    let h = home().await;
    let pics = h.root.join("Pictures");
    let mut app2 = Vec::new();
    for _ in 0..2 {
        app2.extend_from_slice(&[0xff, 0xe2, 0xff, 0xff]);
        app2.extend(vec![0u8; 0xfffd]);
    }
    let files = [
        ("plain.jpg", jpeg(64, 48), 1024, (64, 48)),
        ("small.jpg", jpeg(64, 48), 16, (16, 12)),
        ("wide.jpg", jpeg(1000, 499), 1024, (256, 128)),
        ("line.jpg", jpeg(1000, 3), 1024, (256, 1)),
        ("turned.jpg", exif_photo(6, &jpeg(40, 30)), 1024, (30, 40)),
        ("transposed.jpg", exif_photo(5, &jpeg(40, 30)), 1024, (30, 40)),
        ("upright.jpg", exif_photo(1, &jpeg(40, 30)), 1024, (40, 30)),
        ("icc.jpg", with_segments(&jpeg(40, 30), &app2), 1024, (40, 30)),
        ("fill.jpg", with_segments(&jpeg(40, 30), &[0xff]), 1024, (40, 30)),
        (
            "huffman-first.jpg",
            with_segments(&jpeg(40, 30), &[0xff, 0xc4, 0, 7, 1, 2, 3, 4, 5]),
            1024,
            (40, 30),
        ),
        ("tall.png", png(30, 60), 1024, (30, 60)),
    ];
    for (name, bytes, size, want) in files {
        std::fs::write(pics.join(name), bytes).unwrap();
        let u = uri(&format!("lautta://user-pictures/{name}"));
        let got = clear_dims(&h.core.viewer_preview(&u, size).await.unwrap());
        assert_eq!(got, want, "{name}");
    }
}

#[tokio::test]
async fn images_of_unknown_size_have_no_preview() {
    let h = home().await;
    let pics = h.root.join("Pictures");
    let scan_first = [
        0xff, 0xd8, 0xff, 0xda, 0, 8, 1, 2, 3, 4, 5, 6, 0xff, 0xc0, 0, 17, 8, 0, 30, 0, 40, 3, 1, 0x22, 0, 2,
        0x11, 1, 3, 0x11, 1, 0xff, 0xd9,
    ];
    let cut = [0xff, 0xd8, 0xff, 0xe0, 0, 16, 0x4a, 0x46];
    let files: [(&str, &[u8]); 4] = [
        ("exif.jpg", &exif_jpeg(6)),
        ("scan.jpg", &scan_first),
        ("cut.jpg", &cut),
        ("notes.jpg", b"not an image at all"),
    ];
    for (name, bytes) in files {
        std::fs::write(pics.join(name), bytes).unwrap();
        let u = uri(&format!("lautta://user-pictures/{name}"));
        let err = h.core.viewer_preview(&u, 1024).await.unwrap_err();
        assert_eq!(err.kind, ErrorKind::Unsupported, "{name}");
    }
    let err = h
        .core
        .viewer_preview(&uri("lautta://user-pictures/none.jpg"), 1024)
        .await
        .unwrap_err();
    assert_eq!(err.kind, ErrorKind::NotFound);
}

#[tokio::test]
async fn full_images_carry_their_orientation() {
    let h = home().await;
    h.nas.add_file("a.jpg", &exif_jpeg(8), 0);
    h.nas.add_file("b.jpg", &jpeg(4, 4), 0);
    let a = h.core.full_image(&uri("lautta://nv-nas/a.jpg")).await.unwrap();
    assert_eq!(a.orientation, 8);
    assert_eq!(*a.bytes, exif_jpeg(8));
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

fn block_queue(jobs: &lautta_core::thumbs::ThumbJobs) -> Arc<tokio::sync::Semaphore> {
    let gate = Arc::new(tokio::sync::Semaphore::new(0));
    for i in 0..2 {
        let g = gate.clone();
        jobs.request(&format!("block{i}"), "nv-nas", async move {
            let _ = g.acquire().await;
        });
    }
    gate
}

async fn queue_drained(jobs: &lautta_core::thumbs::ThumbJobs) {
    tokio::time::timeout(std::time::Duration::from_secs(30), async {
        while jobs.queued("nv-nas") + jobs.running("nv-nas") > 0 {
            tokio::time::sleep(std::time::Duration::from_millis(10)).await;
        }
    })
    .await
    .expect("queue drained within 30 s");
}

#[tokio::test]
async fn remote_thumbnails_are_cached_and_shared() {
    let h = home().await;
    h.nas.add_file("p/a.jpg", &jpeg(200, 100), 9_000);
    let rt = tokio::runtime::Handle::current();
    let previews = h.core.previews(&rt).unwrap();
    let u = uri("lautta://nv-nas/p/a.jpg");

    let t1 = previews.request(h.core.clone(), u.clone(), 64);
    let t2 = previews.request(h.core.clone(), u.clone(), 100);
    let bytes = t1.rx.await.unwrap().unwrap();
    let again = t2.rx.await.unwrap().unwrap();
    assert_eq!(
        bytes, again,
        "one size covers both: waiters of one job all get the result"
    );
    let img = image::load_from_memory(&bytes).unwrap();
    assert_eq!(
        (img.width(), img.height()),
        (128, 64),
        "made at the smallest size covering the request"
    );
    assert_eq!(previews.cache().len(), 1);

    let before = open_reads(&h.nas);
    let t3 = previews.request(h.core.clone(), u.clone(), 64);
    t3.rx.await.unwrap().unwrap();
    assert_eq!(
        before,
        open_reads(&h.nas),
        "second thumbnail comes from the cache"
    );

    h.nas.add_file("p/a.jpg", &jpeg(100, 50), 10_000);
    let t4 = previews.request(h.core.clone(), u, 64);
    t4.rx.await.unwrap().unwrap();
    assert_eq!(previews.cache().len(), 2);
}

#[tokio::test]
async fn a_thumbnail_asked_again_right_after_a_cancel_is_still_made() {
    let h = home().await;
    h.nas.add_file("p/a.jpg", &jpeg(1000, 500), 9_000);
    let previews = h.core.previews(&tokio::runtime::Handle::current()).unwrap();
    let u = uri("lautta://nv-nas/p/a.jpg");
    previews.request(h.core.clone(), u.clone(), 300).cancel();
    let again = previews.request(h.core.clone(), u, 400);
    let bytes = again.rx.await.unwrap().expect("made, not canceled");
    let img = image::load_from_memory(&bytes).unwrap();
    assert_eq!((img.width(), img.height()), (512, 256));
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
        "Nemo.Thumbnailer does local files"
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
    assert_eq!(img.dimensions(), (128, 128), "smallest size covering the request");
    let corner = img.get_pixel(2, 2).0;
    assert!(
        corner[0] > 200 && corner[1] < 60 && corner[2] < 60,
        "page 1's red corner: {corner:?}"
    );
    let inside = img.get_pixel(80, 80).0;
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
    assert_eq!(previews.cache().len(), 1, "same size covers it");
    thumbnail(&h, &previews, u, 512).await.unwrap();
    assert_eq!(previews.cache().len(), 2, "another size is another thumbnail");
    h.nas.add_file("d/a.pdf", &pdf(), 10_000);
    thumbnail(&h, &previews, u, 64).await.unwrap();
    assert_eq!(previews.cache().len(), 3, "a changed file is drawn again");
}

#[tokio::test]
async fn local_pdfs_are_left_to_the_system_thumbnailer() {
    let h = home().await;
    std::fs::write(h.root.join("Documents/l.pdf"), pdf()).unwrap();
    let previews = h.core.previews(&tokio::runtime::Handle::current()).unwrap();
    let err = thumbnail(&h, &previews, "lautta://user-documents/l.pdf", 48)
        .await
        .unwrap_err();
    assert_eq!(
        err.message, "no app thumbnail for this file",
        "Nemo.Thumbnailer draws it, in a process of its own"
    );
    assert!(previews.cache().is_empty());
}

#[tokio::test]
async fn server_pdfs_follow_the_server_thumbnail_switch() {
    let h = home().await;
    h.nas.add_file("a.pdf", &pdf(), 1);
    let previews = h.core.previews(&tokio::runtime::Handle::current()).unwrap();
    let remote = "lautta://nv-nas/a.pdf";

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

    s.thumbnails_remote = true;
    h.core.apply_settings(s);
    assert!(
        thumbnail(&h, &previews, remote, 32).await.is_ok(),
        "switched on: made"
    );
}

#[tokio::test]
async fn pdfs_over_the_size_limit_are_refused_before_they_are_read() {
    let h = home().await;
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

    h.nas.add_file("broken.pdf", &pdf(), 2);
    thumbnail(&h, &previews, "lautta://nv-nas/broken.pdf", 32)
        .await
        .expect("the file was replaced");
}

#[tokio::test]
async fn a_pdf_request_that_is_cancelled_while_queued_never_runs() {
    let h = home().await;
    h.nas.add_file("a.pdf", &pdf(), 0);
    let previews = h.core.previews(&tokio::runtime::Handle::current()).unwrap();
    let gate = block_queue(previews.jobs());
    let t = previews.request(h.core.clone(), uri("lautta://nv-nas/a.pdf"), 32);
    assert_eq!(previews.jobs().queued("nv-nas"), 1, "PDFs wait in the same queue");
    t.cancel();
    assert_eq!(
        previews.jobs().queued("nv-nas"),
        0,
        "a delegate that left takes its request along"
    );
    gate.add_permits(2);
    queue_drained(previews.jobs()).await;
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
    for _ in 0..3000 {
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
    let gate = block_queue(previews.jobs());
    let t = previews.request(h.core.clone(), uri("lautta://nv-nas/x.jpg"), 32);
    assert_eq!(previews.jobs().queued("nv-nas"), 1);
    t.cancel();
    assert_eq!(previews.jobs().queued("nv-nas"), 0, "canceled on scroll");
    gate.add_permits(2);
    queue_drained(previews.jobs()).await;
    assert_eq!(open_reads(&h.nas), 0, "never read");
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
    h.nas.add_file("docs/fresh.pdf", &build(&[page(10, 10, "")]), 2);
    assert_eq!(h.core.open_pdf(&u, None).await.unwrap().pages.len(), 1);
    assert_eq!(
        h.core.pdf_page(&u, 1, 60).await.unwrap_err(),
        PdfFailure::Pdf(lautta_pdf::PdfError::NoSuchPage)
    );
}

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

async fn text_thumb(h: &Home, previews: &Arc<lautta_core::app_viewers::Previews>, u: &str) -> Option<String> {
    previews.request_text(h.core.clone(), uri(u)).rx.await.unwrap()
}

#[tokio::test]
async fn a_text_thumbnail_is_the_head_of_the_file() {
    let h = home().await;
    let long = format!("# Packing\n\n{}", "- socks\n".repeat(200));
    std::fs::write(h.root.join("Documents/list.md"), &long).unwrap();
    let previews = h.core.previews(&tokio::runtime::Handle::current()).unwrap();
    let text = text_thumb(&h, &previews, "lautta://user-documents/list.md")
        .await
        .unwrap();
    assert!(text.starts_with("# Packing\n\n- socks\n"), "{text}");
    assert_eq!(text.len(), 512, "512 bytes read");
    assert!(text.ends_with("\n- soc"), "cut where the read ends");
    assert_eq!(previews.cache().len(), 0, "local heads are read again, not kept");

    std::fs::write(h.root.join("Documents/blob.txt"), b"PK\x03\x04\0\0").unwrap();
    std::fs::write(h.root.join("Documents/empty.txt"), b"").unwrap();
    std::fs::write(h.root.join("Documents/a.jpg"), jpeg(8, 8)).unwrap();
    for name in ["blob.txt", "empty.txt", "a.jpg", "gone.txt"] {
        let u = format!("lautta://user-documents/{name}");
        assert_eq!(text_thumb(&h, &previews, &u).await, None, "{name}");
    }
}

#[tokio::test]
async fn a_server_text_thumbnail_is_cached_per_version() {
    let h = home().await;
    h.nas.add_file("n/notes.txt", b"Ferry at 06:00\n", 9_000);
    let previews = h.core.previews(&tokio::runtime::Handle::current()).unwrap();
    let u = "lautta://nv-nas/n/notes.txt";
    assert_eq!(
        text_thumb(&h, &previews, u).await.as_deref(),
        Some("Ferry at 06:00")
    );
    assert_eq!(previews.cache().len(), 1);
    let before = open_reads(&h.nas);
    assert_eq!(
        text_thumb(&h, &previews, u).await.as_deref(),
        Some("Ferry at 06:00")
    );
    assert_eq!(open_reads(&h.nas), before, "second one from the cache");

    h.nas.add_file("n/notes.txt", b"Ferry at 07:00\n", 10_000);
    assert_eq!(
        text_thumb(&h, &previews, u).await.as_deref(),
        Some("Ferry at 07:00")
    );

    let mut s = h.core.settings();
    s.thumbnails_remote = false;
    h.core.apply_settings(s);
    assert_eq!(
        text_thumb(&h, &previews, u).await,
        None,
        "Thumbnails from servers off"
    );
}

#[tokio::test]
async fn a_text_thumbnail_cancelled_while_queued_never_runs() {
    let h = home().await;
    h.nas.add_file("a.txt", b"hei", 0);
    let previews = h.core.previews(&tokio::runtime::Handle::current()).unwrap();
    let gate = Arc::new(tokio::sync::Semaphore::new(0));
    for i in 0..2 {
        let g = gate.clone();
        previews
            .jobs()
            .request(&format!("block{i}"), "nv-nas", async move {
                let _ = g.acquire().await;
            });
    }
    let first = previews.request_text(h.core.clone(), uri("lautta://nv-nas/a.txt"));
    let second = previews.request_text(h.core.clone(), uri("lautta://nv-nas/a.txt"));
    assert_eq!(
        previews.jobs().queued("nv-nas"),
        2,
        "text waits in the same queue"
    );
    previews.cancel_text(&first.key);
    assert_eq!(previews.jobs().queued("nv-nas"), 1, "the other request stays");
    gate.add_permits(2);
    let second = second.rx.await.unwrap();
    let first = first.rx.await;
    assert_eq!(second.as_deref(), Some("hei"));
    assert!(first.is_err(), "a cancelled request never answers");
    assert_eq!(open_reads(&h.nas), 1);
}

fn sized(bytes: &[u8]) -> (u32, u32) {
    let img = image::load_from_memory(bytes).unwrap();
    (img.width(), img.height())
}

#[tokio::test]
async fn a_smaller_thumbnail_is_scaled_from_a_cached_larger_one() {
    let h = home().await;
    h.nas.add_file("p/a.jpg", &jpeg(1200, 600), 9_000);
    let previews = h.core.previews(&tokio::runtime::Handle::current()).unwrap();
    let u = "lautta://nv-nas/p/a.jpg";
    assert_eq!(
        sized(&thumbnail(&h, &previews, u, 360).await.unwrap()),
        (512, 256)
    );
    let before = open_reads(&h.nas);
    assert_eq!(sized(&thumbnail(&h, &previews, u, 112).await.unwrap()), (128, 64));
    assert_eq!(before, open_reads(&h.nas), "file not read again");
    assert_eq!(previews.cache().len(), 2, "scaled one kept too");
    assert_eq!(
        sized(&thumbnail(&h, &previews, u, 700).await.unwrap()),
        (1024, 512)
    );
    assert_eq!(before + 1, open_reads(&h.nas), "larger one needs the file");
}

#[tokio::test]
async fn a_cache_only_request_never_reads_the_file() {
    let h = home().await;
    h.nas.add_file("p/a.jpg", &jpeg(1200, 600), 9_000);
    let previews = h.core.previews(&tokio::runtime::Handle::current()).unwrap();
    let u = uri("lautta://nv-nas/p/a.jpg");
    let cached = |size| {
        let t = previews.request_cached(h.core.clone(), u.clone(), size);
        async move { t.rx.await.unwrap().map(|b| sized(&b)) }
    };
    assert_eq!(cached(1024).await.unwrap_err().kind, ErrorKind::NotFound);
    assert_eq!(open_reads(&h.nas), 0, "none made yet: nothing read");
    thumbnail(&h, &previews, "lautta://nv-nas/p/a.jpg", 200)
        .await
        .unwrap();
    let before = open_reads(&h.nas);
    assert_eq!(cached(1024).await.unwrap(), (256, 128), "largest there is");
    assert_eq!(cached(128).await.unwrap(), (128, 64), "scaled from a larger one");
    assert_eq!(before, open_reads(&h.nas));
    h.nas.add_file("p/a.jpg", &jpeg(1200, 600), 10_000);
    assert_eq!(
        cached(1024).await.unwrap_err().kind,
        ErrorKind::NotFound,
        "changed file: old thumbnail is not it"
    );
}

#[tokio::test]
async fn battery_saving_reads_no_whole_server_file() {
    let h = home().await;
    h.nas.add_file("a.jpg", &jpeg(300, 300), 0);
    h.nas.add_file("d.pdf", &pdf(), 0);
    let previews = h.core.previews(&tokio::runtime::Handle::current()).unwrap();
    h.core.set_power_save(true);
    let err = thumbnail(&h, &previews, "lautta://nv-nas/a.jpg", 64)
        .await
        .unwrap_err();
    assert_eq!(
        err.kind,
        ErrorKind::Unsupported,
        "no EXIF thumbnail, whole file not read"
    );
    let before = open_reads(&h.nas);
    let err = thumbnail(&h, &previews, "lautta://nv-nas/d.pdf", 64)
        .await
        .unwrap_err();
    assert_eq!(err.kind, ErrorKind::Unsupported);
    assert_eq!(before, open_reads(&h.nas), "PDF not even opened");
    h.core.set_power_save(false);
    thumbnail(&h, &previews, "lautta://nv-nas/a.jpg", 64)
        .await
        .unwrap();
    thumbnail(&h, &previews, "lautta://nv-nas/d.pdf", 64)
        .await
        .unwrap();
    h.core.set_power_save(true);
    thumbnail(&h, &previews, "lautta://nv-nas/a.jpg", 64)
        .await
        .expect("cached ones still show");
}

#[tokio::test]
async fn a_shown_image_is_not_read_again_until_it_changes_or_the_viewer_closes() {
    let h = home().await;
    let (a, b) = (jpeg(40, 40), jpeg(41, 40));
    h.nas.add_file("a.jpg", &a, 0);
    let u = uri("lautta://nv-nas/a.jpg");
    assert_eq!(*h.core.full_image(&u).await.unwrap().bytes, a);
    let mut same_size = a.clone();
    same_size[a.len() - 3] ^= 0xff;
    h.nas.add_file("a.jpg", &same_size, 1);
    assert_eq!(
        *h.core.full_image(&u).await.unwrap().bytes,
        a,
        "kept copy, not read again"
    );
    h.core.forget_shown_images();
    assert_eq!(
        *h.core.full_image(&u).await.unwrap().bytes,
        same_size,
        "viewer closed"
    );
    h.nas.add_file("a.jpg", &b, 2);
    assert_eq!(
        *h.core.full_image(&u).await.unwrap().bytes,
        b,
        "other size: read again"
    );
}

#[tokio::test]
async fn only_the_last_few_shown_images_are_kept() {
    use lautta_core::app_viewers::SHOWN_IMAGES;
    let h = home().await;
    let n = SHOWN_IMAGES + 1;
    for i in 0..n {
        h.nas.add_file(&format!("{i}.jpg"), &jpeg(8, 8), 0);
        h.core
            .full_image(&uri(&format!("lautta://nv-nas/{i}.jpg")))
            .await
            .unwrap();
    }
    assert_eq!(h.core.shown_images().len(), SHOWN_IMAGES);
    let first = uri("lautta://nv-nas/0.jpg");
    let mut changed = jpeg(8, 8);
    let last = changed.len() - 3;
    changed[last] ^= 0xff;
    h.nas.add_file("0.jpg", &changed, 1);
    assert_eq!(
        *h.core.full_image(&first).await.unwrap().bytes,
        changed,
        "oldest went"
    );
    let newest = uri(&format!("lautta://nv-nas/{}.jpg", n - 1));
    h.nas.add_file(&format!("{}.jpg", n - 1), &changed, 1);
    assert_eq!(
        *h.core.full_image(&newest).await.unwrap().bytes,
        jpeg(8, 8),
        "newest kept"
    );
}

#[tokio::test]
async fn sizes_and_cache_only_requests_are_jobs_of_their_own() {
    let h = home().await;
    h.nas.add_file("p/a.jpg", &jpeg(1200, 600), 9_000);
    let previews = h.core.previews(&tokio::runtime::Handle::current()).unwrap();
    let u = uri("lautta://nv-nas/p/a.jpg");
    let gate = block_queue(previews.jobs());
    let small = previews.request(h.core.clone(), u.clone(), 128);
    let large = previews.request(h.core.clone(), u.clone(), 512);
    let cached = previews.request_cached(h.core.clone(), u, 128);
    gate.add_permits(2);
    assert_eq!(
        cached.rx.await.unwrap().unwrap_err().kind,
        ErrorKind::NotFound,
        "cache-only request does not wait for the one that reads"
    );
    assert_eq!(sized(&small.rx.await.unwrap().unwrap()), (128, 64));
    assert_eq!(sized(&large.rx.await.unwrap().unwrap()), (512, 256));
}

#[tokio::test]
async fn a_listed_file_s_cached_thumbnail_needs_no_server_call() {
    use lautta_core::provider::{Lane, Provider};
    let h = home().await;
    h.nas.add_file("p/a.jpg", &jpeg(1200, 600), 9_000);
    let previews = h.core.previews(&tokio::runtime::Handle::current()).unwrap();
    thumbnail(&h, &previews, "lautta://nv-nas/p/a.jpg", 200)
        .await
        .unwrap();
    let path = uri("lautta://nv-nas/p/a.jpg").path;
    let entry = h.nas.stat(&path, true, Lane::Interactive).await.unwrap();
    h.core.dircache.put(&uri("lautta://nv-nas/p"), &[entry]).unwrap();
    let calls = h.nas.calls().len();
    let t = previews.request_cached(h.core.clone(), uri("lautta://nv-nas/p/a.jpg"), 1024);
    assert_eq!(sized(&t.rx.await.unwrap().unwrap()), (256, 128));
    assert_eq!(h.nas.calls().len(), calls, "listing says which version is cached");

    h.nas.add_symlink("p/l.jpg", "a.jpg");
    thumbnail(&h, &previews, "lautta://nv-nas/p/l.jpg", 200)
        .await
        .unwrap();
    let link = h
        .nas
        .stat(&uri("lautta://nv-nas/p/l.jpg").path, false, Lane::Interactive)
        .await
        .unwrap();
    h.core.dircache.put(&uri("lautta://nv-nas/p"), &[link]).unwrap();
    let t = previews.request_cached(h.core.clone(), uri("lautta://nv-nas/p/l.jpg"), 1024);
    assert_eq!(
        sized(&t.rx.await.unwrap().unwrap()),
        (256, 128),
        "a listed link says nothing of its target: asked"
    );
}

#[path = "fixtures/audio.rs"]
#[allow(dead_code)]
mod audio_fixture;

use lautta_core::app_viewers::{CoverFrom, PreviewKind};

fn red() -> [u8; 3] {
    [220, 20, 20]
}

fn blue() -> [u8; 3] {
    [20, 20, 220]
}

fn tagged(cover: Option<[u8; 3]>) -> Vec<u8> {
    let art = cover.map(|c| audio_fixture::jpeg(300, c));
    audio_fixture::mp3(
        &[
            ("TIT2", "Harbour Lights"),
            ("TPE1", "The Ferry Pilots"),
            ("TRCK", "3/11"),
        ],
        art.as_deref(),
        8,
    )
}

fn tint(bytes: &[u8]) -> [u8; 3] {
    let img = thumb_pixels(bytes);
    let n = u64::from(img.width() * img.height());
    let mut sum = [0u64; 3];
    for p in img.pixels() {
        for (s, c) in sum.iter_mut().zip(p.0) {
            *s += u64::from(c);
        }
    }
    sum.map(|s| (s / n) as u8)
}

fn near(a: [u8; 3], b: [u8; 3]) -> bool {
    a.iter().zip(b).all(|(x, y)| x.abs_diff(y) < 40)
}

fn reads_of(nas: &MemoryProvider, path: &str) -> usize {
    nas.calls()
        .iter()
        .filter(|c| c.starts_with("open_read") && c.ends_with(path))
        .count()
}

async fn cover(
    h: &Home,
    previews: &Arc<lautta_core::app_viewers::Previews>,
    u: &str,
    kind: PreviewKind,
) -> Result<Vec<u8>, lautta_core::Error> {
    let t = previews.request_cover(h.core.clone(), uri(u), 512, kind);
    t.rx.await.unwrap().map(|b| b.to_vec())
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn audio_info_has_the_tags_and_where_the_cover_comes_from() {
    let h = home().await;
    h.nas.add_file("m/own.mp3", &tagged(Some(red())), 1);
    h.nas.add_file("m/bare.mp3", &audio_fixture::mp3(&[], None, 8), 1);
    h.nas.add_file("m/noise.mp3", b"not audio at all", 1);
    h.nas
        .add_file("lone/bare.mp3", &audio_fixture::mp3(&[], None, 8), 1);

    let info = h
        .core
        .audio_info(&uri("lautta://nv-nas/m/own.mp3"))
        .await
        .unwrap();
    assert_eq!(info.tags.title.as_deref(), Some("Harbour Lights"));
    assert_eq!(info.tags.artist.as_deref(), Some("The Ferry Pilots"));
    assert_eq!((info.tags.track, info.tags.track_total), (Some(3), Some(11)));
    assert_eq!(info.cover, CoverFrom::File);

    let bare = h
        .core
        .audio_info(&uri("lautta://nv-nas/m/bare.mp3"))
        .await
        .unwrap();
    assert_eq!(bare.cover, CoverFrom::None, "no picture in the folder yet");
    h.nas
        .add_file("m/Folder.JPG", &audio_fixture::jpeg(40, blue()), 1);
    let bare = h
        .core
        .audio_info(&uri("lautta://nv-nas/m/bare.mp3"))
        .await
        .unwrap();
    assert_eq!(bare.cover, CoverFrom::Folder);
    assert_eq!(bare.tags, Default::default());

    let noise = h
        .core
        .audio_info(&uri("lautta://nv-nas/m/noise.mp3"))
        .await
        .unwrap();
    assert_eq!(noise.tags, Default::default(), "unreadable: empty tags, no error");
    let lone = h
        .core
        .audio_info(&uri("lautta://nv-nas/lone/bare.mp3"))
        .await
        .unwrap();
    assert_eq!(lone.cover, CoverFrom::None);
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn the_player_cover_needs_no_second_read_after_audio_info() {
    let h = home().await;
    h.nas.add_file("m/own.mp3", &tagged(Some(red())), 1);
    let previews = h.core.previews(&tokio::runtime::Handle::current()).unwrap();
    h.core
        .audio_info(&uri("lautta://nv-nas/m/own.mp3"))
        .await
        .unwrap();
    let before = reads_of(&h.nas, "m/own.mp3");
    let big = cover(&h, &previews, "lautta://nv-nas/m/own.mp3", PreviewKind::Cover)
        .await
        .unwrap();
    assert!(near(tint(&big), red()));
    assert_eq!(thumb_pixels(&big).width(), 300, "never scaled up");
    assert_eq!(
        reads_of(&h.nas, "m/own.mp3"),
        before,
        "cover from what audio_info read"
    );
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn audio_thumbnails_take_the_files_cover_else_the_folders() {
    let h = home().await;
    h.nas.add_file("m/own.mp3", &tagged(Some(red())), 1);
    h.nas.add_file("m/a.mp3", &tagged(None), 1);
    h.nas
        .add_file("m/b.flac", &audio_fixture::flac(&["TITLE=b"], None), 1);
    h.nas
        .add_file("m/cover.jpg", &audio_fixture::jpeg(300, blue()), 1);
    let previews = h.core.previews(&tokio::runtime::Handle::current()).unwrap();

    let own = thumbnail(&h, &previews, "lautta://nv-nas/m/own.mp3", 128)
        .await
        .unwrap();
    assert!(
        near(tint(&own), red()),
        "file's own cover first: {:?}",
        tint(&own)
    );
    assert_eq!(thumb_pixels(&own).width(), 128);

    let a = thumbnail(&h, &previews, "lautta://nv-nas/m/a.mp3", 128)
        .await
        .unwrap();
    let b = thumbnail(&h, &previews, "lautta://nv-nas/m/b.flac", 128)
        .await
        .unwrap();
    assert!(
        near(tint(&a), blue()) && near(tint(&b), blue()),
        "folder's cover.jpg"
    );
    assert_eq!(
        reads_of(&h.nas, "m/cover.jpg"),
        1,
        "folder picture made once for all tracks"
    );

    let (a_reads, cover_reads) = (reads_of(&h.nas, "m/a.mp3"), reads_of(&h.nas, "m/cover.jpg"));
    thumbnail(&h, &previews, "lautta://nv-nas/m/a.mp3", 256)
        .await
        .unwrap();
    assert_eq!(
        reads_of(&h.nas, "m/a.mp3"),
        a_reads,
        "known to have no cover: not read again"
    );
    assert_eq!(
        reads_of(&h.nas, "m/cover.jpg"),
        cover_reads + 1,
        "new size of the folder picture"
    );

    h.nas.add_file("m/cover.jpg", &audio_fixture::jpeg(300, red()), 2);
    h.core.dircache.invalidate(&uri("lautta://nv-nas/m")).unwrap();
    let a = thumbnail(&h, &previews, "lautta://nv-nas/m/a.mp3", 256)
        .await
        .unwrap();
    assert!(near(tint(&a), red()), "changed folder picture shows at once");
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn audio_without_any_cover_is_read_once() {
    let h = home().await;
    h.nas.add_file("m/a.mp3", &tagged(None), 1);
    let previews = h.core.previews(&tokio::runtime::Handle::current()).unwrap();
    let err = thumbnail(&h, &previews, "lautta://nv-nas/m/a.mp3", 128)
        .await
        .unwrap_err();
    assert_eq!(err.kind, ErrorKind::NotFound);
    let reads = reads_of(&h.nas, "m/a.mp3");
    thumbnail(&h, &previews, "lautta://nv-nas/m/a.mp3", 128)
        .await
        .unwrap_err();
    assert_eq!(reads_of(&h.nas, "m/a.mp3"), reads);
    h.nas.add_file("m/a.mp3", &tagged(Some(blue())), 2);
    let changed = thumbnail(&h, &previews, "lautta://nv-nas/m/a.mp3", 128)
        .await
        .unwrap();
    assert!(near(tint(&changed), blue()), "a changed file is read again");
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn local_audio_gets_cover_thumbnails_too() {
    let h = home().await;
    std::fs::write(h.root.join("Documents/song.mp3"), tagged(Some(red()))).unwrap();
    let previews = h.core.previews(&tokio::runtime::Handle::current()).unwrap();
    let t = thumbnail(&h, &previews, "lautta://user-documents/song.mp3", 64)
        .await
        .unwrap();
    assert!(near(tint(&t), red()));
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn server_audio_thumbnails_follow_switches_but_the_player_cover_does_not() {
    let h = home().await;
    h.nas.add_file("m/own.mp3", &tagged(Some(red())), 1);
    let previews = h.core.previews(&tokio::runtime::Handle::current()).unwrap();
    let u = "lautta://nv-nas/m/own.mp3";

    h.core.set_power_save(true);
    let err = thumbnail(&h, &previews, u, 128).await.unwrap_err();
    assert_eq!(
        err.kind,
        ErrorKind::Unsupported,
        "battery saving reads no server audio"
    );
    assert_eq!(reads_of(&h.nas, "m/own.mp3"), 0);
    let mut s = h.core.settings();
    s.thumbnails_remote = false;
    h.core.apply_settings(s);
    h.core.set_power_save(false);
    let err = thumbnail(&h, &previews, u, 128).await.unwrap_err();
    assert_eq!(err.kind, ErrorKind::Unsupported, "Thumbnails from servers off");

    h.core.set_power_save(true);
    let c = cover(&h, &previews, u, PreviewKind::Cover).await.unwrap();
    assert!(
        near(tint(&c), red()),
        "player shows the cover of the file it plays"
    );
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn the_glow_is_a_small_soft_copy_of_the_cover() {
    let h = home().await;
    h.nas.add_file("m/own.mp3", &tagged(Some(red())), 1);
    h.nas.add_file("m/a.jpg", &jpeg(10, 10), 1);
    let previews = h.core.previews(&tokio::runtime::Handle::current()).unwrap();
    let g = cover(&h, &previews, "lautta://nv-nas/m/own.mp3", PreviewKind::Glow)
        .await
        .unwrap();
    let px = thumb_pixels(&g);
    assert_eq!((px.width(), px.height()), (48, 48));
    assert!(near(tint(&g), red()));
    let reads = reads_of(&h.nas, "m/own.mp3");
    cover(&h, &previews, "lautta://nv-nas/m/own.mp3", PreviewKind::Glow)
        .await
        .unwrap();
    assert_eq!(reads_of(&h.nas, "m/own.mp3"), reads, "kept");

    let err = cover(&h, &previews, "lautta://nv-nas/m/a.jpg", PreviewKind::Cover)
        .await
        .unwrap_err();
    assert_eq!(err.kind, ErrorKind::Unsupported, "covers are for audio files");
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn folder_pictures_up_to_the_limit_are_covers_larger_ones_are_not() {
    const LIMIT: u64 = 16 * 1024 * 1024;
    let h = home().await;
    let padded = |extra: u64| {
        let mut jpg = audio_fixture::jpeg(32, blue());
        jpg.resize((LIMIT + extra) as usize, 0);
        jpg
    };
    h.nas.add_file("fit/a.mp3", &tagged(None), 1);
    h.nas.add_file("fit/cover.jpg", &padded(0), 1);
    h.nas.add_file("big/a.mp3", &tagged(None), 1);
    h.nas.add_file("big/cover.jpg", &padded(1), 1);
    let previews = h.core.previews(&tokio::runtime::Handle::current()).unwrap();
    let fit = thumbnail(&h, &previews, "lautta://nv-nas/fit/a.mp3", 64)
        .await
        .unwrap();
    assert!(near(tint(&fit), blue()), "exactly the limit: {:?}", tint(&fit));
    let big = thumbnail(&h, &previews, "lautta://nv-nas/big/a.mp3", 64)
        .await
        .unwrap_err();
    assert_eq!(big.kind, ErrorKind::NotFound);
    assert_eq!(reads_of(&h.nas, "big/cover.jpg"), 0, "too large: never read");
}

fn listed(name: &str, size: u64) -> lautta_core::entry::Entry {
    let mut e = lautta_core::entry::Entry::new(name.as_bytes(), lautta_core::entry::Kind::File);
    e.size = Some(size);
    e.modified = Some(std::time::UNIX_EPOCH + std::time::Duration::from_millis(1));
    e
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_cached_folder_listing_finds_the_folder_cover_without_listing_again() {
    let h = home().await;
    let jpg = audio_fixture::jpeg(32, blue());
    h.nas.add_file("m/a.mp3", &tagged(None), 1);
    h.nas.add_file("m/cover.jpg", &jpg, 1);
    let entries = [listed("a.mp3", 9), listed("cover.jpg", jpg.len() as u64)];
    h.core.dircache.put(&uri("lautta://nv-nas/m"), &entries).unwrap();
    let previews = h.core.previews(&tokio::runtime::Handle::current()).unwrap();
    let info = h.core.audio_info(&uri("lautta://nv-nas/m/a.mp3")).await.unwrap();
    assert_eq!(info.cover, CoverFrom::Folder);
    let t = thumbnail(&h, &previews, "lautta://nv-nas/m/a.mp3", 64)
        .await
        .unwrap();
    assert!(near(tint(&t), blue()));
    let lists: Vec<String> = h
        .nas
        .calls()
        .into_iter()
        .filter(|c| c.starts_with("list"))
        .collect();
    assert!(lists.is_empty(), "folder listed again: {lists:?}");
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_folder_picture_grown_past_the_limit_since_listing_is_no_cover() {
    const LIMIT: usize = 16 * 1024 * 1024;
    let h = home().await;
    let mut jpg = audio_fixture::jpeg(32, blue());
    jpg.resize(LIMIT + 1, 0);
    h.nas.add_file("m/a.mp3", &tagged(None), 1);
    h.nas.add_file("m/cover.jpg", &jpg, 1);
    let entries = [listed("a.mp3", 9), listed("cover.jpg", 1000)];
    h.core.dircache.put(&uri("lautta://nv-nas/m"), &entries).unwrap();
    let previews = h.core.previews(&tokio::runtime::Handle::current()).unwrap();
    let err = thumbnail(&h, &previews, "lautta://nv-nas/m/a.mp3", 64)
        .await
        .unwrap_err();
    assert_eq!(err.kind, ErrorKind::NotFound);
}
