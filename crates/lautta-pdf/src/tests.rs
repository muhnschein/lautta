// SPDX-License-Identifier: LGPL-2.1-or-later
use super::sample::{
    build, page, Spec, LOCKED_AES256, LOCKED_OWNER_PASSWORD, LOCKED_PASSWORD, LOCKED_RC4, RED_SQUARE,
};
use super::*;

fn pixel(p: &RenderedPage, x: u32, y: u32) -> [u8; 4] {
    let i = ((y * p.width + x) * 4) as usize;
    [p.rgba[i], p.rgba[i + 1], p.rgba[i + 2], p.rgba[i + 3]]
}

const RED: [u8; 4] = [255, 0, 0, 255];
const WHITE_PX: [u8; 4] = [255, 255, 255, 255];

#[test]
fn pages_and_sizes_are_read() {
    let doc = PdfDocument::open(build(&[page(100, 200, ""), page(300, 150, "")])).unwrap();
    assert_eq!(doc.page_count(), 2);
    assert_eq!(
        doc.page_sizes(),
        [
            PageSize {
                width: 100.0,
                height: 200.0
            },
            PageSize {
                width: 300.0,
                height: 150.0
            }
        ]
    );
    assert_eq!(format!("{doc:?}"), "PdfDocument { pages: 2 }");
}

#[test]
fn a_page_renders_at_the_asked_width_on_white() {
    let doc = PdfDocument::open(build(&[page(100, 200, RED_SQUARE)])).unwrap();
    let r = doc.render(0, 200).unwrap();
    assert_eq!((r.width, r.height), (200, 400), "twice the points, aspect kept");
    assert_eq!(r.rgba.len(), 200 * 400 * 4);
    // PDF y grows upwards: the square sits in the bottom left corner.
    assert_eq!(pixel(&r, 20, 380), RED);
    assert_eq!(pixel(&r, 180, 20), WHITE_PX, "the background is white");
    assert_eq!(pixel(&r, 120, 380), WHITE_PX, "the square ends at 50 pt");
}

#[test]
fn rotated_pages_are_shown_upright() {
    let rotated = Spec {
        w: 100,
        h: 200,
        rotate: 90,
        content: RED_SQUARE.to_string(),
    };
    let doc = PdfDocument::open(build(&[rotated])).unwrap();
    assert_eq!(
        doc.page_sizes()[0],
        PageSize {
            width: 200.0,
            height: 100.0
        }
    );
    let r = doc.render(0, 200).unwrap();
    assert_eq!((r.width, r.height), (200, 100));
    // Turned clockwise: the bottom left corner moves to the top left.
    assert_eq!(pixel(&r, 10, 10), RED);
    assert_eq!(pixel(&r, 10, 90), WHITE_PX);
}

#[test]
fn page_numbers_out_of_range_fail() {
    let doc = PdfDocument::open(build(&[page(100, 100, "")])).unwrap();
    assert_eq!(doc.render(1, 100), Err(PdfError::NoSuchPage));
    assert_eq!(doc.render(usize::MAX, 100), Err(PdfError::NoSuchPage));
}

#[test]
fn garbage_and_empty_documents_are_refused() {
    assert_eq!(
        PdfDocument::open(b"this is not a pdf".to_vec()).unwrap_err(),
        PdfError::Invalid
    );
    assert_eq!(PdfDocument::open(Vec::new()).unwrap_err(), PdfError::Invalid);
    assert_eq!(PdfDocument::open(build(&[])).unwrap_err(), PdfError::Empty);
}

#[test]
fn a_damaged_cross_reference_table_is_repaired() {
    let mut bytes = build(&[page(100, 200, RED_SQUARE)]);
    let at = bytes.windows(9).position(|w| w == b"startxref").unwrap();
    bytes.truncate(at);
    bytes.extend_from_slice(b"startxref\n999999\n%%EOF\n");
    let doc = PdfDocument::open(bytes).unwrap();
    assert_eq!(pixel(&doc.render(0, 100).unwrap(), 10, 190), RED);
}

#[test]
fn password_protected_documents_are_named_as_such() {
    // Standard security handler, revision 3, with an /O and /U that match
    // no password; the empty password does not open it.
    let mut bytes = build(&[page(100, 100, RED_SQUARE)]);
    let at = bytes.windows(7).position(|w| w == b"trailer").unwrap();
    let tail = String::from_utf8(bytes.split_off(at)).unwrap();
    let enc = format!(
        "/Encrypt << /Filter /Standard /V 2 /R 3 /Length 128 /P -4 /O <{o}> /U <{u}> >> /ID [<{id}> <{id}>]",
        o = "11".repeat(32),
        u = "22".repeat(32),
        id = "33".repeat(16)
    );
    bytes.extend_from_slice(
        tail.replace("/Root 1 0 R", &format!("/Root 1 0 R {enc}"))
            .as_bytes(),
    );
    assert_eq!(PdfDocument::open(bytes).unwrap_err(), PdfError::Encrypted);
}

#[test]
fn oversized_files_are_refused_before_parsing() {
    let big = vec![b' '; MAX_PDF_BYTES as usize + 1];
    assert_eq!(PdfDocument::open(big).unwrap_err(), PdfError::TooLarge);
}

#[test]
fn renders_stay_within_the_edge_limit() {
    let a4 = PageSize {
        width: 595.0,
        height: 842.0,
    };
    assert!((render_scale(a4, 1190) - 2.0).abs() < 1e-6, "fit to width");
    assert!(
        (render_scale(a4, 0) - 1.0 / 595.0).abs() < 1e-9,
        "at least one pixel"
    );
    // Wider than the edge limit: clamped to it.
    assert!((render_scale(a4, 100_000) * 842.0 - MAX_EDGE as f32).abs() < 0.01);
    // A long strip: the height, not the asked width, decides.
    let strip = PageSize {
        width: 100.0,
        height: 10_000.0,
    };
    assert!((render_scale(strip, 1000) * 10_000.0 - MAX_EDGE as f32).abs() < 0.01);
    // Degenerate sizes do not divide by zero.
    let zero = PageSize {
        width: 0.0,
        height: 0.0,
    };
    assert!((render_scale(zero, 10) - 10.0).abs() < 1e-6);
}

#[test]
fn a_huge_page_renders_within_the_limits() {
    let doc = PdfDocument::open(build(&[page(14_400, 14_400, RED_SQUARE)])).unwrap();
    let r = doc.render(0, 100_000).unwrap();
    assert_eq!((r.width, r.height), (MAX_EDGE, MAX_EDGE));
}

#[test]
fn errors_read_as_sentences() {
    for (e, text) in [
        (PdfError::Invalid, "not a readable PDF"),
        (PdfError::Encrypted, "the PDF is protected with a password"),
        (PdfError::Empty, "the PDF has no pages"),
        (PdfError::NoSuchPage, "no such page"),
        (PdfError::TooLarge, "the PDF is too large to show"),
    ] {
        assert_eq!(e.to_string(), text);
    }
}

#[test]
fn encrypted_documents_open_with_their_password() {
    for bytes in [LOCKED_AES256, LOCKED_RC4] {
        for wrong in ["", "Sesame", "sesame "] {
            assert_eq!(
                PdfDocument::open_with_password(bytes.to_vec(), wrong).unwrap_err(),
                PdfError::Encrypted,
                "{wrong:?}"
            );
        }
        assert_eq!(
            PdfDocument::open(bytes.to_vec()).unwrap_err(),
            PdfError::Encrypted
        );
        let doc = PdfDocument::open_with_password(bytes.to_vec(), LOCKED_PASSWORD).unwrap();
        assert_eq!(doc.page_count(), 2);
        assert_eq!(
            pixel(&doc.render(0, 100).unwrap(), 10, 190),
            RED,
            "decrypted content"
        );
    }
    // hayro checks the owner password of AES-256 documents only.
    let doc = PdfDocument::open_with_password(LOCKED_AES256.to_vec(), LOCKED_OWNER_PASSWORD).unwrap();
    assert_eq!(doc.page_count(), 2);
}
