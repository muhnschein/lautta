// SPDX-License-Identifier: LGPL-2.1-or-later
use super::{find, fraction, gap, Gap, Placed, TextChar, TextSink, LINE_BACK, LINE_SHIFT, WORD_GAP};
use crate::sample::{build, page, text, Spec};
use crate::{PageRect, PageSize, PdfDocument, PdfError, TextHit, MAX_PAGE_CHARS, MAX_PAGE_HITS};
use hayro::hayro_interpret::hayro_cmap::BfString;
use hayro::kurbo::{Affine, Point, Rect};

/// Helvetica advance widths per 1000 units of font size.
const HELLO: f32 = 722.0 + 556.0 + 222.0 + 222.0 + 556.0;
const WORLD: f32 = 722.0 + 556.0 + 333.0 + 222.0 + 556.0;
fn pt(units: f32) -> f32 {
    units * 12.0 / 1000.0
}

fn search(content: &str, query: &str) -> Vec<TextHit> {
    let doc = PdfDocument::open(build(&[page(200, 100, content)])).unwrap();
    doc.search_page(0, query).unwrap()
}

fn assert_near(r: PageRect, x: f32, y: f32, width: f32, height: f32) {
    let near = |a: f32, b: f32| (a - b).abs() < 1e-3;
    assert!(
        near(r.x, x) && near(r.y, y) && near(r.width, width) && near(r.height, height),
        "{r:?} is not near {x} {y} {width} {height}"
    );
}

fn only_area(hits: &[TextHit]) -> PageRect {
    assert_eq!(hits.len(), 1, "{hits:?}");
    assert_eq!(hits[0].areas.len(), 1, "{hits:?}");
    hits[0].areas[0]
}

#[test]
fn a_word_is_found_where_it_is_shown() {
    let hits = search(&text(10, 50, "Hello world"), "hello");
    // box spans the advance, 0.2 em below to 0.8 em above the baseline; y counts from the top
    let top = 100.0 - 50.0 - pt(800.0);
    assert_near(only_area(&hits), 0.05, top / 100.0, pt(HELLO) / 200.0, 0.12);
}

#[test]
fn search_ignores_case() {
    let content = text(10, 50, "Hello Ärger");
    for query in ["hello", "HELLO", "hELLo", "ärger", "ÄRGER"] {
        assert_eq!(search(&content, query).len(), 1, "{query}");
    }
}

#[test]
fn every_hit_is_found_in_order() {
    let content = text(10, 50, "ab ab") + &text(10, 20, "xab");
    let hits = search(&content, "ab");
    let xs: Vec<f32> = hits.iter().map(|h| h.areas[0].x).collect();
    let ys: Vec<f32> = hits.iter().map(|h| h.areas[0].y).collect();
    let second = 10.0 + pt(556.0 + 556.0 + 278.0);
    let third = 10.0 + pt(500.0);
    assert_eq!(hits.len(), 3, "{hits:?}");
    assert!((xs[0] - 0.05).abs() < 1e-3 && (xs[1] - second / 200.0).abs() < 1e-3);
    assert!((xs[2] - third / 200.0).abs() < 1e-3);
    assert!(ys[0] == ys[1] && ys[2] > ys[1], "{ys:?}");
    assert!(search(&content, "abab").is_empty());
}

#[test]
fn a_gap_between_text_pieces_reads_as_a_space() {
    let pieces = "BT /F1 12 Tf 10 50 Td (Hello) Tj 40 0 Td (world) Tj ET";
    let width = 40.0 + pt(WORLD);
    assert_near(
        only_area(&search(pieces, "hello world")),
        0.05,
        0.404,
        width / 200.0,
        0.12,
    );
    assert!(search(pieces, "helloworld").is_empty());
    let kerned = "BT /F1 12 Tf 10 50 Td [(Hel) -20 (lo) -300 (world)] TJ ET";
    assert_eq!(search(kerned, "hello world").len(), 1);
    assert!(search(kerned, "helloworld").is_empty());
}

#[test]
fn a_hit_spans_lines_with_one_area_per_line() {
    let content = text(10, 60, "Hello") + &text(10, 40, "world");
    let hits = search(&content, "hello world");
    assert_eq!(hits.len(), 1);
    let areas = &hits[0].areas;
    assert_eq!(areas.len(), 2, "{areas:?}");
    assert_near(areas[0], 0.05, 0.304, pt(HELLO) / 200.0, 0.12);
    assert_near(areas[1], 0.05, 0.504, pt(WORLD) / 200.0, 0.12);
    assert!(search(&content, "helloworld").is_empty());
}

#[test]
fn whitespace_runs_match_each_other() {
    let spaced = text(10, 50, "Hello   world");
    assert_eq!(search(&spaced, "hello world").len(), 1);
    assert_eq!(search(&spaced, " hello \t\n world  ").len(), 1);
    let single = text(10, 50, "Hello world");
    assert_eq!(search(&single, "hello   world").len(), 1);
}

#[test]
fn missing_and_empty_queries_find_nothing() {
    let content = text(10, 50, "Hello world");
    assert!(search(&content, "goodbye").is_empty());
    assert!(search(&content, "").is_empty());
    assert!(search(&content, " \t\n ").is_empty());
}

#[test]
fn areas_follow_the_page_rotation() {
    let rotated = Spec {
        w: 100,
        h: 200,
        rotate: 90,
        content: text(10, 20, "Hello"),
    };
    let doc = PdfDocument::open(build(&[rotated])).unwrap();
    let hits = doc.search_page(0, "hello").unwrap();
    let left = 20.0 - pt(200.0);
    assert_near(only_area(&hits), left / 200.0, 0.1, 0.06, pt(HELLO) / 100.0);
}

#[test]
fn page_numbers_out_of_range_fail() {
    let doc = PdfDocument::open(build(&[page(100, 100, &text(10, 50, "x"))])).unwrap();
    assert_eq!(doc.search_page(1, "x"), Err(PdfError::NoSuchPage));
    assert_eq!(doc.search_page(usize::MAX, ""), Err(PdfError::NoSuchPage));
}

#[test]
fn text_drawn_twice_in_place_is_read_once() {
    let fill_and_stroke = "BT /F1 12 Tf 2 Tr 10 50 Td (Hello) Tj ET";
    assert_eq!(search(fill_and_stroke, "hello").len(), 1);
    let overprinted = text(10, 50, "Hello") + &text(10, 50, "Hello");
    assert_eq!(search(&overprinted, "hello").len(), 1);
    let elsewhere = text(10, 50, "Hello") + &text(11, 50, "Hello");
    assert_eq!(search(&elsewhere, "hello").len(), 2);
}

#[test]
fn work_per_page_is_bounded() {
    let long = "a".repeat(MAX_PAGE_CHARS - 1) + "bc";
    let content = text(10, 50, &long);
    assert_eq!(search(&content, "b").len(), 1, "the last character read");
    assert!(search(&content, "c").is_empty(), "past the limit");
    let many = text(10, 50, &"a".repeat(MAX_PAGE_HITS + 1));
    assert_eq!(search(&many, "a").len(), MAX_PAGE_HITS);
}

#[test]
fn broken_content_is_read_as_far_as_it_goes() {
    let broken = "BT /F9 Tf (Hello) Tj ET ] [ << Q Q Td Tj BT (";
    assert_eq!(search(broken, "hello").len(), 1);
}

const IDENTITY: Placed = Placed {
    inverse: Affine::IDENTITY,
    advance: 0.0,
};

#[test]
fn gaps_are_measured_in_the_last_glyphs_space() {
    let at = |x, y| gap(IDENTITY, Point::new(x, y));
    assert_eq!(at(0.0, 0.0), Gap::None);
    assert_eq!(at(WORD_GAP, 0.0), Gap::None);
    assert_eq!(at(WORD_GAP + 1.0, 0.0), Gap::Word);
    assert_eq!(at(0.0, LINE_SHIFT), Gap::None);
    assert_eq!(at(0.0, -LINE_SHIFT), Gap::None);
    assert_eq!(at(0.0, LINE_SHIFT + 1.0), Gap::Line);
    assert_eq!(at(0.0, -LINE_SHIFT - 1.0), Gap::Line);
    assert_eq!(at(-LINE_BACK, 0.0), Gap::None);
    assert_eq!(at(-LINE_BACK - 1.0, 0.0), Gap::Line);
    let wide = Placed {
        inverse: Affine::scale(0.5),
        advance: 500.0,
    };
    assert_eq!(gap(wide, Point::new(1300.0, 0.0)), Gap::None);
    assert_eq!(gap(wide, Point::new(1400.0, 0.0)), Gap::Word);
    let flat = Placed {
        inverse: Affine::scale(0.0).inverse(),
        advance: 0.0,
    };
    assert_eq!(gap(flat, Point::new(1.0, 1.0)), Gap::Line);
}

fn glyph(ch: char, x: f64) -> TextChar {
    TextChar {
        ch,
        area: Some(Rect::new(x, 0.0, x + 1.0, 2.0)),
        line: 0,
    }
}

const UNIT: PageSize = PageSize {
    width: 10.0,
    height: 4.0,
};

#[test]
fn a_ligature_shares_its_glyphs_area() {
    let mut sink = TextSink::default();
    sink.push_text(&BfString::Char('o'), Rect::new(0.0, 0.0, 1.0, 2.0));
    let ffi = Rect::new(1.0, 0.0, 2.0, 2.0);
    sink.push_text(&BfString::String("ffi".into()), ffi);
    let chars: Vec<_> = sink.chars.iter().map(|c| (c.ch, c.area)).collect();
    let shared = Some(ffi);
    assert_eq!(chars[1..], [('f', shared), ('f', shared), ('i', shared)]);
    let fi = find(&sink.chars, "fi", UNIT);
    assert_eq!(fi[0].areas, [fraction(ffi, UNIT)]);
    let whole = find(&sink.chars, "offi", UNIT);
    assert_eq!(whole[0].areas, [fraction(Rect::new(0.0, 0.0, 2.0, 2.0), UNIT)]);
}

#[test]
fn hits_after_lowercase_expansions_keep_their_areas() {
    // 'İ' lowercases to two chars, 'i' and a combining dot.
    let chars = [glyph('İ', 0.0), glyph('x', 3.0)];
    let hits = find(&chars, "x", UNIT);
    assert_eq!(
        hits[0].areas,
        [PageRect {
            x: 0.3,
            y: 0.0,
            width: 0.1,
            height: 0.5
        }]
    );
    assert_eq!(find(&chars, "i\u{307}x", UNIT).len(), 1);
}
