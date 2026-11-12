// SPDX-License-Identifier: LGPL-2.1-or-later
use super::*;
use crate::preview::exif::testutil;
use crate::scan::Point;
use image::{Rgb, RgbImage};
use lautta_pdf::PdfDocument;

const SHEET: [(f32, f32); 4] = [(0.2, 0.2), (0.8, 0.2), (0.8, 0.8), (0.2, 0.8)];
const TABLE: [u8; 3] = [30, 30, 30];
const PAPER: [u8; 3] = [235, 235, 235];
const RED: [u8; 3] = [200, 30, 30];

struct Desk {
    home: tempfile::TempDir,
    session: ScanSession,
}

fn desk() -> Desk {
    let home = tempfile::tempdir().unwrap();
    let session = ScanSession::create(&AppPaths::new(home.path())).unwrap();
    Desk { home, session }
}

/// Sheet in the middle of a dark table; the sheet's top left quarter is red.
fn sheet_photo(w: u32, h: u32) -> RgbImage {
    RgbImage::from_fn(w, h, |x, y| {
        let (fx, fy) = (x as f32 / w as f32, y as f32 / h as f32);
        Rgb(match (fx, fy) {
            _ if !(0.2..0.8).contains(&fx) || !(0.2..0.8).contains(&fy) => TABLE,
            _ if fx < 0.5 && fy < 0.5 => RED,
            _ => PAPER,
        })
    })
}

fn jpeg_bytes(img: &RgbImage) -> Vec<u8> {
    jpeg(img, 95).unwrap()
}

impl Desk {
    /// Photo outside the session, as picked from a folder.
    fn file(&self, name: &str, bytes: &[u8]) -> PathBuf {
        let path = self.home.path().join(name);
        std::fs::write(&path, bytes).unwrap();
        path
    }

    fn sheet(&self) -> PathBuf {
        self.file("sheet.jpg", &jpeg_bytes(&sheet_photo(800, 600)))
    }

    fn plain(&self) -> PathBuf {
        self.file(
            "plain.jpg",
            &jpeg_bytes(&RgbImage::from_pixel(400, 300, Rgb([90, 120, 150]))),
        )
    }

    fn info(&self, index: usize) -> PageInfo {
        self.session.pages()[index].clone()
    }

    fn view(&self, index: usize) -> RgbImage {
        image::open(self.info(index).view).unwrap().into_rgb8()
    }

    fn files(&self) -> Vec<String> {
        let mut names: Vec<String> = std::fs::read_dir(self.session.dir())
            .unwrap()
            .map(|e| e.unwrap().file_name().to_string_lossy().into_owned())
            .collect();
        names.sort();
        names
    }

    fn pdf(&self) -> Vec<u8> {
        self.session
            .write_pdf(Vec::new(), "Scan", "D:20261112101500Z", &|_, _| {})
            .unwrap()
    }
}

fn near(got: Rgb<u8>, want: [u8; 3]) -> bool {
    got.0.iter().zip(want).all(|(a, b)| a.abs_diff(b) <= 25)
}

fn assert_sheet(quad: [f32; 8]) {
    for (i, want) in SHEET.iter().enumerate() {
        let (x, y) = (quad[2 * i], quad[2 * i + 1]);
        assert!(
            (x - want.0).abs() < 0.02 && (y - want.1).abs() < 0.02,
            "corner {i}: {x},{y}"
        );
    }
}

#[test]
fn session_lives_in_its_own_folder_under_the_cache() {
    let d = desk();
    let root = d.home.path().join(".cache/org.netvfs/lautta/scan");
    assert_eq!(d.session.dir().parent(), Some(root.as_path()));
    assert!(d.session.dir().is_dir());
    assert!(d.session.is_empty());
    let dir = d.session.dir().to_path_buf();
    drop(d.session);
    assert!(!dir.exists());
    assert!(root.is_dir());
}

#[test]
fn leftovers_of_a_killed_app_are_removed() {
    let home = tempfile::tempdir().unwrap();
    let paths = AppPaths::new(home.path());
    let stale = paths.scan_dir().join("scan-old");
    std::fs::create_dir_all(&stale).unwrap();
    std::fs::write(stale.join("page-1.jpg"), b"x").unwrap();
    let session = ScanSession::create(&paths).unwrap();
    assert!(!stale.exists());
    assert!(session.dir().is_dir());
}

#[test]
fn added_photo_becomes_a_page_cut_to_the_sheet() {
    let mut d = desk();
    let photo = d.sheet();
    assert_eq!(d.session.add(&photo).unwrap(), 0);
    assert_eq!(d.session.len(), 1);
    assert!(!d.session.is_empty());
    let info = d.info(0);
    assert_eq!(
        (info.id, info.turns, info.colour, info.found),
        (1, 0, "original", true)
    );
    assert_sheet(info.quad);
    assert_eq!(image::image_dimensions(&info.picture).unwrap(), (800, 600));
    let view = d.view(0);
    assert!(view.width().abs_diff(480) <= 16 && view.height().abs_diff(360) <= 12);
    let (w, h) = (view.width(), view.height());
    assert!(near(*view.get_pixel(w / 4, h / 4), RED));
    assert!(near(*view.get_pixel(3 * w / 4, h / 4), PAPER));
    assert!(near(*view.get_pixel(w / 4, 3 * h / 4), PAPER));
    assert!(near(*view.get_pixel(4, 4), RED), "no table left at the corner");
    assert_eq!(d.files(), ["photo-1.jpg", "view-1-1.jpg", "work-1.jpg"]);
}

#[test]
fn photo_from_a_folder_is_copied_and_camera_shot_is_moved() {
    let mut d = desk();
    let outside = d.sheet();
    d.session.add(&outside).unwrap();
    assert!(outside.exists());
    let shot = d.session.dir().join("shot.jpg");
    std::fs::copy(&outside, &shot).unwrap();
    assert_eq!(d.session.add(&shot).unwrap(), 1);
    assert!(!shot.exists());
    assert_eq!(d.info(1).id, 2);
}

#[test]
fn large_photo_is_shown_on_a_smaller_working_picture() {
    let mut d = desk();
    let photo = d.file("large.jpg", &jpeg_bytes(&sheet_photo(2400, 1200)));
    d.session.add(&photo).unwrap();
    assert_eq!(image::image_dimensions(d.info(0).picture).unwrap(), (1600, 800));
    assert_sheet(d.info(0).quad);
}

#[test]
fn photo_without_edges_is_a_page_of_the_whole_picture() {
    let mut d = desk();
    d.session.add(&d.plain()).unwrap();
    let info = d.info(0);
    assert!(!info.found);
    assert_eq!(info.quad, Quad::FULL.to_flat());
    assert_eq!(d.view(0).dimensions(), (400, 300));
}

#[test]
fn broken_photo_adds_nothing_and_leaves_no_files() {
    let mut d = desk();
    let junk = d.file("junk.jpg", b"not a picture");
    assert_eq!(d.session.add(&junk).unwrap_err().kind, ErrorKind::Unsupported);
    assert_eq!(d.session.len(), 0);
    assert!(d.files().is_empty());
    let missing = d.home.path().join("none.jpg");
    assert_eq!(d.session.add(&missing).unwrap_err().kind, ErrorKind::NotFound);
    d.session.add(&d.plain()).unwrap();
    assert_eq!(d.info(0).id, 1, "failed tries use up no page number");
    assert_eq!(d.files(), ["photo-3.jpg", "view-3-1.jpg", "work-3.jpg"]);
}

#[test]
fn oversized_picture_is_refused() {
    let mut d = desk();
    // PNG of 20000 × 6000 pixels up to the start of its pixel data: enough to read the size.
    let mut png = vec![0x89, b'P', b'N', b'G', 0x0d, 0x0a, 0x1a, 0x0a];
    let mut chunk = |kind: &[u8; 4], data: &[u8]| {
        png.extend_from_slice(&(data.len() as u32).to_be_bytes());
        let mut crc = flate2::Crc::new();
        crc.update(kind);
        crc.update(data);
        png.extend_from_slice(kind);
        png.extend_from_slice(data);
        png.extend_from_slice(&crc.sum().to_be_bytes());
    };
    let mut header = 20_000u32.to_be_bytes().to_vec();
    header.extend_from_slice(&6_000u32.to_be_bytes());
    header.extend_from_slice(&[8, 2, 0, 0, 0]);
    chunk(b"IHDR", &header);
    chunk(b"IDAT", &[0x78, 0x9c]);
    let e = d.session.add(&d.file("huge.png", &png)).unwrap_err();
    assert_eq!(e.kind, ErrorKind::Unsupported);
    assert!(e.message.contains("too large"), "{}", e.message);
}

#[test]
fn exif_orientation_turns_the_photo_upright() {
    let mut d = desk();
    let spec = testutil::Spec {
        orientation: Some(6),
        thumbnail: None,
        gps: false,
    };
    // Stored lying on its left side: red quarter bottom left turns to top left.
    let lying = image::imageops::rotate270(&sheet_photo(800, 600));
    let photo = d.file("lying.jpg", &testutil::with_exif(&jpeg_bytes(&lying), &spec));
    d.session.add(&photo).unwrap();
    assert_eq!(image::image_dimensions(d.info(0).picture).unwrap(), (800, 600));
    let view = d.view(0);
    assert!(near(*view.get_pixel(view.width() / 4, view.height() / 4), RED));
}

#[test]
fn next_page_takes_the_colour_of_the_page_before() {
    let mut d = desk();
    let photo = d.sheet();
    d.session.add(&photo).unwrap();
    d.session.set_colour(0, Colour::Grayscale).unwrap();
    d.session.add(&photo).unwrap();
    assert_eq!(d.info(1).colour, "grayscale");
    assert_eq!(d.info(0).colour, "grayscale");
}

#[test]
fn gray_page_has_no_colour_and_full_contrast() {
    let mut d = desk();
    d.session.add(&d.sheet()).unwrap();
    let before = d.info(0).view;
    d.session.set_colour(0, Colour::Grayscale).unwrap();
    let info = d.info(0);
    assert_eq!(info.colour, "grayscale");
    assert!(info.view.ends_with("view-1-2.jpg"), "{}", info.view);
    assert!(!Path::new(&before).exists(), "old view removed");
    let img = image::open(&info.view).unwrap();
    assert_eq!(img.color(), image::ColorType::L8);
    let view = img.into_luma8();
    let (w, h) = view.dimensions();
    assert!(
        view.get_pixel(w / 4, h / 4)[0] < 40,
        "red quarter stretched to black"
    );
    assert!(view.get_pixel(3 * w / 4, 3 * h / 4)[0] > 215);
}

#[test]
fn black_and_white_page_has_only_ink_and_paper() {
    let mut d = desk();
    d.session.add(&d.sheet()).unwrap();
    d.session.set_colour(0, Colour::BlackWhite).unwrap();
    let info = d.info(0);
    assert!(info.view.ends_with("view-1-2.png"), "{}", info.view);
    let view = image::open(&info.view).unwrap().into_luma8();
    assert!(view.pixels().all(|p| p[0] == 0 || p[0] == 255));
    assert!(view.pixels().any(|p| p[0] == 0) && view.pixels().any(|p| p[0] == 255));
    d.session.set_colour(0, Colour::Original).unwrap();
    assert!(near(*d.view(0).get_pixel(20, 20), RED));
    assert_eq!(d.files(), ["photo-1.jpg", "view-1-3.jpg", "work-1.jpg"]);
}

#[test]
fn turning_moves_the_top_left_quarter_clockwise() {
    let mut d = desk();
    d.session.add(&d.sheet()).unwrap();
    let (w, h) = d.view(0).dimensions();
    d.session.turn(0, 1).unwrap();
    let view = d.view(0);
    assert_eq!(view.dimensions(), (h, w));
    assert!(near(*view.get_pixel(3 * h / 4, w / 4), RED), "red now top right");
    assert!(near(*view.get_pixel(h / 4, w / 4), PAPER));
    assert_eq!(d.info(0).turns, 1);
    d.session.turn(0, 1).unwrap();
    let view = d.view(0);
    assert_eq!(view.dimensions(), (w, h));
    assert!(
        near(*view.get_pixel(3 * w / 4, 3 * h / 4), RED),
        "red now bottom right"
    );
    d.session.turn(0, 1).unwrap();
    let view = d.view(0);
    assert!(
        near(*view.get_pixel(h / 4, 3 * w / 4), RED),
        "red now bottom left"
    );
    assert_eq!(d.info(0).turns, 3);
    d.session.turn(0, 1).unwrap();
    assert_eq!(d.info(0).turns, 0);
    d.session.turn(0, -1).unwrap();
    assert_eq!(d.info(0).turns, 3);
    d.session.turn(0, -2).unwrap();
    assert_eq!(d.info(0).turns, 1);
}

#[test]
fn moved_corners_cut_another_part() {
    let mut d = desk();
    d.session.add(&d.sheet()).unwrap();
    let red_only = Quad::from_flat(&[0.25, 0.25, 0.45, 0.25, 0.45, 0.45, 0.25, 0.45]).unwrap();
    d.session.set_quad(0, red_only).unwrap();
    let view = d.view(0);
    assert_eq!(view.dimensions(), (160, 120));
    assert!(view.pixels().all(|p| near(*p, RED)));
    assert_eq!(d.info(0).quad, red_only.to_flat());
    assert!(d.info(0).found);
    d.session.set_quad(0, Quad::FULL).unwrap();
    assert_eq!(d.view(0).dimensions(), (800, 600));
    assert!(!d.info(0).found);
}

#[test]
fn edge_search_answers_without_changing_the_page() {
    let mut d = desk();
    d.session.add(&d.sheet()).unwrap();
    d.session.add(&d.plain()).unwrap();
    d.session.set_quad(0, Quad::FULL).unwrap();
    let found = d.session.find_edges(0).unwrap().unwrap();
    assert_sheet(found.to_flat());
    assert_eq!(d.info(0).quad, Quad::FULL.to_flat());
    assert!(!d.info(0).found);
    let odd = Quad([
        Point::new(0.1, 0.1),
        Point::new(0.9, 0.1),
        Point::new(0.9, 0.9),
        Point::new(0.1, 0.9),
    ]);
    d.session.set_quad(1, odd).unwrap();
    assert_eq!(d.session.find_edges(1).unwrap(), None);
    assert_eq!(d.info(1).quad, odd.to_flat());
    assert_eq!(d.session.find_edges(2).unwrap_err().kind, ErrorKind::NotFound);
}

#[test]
fn removed_page_takes_its_files_along() {
    let mut d = desk();
    let photo = d.sheet();
    d.session.add(&photo).unwrap();
    d.session.add(&photo).unwrap();
    d.session.remove(0).unwrap();
    assert_eq!(d.session.len(), 1);
    assert_eq!(d.info(0).id, 2);
    assert_eq!(d.files(), ["photo-2.jpg", "view-2-1.jpg", "work-2.jpg"]);
    assert_eq!(d.session.remove(1).unwrap_err().kind, ErrorKind::NotFound);
    assert_eq!(d.session.len(), 1);
}

#[test]
fn pages_move_to_a_new_place_in_the_row() {
    let mut d = desk();
    let photo = d.sheet();
    for _ in 0..4 {
        d.session.add(&photo).unwrap();
    }
    let order = |d: &Desk| d.session.pages().iter().map(|p| p.id).collect::<Vec<_>>();
    d.session.move_page(0, 2).unwrap();
    assert_eq!(order(&d), [2, 3, 1, 4]);
    d.session.move_page(3, 0).unwrap();
    assert_eq!(order(&d), [4, 2, 3, 1]);
    d.session.move_page(1, 3).unwrap();
    assert_eq!(order(&d), [4, 3, 1, 2]);
    assert_eq!((d.session.position(4), d.session.position(2)), (Some(0), Some(3)));
    assert_eq!(d.session.position(5), None);
    assert_eq!(d.session.move_page(4, 0).unwrap_err().kind, ErrorKind::NotFound);
    assert_eq!(d.session.move_page(0, 4).unwrap_err().kind, ErrorKind::NotFound);
    assert_eq!(order(&d), [4, 3, 1, 2]);
}

#[test]
fn retake_swaps_the_photo_and_keeps_place_and_colour() {
    let mut d = desk();
    let photo = d.sheet();
    d.session.add(&photo).unwrap();
    d.session.add(&photo).unwrap();
    d.session.set_colour(0, Colour::Grayscale).unwrap();
    d.session.turn(0, 1).unwrap();
    d.session.retake(0, &d.plain()).unwrap();
    assert_eq!(d.session.len(), 2);
    let info = d.info(0);
    assert_eq!(
        (info.id, info.colour, info.turns, info.found),
        (1, "grayscale", 0, false)
    );
    assert!(info.picture.ends_with("work-3.jpg"), "{}", info.picture);
    assert_eq!(d.info(1).id, 2);
    assert_eq!(
        d.files(),
        [
            "photo-2.jpg",
            "photo-3.jpg",
            "view-2-1.jpg",
            "view-3-1.jpg",
            "work-2.jpg",
            "work-3.jpg"
        ]
    );
    let junk = d.file("junk.jpg", b"no");
    assert_eq!(
        d.session.retake(0, &junk).unwrap_err().kind,
        ErrorKind::Unsupported
    );
    assert!(
        d.info(0).picture.ends_with("work-3.jpg"),
        "page stays when the new photo is unusable"
    );
    assert_eq!(d.session.retake(2, &photo).unwrap_err().kind, ErrorKind::NotFound);
    assert_eq!(d.files().len(), 6);
}

#[test]
fn changes_to_missing_pages_fail() {
    let mut d = desk();
    assert_eq!(
        d.session.set_colour(0, Colour::Grayscale).unwrap_err().kind,
        ErrorKind::NotFound
    );
    assert_eq!(d.session.turn(0, 1).unwrap_err().kind, ErrorKind::NotFound);
    assert_eq!(
        d.session.set_quad(0, Quad::FULL).unwrap_err().kind,
        ErrorKind::NotFound
    );
}

#[test]
fn pdf_has_every_page_in_order_as_shown() {
    let mut d = desk();
    let photo = d.sheet();
    d.session.add(&photo).unwrap();
    d.session.add(&photo).unwrap();
    d.session.add(&photo).unwrap();
    d.session.turn(1, 1).unwrap();
    d.session.set_colour(1, Colour::Grayscale).unwrap();
    d.session.set_colour(2, Colour::BlackWhite).unwrap();
    let pdf = d.pdf();
    let text = String::from_utf8_lossy(&pdf).into_owned();
    assert_eq!(
        text.matches("/DeviceRGB /BitsPerComponent 8 /Filter /DCTDecode")
            .count(),
        1
    );
    assert_eq!(
        text.matches("/DeviceGray /BitsPerComponent 8 /Filter /DCTDecode")
            .count(),
        1
    );
    assert_eq!(
        text.matches("/DeviceGray /BitsPerComponent 1 /Filter /FlateDecode")
            .count(),
        1
    );
    let doc = PdfDocument::open(pdf).unwrap();
    assert_eq!(doc.page_count(), 3);
    let sizes: Vec<(i32, i32)> = doc
        .page_sizes()
        .iter()
        .map(|s| (s.width.round() as i32, s.height.round() as i32))
        .collect();
    // Sheet of 4:3 with the long side of A4; the turned one stands upright.
    assert!(sizes[0].0 == 842 && (sizes[0].1 - 631).abs() <= 12, "{sizes:?}");
    assert!(sizes[1].1 == 842 && (sizes[1].0 - 631).abs() <= 12, "{sizes:?}");
    assert_eq!(sizes[2], sizes[0]);
    let page = doc.render(0, 200).unwrap();
    let at = |page: &lautta_pdf::RenderedPage, x: u32, y: u32| {
        let i = ((y * page.width + x) * 4) as usize;
        Rgb([page.rgba[i], page.rgba[i + 1], page.rgba[i + 2]])
    };
    assert!(near(at(&page, 50, 40), RED));
    assert!(near(at(&page, 150, 110), PAPER));
    let turned = doc.render(1, 150).unwrap();
    let px = at(&turned, 110, 50);
    assert!(
        px.0[0] < 60 && px.0[0] == px.0[1] && px.0[1] == px.0[2],
        "gray red quarter top right: {px:?}"
    );
    assert!(near(at(&turned, 40, 50), [255, 255, 255]));
}

#[test]
fn saved_page_is_cut_from_the_full_photo_up_to_a4_at_200_dpi() {
    let mut d = desk();
    let large = d.file(
        "large.jpg",
        &jpeg_bytes(&RgbImage::from_pixel(3000, 1500, Rgb([200, 200, 200]))),
    );
    d.session.add(&large).unwrap();
    d.session
        .add(&d.file("mid.jpg", &jpeg_bytes(&sheet_photo(2000, 1000))))
        .unwrap();
    assert_eq!(d.view(0).dimensions(), (1600, 800));
    let text = String::from_utf8_lossy(&d.pdf()).into_owned();
    assert!(text.contains("/Width 2339 /Height 1170 "), "capped page");
    // Sheet covers 60 % of each side of the 2000 px photo: more than the working picture has.
    let width: u32 = text
        .rsplit("/Width ")
        .next()
        .and_then(|t| t.split(' ').next())
        .and_then(|n| n.parse().ok())
        .unwrap();
    assert!(width.abs_diff(1200) <= 30, "width {width}");
}

#[test]
fn saving_reports_each_page() {
    let mut d = desk();
    let photo = d.plain();
    d.session.add(&photo).unwrap();
    d.session.add(&photo).unwrap();
    let seen = std::sync::Mutex::new(Vec::new());
    d.session
        .write_pdf(Vec::new(), "t", "D:20261112101500Z", &|done, of| {
            seen.lock().unwrap().push((done, of))
        })
        .unwrap();
    assert_eq!(*seen.lock().unwrap(), [(1, 2), (2, 2)]);
}

#[test]
fn scan_without_pages_cannot_be_saved() {
    let d = desk();
    let e = d
        .session
        .write_pdf(Vec::new(), "t", "D:20261112101500Z", &|_, _| {})
        .unwrap_err();
    assert_eq!(e.kind, ErrorKind::InvalidArgument);
}
