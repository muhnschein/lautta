// SPDX-License-Identifier: LGPL-2.1-or-later
use super::nav::{MAX_LINKS, MAX_NAMED_DESTS, MAX_OUTLINE_DEPTH, MAX_OUTLINE_ITEMS, MAX_URI_BYTES};
use super::sample::{link, page, page_ref, Doc, Spec};
use super::{Link, LinkTarget, OutlineItem, PageRect, PdfDocument, PdfError};

fn pages(n: usize) -> Doc {
    Doc::new(vec![page(256, 128, ""); n])
}

fn open(doc: &Doc) -> PdfDocument {
    PdfDocument::open(doc.build()).unwrap()
}

fn dest(index: usize, rest: &str) -> String {
    format!("/Dest [{} {rest}]", page_ref(index))
}

fn item(title: &str, level: u32, page: Option<usize>) -> OutlineItem {
    OutlineItem {
        title: title.to_string(),
        level,
        page,
    }
}

fn flat_outline(doc: &mut Doc, items: &[(&str, &str)]) -> Vec<OutlineItem> {
    let items: Vec<(&str, u32, &str)> = items.iter().map(|&(t, d)| (t, 0, d)).collect();
    open(doc.outline(&items)).outline()
}

fn pages_of(outline: &[OutlineItem]) -> Vec<Option<usize>> {
    outline.iter().map(|i| i.page).collect()
}

fn titles_of(outline: &[OutlineItem]) -> Vec<&str> {
    outline.iter().map(|i| i.title.as_str()).collect()
}

fn links_of(mut doc: Doc, annots: &[String]) -> Vec<Link> {
    doc.page_entry(0, &format!("/Annots [{}]", annots.join(" ")));
    open(&doc).links(0).unwrap()
}

fn targets_of(links: &[Link]) -> Vec<LinkTarget> {
    links.iter().map(|l| l.target.clone()).collect()
}

fn to_page(index: usize, top: Option<f32>) -> LinkTarget {
    LinkTarget::Page { index, top }
}

fn uri(target: &str) -> String {
    format!("/A << /S /URI /URI {target} >>")
}

fn rect(x: f32, y: f32, width: f32, height: f32) -> PageRect {
    PageRect { x, y, width, height }
}

#[test]
fn a_nested_outline_is_read_depth_first_with_levels_and_pages() {
    let mut doc = pages(3);
    let targets = [
        dest(0, "/Fit"),
        dest(1, "/XYZ 0 100 0"),
        format!("/A << /S /GoTo /D [{} /FitH 50] >>", page_ref(2)),
        dest(1, "/FitR 0 0 10 10"),
        uri("(https://example.org)"),
        "/Dest [99 0 R /Fit]".to_string(),
        "/Dest [2 /Fit]".to_string(),
        "/Dest [3 /Fit]".to_string(),
        "/A << /S /JavaScript /JS (app.alert\\(1\\)) >>".to_string(),
        dest(0, ""),
    ];
    doc.outline(&[
        ("(Intro)", 0, &targets[0]),
        ("(Part)", 0, &targets[1]),
        ("(Chapter)", 1, &targets[2]),
        ("(Section)", 2, &targets[3]),
        ("(Web)", 1, &targets[4]),
        ("(Missing)", 0, &targets[5]),
        ("(Numbered)", 0, &targets[6]),
        ("(Past the end)", 0, &targets[7]),
        ("(Script)", 0, &targets[8]),
        ("(Bare)", 0, &targets[9]),
        ("(Nowhere)", 0, ""),
    ]);
    assert_eq!(
        open(&doc).outline(),
        [
            item("Intro", 0, Some(0)),
            item("Part", 0, Some(1)),
            item("Chapter", 1, Some(2)),
            item("Section", 2, Some(1)),
            item("Web", 1, None),
            item("Missing", 0, None),
            item("Numbered", 0, Some(2)),
            item("Past the end", 0, None),
            item("Script", 0, None),
            item("Bare", 0, Some(0)),
            item("Nowhere", 0, None),
        ]
    );
}

#[test]
fn named_destinations_are_looked_up_in_the_name_tree_and_the_dests_dictionary() {
    let mut doc = pages(3);
    // A name tree of two leaves whose second one points back to the root.
    let (root, leaf, back) = (doc.ahead(0), doc.ahead(1), doc.ahead(2));
    doc.object(&format!("<< /Kids [{leaf} {back}] >>"));
    doc.object(&format!(
        "<< /Names [(chap) [{} /Fit] (dict) << /D [{} /Fit] >> (bad) 5] >>",
        page_ref(2),
        page_ref(1)
    ));
    let indirect = doc.ahead(1);
    doc.object(&format!("<< /Kids [{root}] /Names [(deep) {indirect}] >>"));
    doc.object(&format!("[{} /Fit]", page_ref(0)));
    doc.catalog(&format!("/Names << /Dests {root} >>"));
    doc.catalog(&format!(
        "/Dests << /old [{} /Fit] /olddict << /D [{} /Fit] >> >>",
        page_ref(1),
        page_ref(2)
    ));
    let outline = flat_outline(
        &mut doc,
        &[
            ("(a)", "/Dest (chap)"),
            ("(b)", "/A << /S /GoTo /D (dict) >>"),
            ("(c)", "/Dest (deep)"),
            ("(d)", "/Dest /old"),
            ("(e)", "/A << /S /GoTo /D /olddict >>"),
            ("(f)", "/Dest (old)"),
            ("(g)", "/Dest /missing"),
            ("(h)", "/Dest (bad)"),
            ("(i)", "/Dest 5"),
        ],
    );
    assert_eq!(
        pages_of(&outline),
        [
            Some(2),
            Some(1),
            Some(0),
            Some(1),
            Some(2),
            Some(1),
            None,
            None,
            None
        ]
    );
}

#[test]
fn named_destinations_are_bounded() {
    let mut doc = pages(1);
    let names: Vec<String> = (0..=MAX_NAMED_DESTS)
        .map(|i| format!("(n{i}) [{} /Fit]", page_ref(0)))
        .collect();
    doc.catalog(&format!("/Names << /Dests << /Names [{}] >> >>", names.join(" ")));
    let (last, over) = (MAX_NAMED_DESTS - 1, MAX_NAMED_DESTS);
    let outline = flat_outline(
        &mut doc,
        &[
            ("(last)", &format!("/Dest (n{last})")),
            ("(over)", &format!("/Dest (n{over})")),
        ],
    );
    assert_eq!(pages_of(&outline), [Some(0), None]);
}

#[test]
fn titles_are_decoded_trimmed_and_cut() {
    let mut doc = pages(1);
    let long = format!("({})", "a".repeat(3000));
    let outline = flat_outline(
        &mut doc,
        &[
            ("<FEFF00480069D83DDE00>", ""),
            ("<FEFF0041D800>", ""),
            ("<FEFF004100>", ""),
            ("<EFBBBFC3A9>", ""),
            ("<801F92A0E9>", ""),
            ("(  Spaced\\r\\n out\\t )", ""),
            (&long, ""),
            ("5", ""),
        ],
    );
    let a = "a".repeat(2048);
    assert_eq!(
        titles_of(&outline),
        ["Hi😀", "A\u{FFFD}", "A", "é", "•˜™€é", "Spaced out", &a, ""]
    );
}

#[test]
fn cycles_in_the_outline_end() {
    let mut doc = pages(1);
    let (a, b, c) = (doc.ahead(0), doc.ahead(1), doc.ahead(2));
    doc.object(&format!("<< /Title (A) /First {c} /Next {b} >>"));
    doc.object(&format!("<< /Title (B) /Next {a} /First {a} >>"));
    doc.object(&format!("<< /Title (C) /First {c} /Next {a} >>"));
    doc.catalog(&format!("/Outlines << /First {a} >>"));
    assert_eq!(
        open(&doc).outline(),
        [item("A", 0, None), item("C", 1, None), item("B", 0, None)]
    );
}

#[test]
fn the_outline_is_bounded_in_items_and_depth() {
    let titles: Vec<String> = (0..MAX_OUTLINE_ITEMS + 5).map(|i| format!("({i})")).collect();
    let items: Vec<(&str, &str)> = titles.iter().map(|t| (t.as_str(), "")).collect();
    let outline = flat_outline(&mut pages(1), &items);
    assert_eq!(outline.len(), MAX_OUTLINE_ITEMS);
    assert_eq!(
        outline[MAX_OUTLINE_ITEMS - 1].title,
        (MAX_OUTLINE_ITEMS - 1).to_string()
    );

    let deep: Vec<(&str, u32, &str)> = (0..MAX_OUTLINE_DEPTH + 8).map(|l| ("(x)", l, "")).collect();
    let outline = open(pages(1).outline(&deep)).outline();
    let levels: Vec<u32> = outline.iter().map(|i| i.level).collect();
    assert_eq!(levels, (0..MAX_OUTLINE_DEPTH).collect::<Vec<_>>());
}

#[test]
fn documents_without_an_outline_have_an_empty_one() {
    assert_eq!(open(&pages(1)).outline(), []);
    assert_eq!(open(pages(1).catalog("/Outlines << /Count 0 >>")).outline(), []);
    assert_eq!(open(pages(1).catalog("/Outlines << /First 5 >>")).outline(), []);
}

#[test]
fn links_on_an_upright_page_map_to_the_shown_page() {
    let mut doc = pages(3);
    doc.page_entry(2, "/CropBox [64 32 192 96]");
    let links = links_of(
        doc,
        &[
            link([64, 32, 128, 96], &dest(1, "/XYZ null 96 null")),
            link([192, -64, 512, 32], &dest(1, "/XYZ 10 null 0")),
            link([0, 0, 1, 1], &dest(1, "/FitH 32")),
            link([0, 0, 1, 1], &dest(1, "/FitV 10")),
            link([0, 0, 1, 1], &dest(1, "/FitR 0 0 100 64")),
            link([0, 0, 1, 1], &dest(1, "/XYZ 0 1000 0")),
            link([0, 0, 1, 1], &dest(1, "/FitBH -50")),
            link([0, 0, 1, 1], &dest(2, "/XYZ 0 80 0")),
            link([0, 0, 1, 1], &dest(0, "/Fit")),
        ],
    );
    assert_eq!(links[0].area, rect(0.25, 0.25, 0.25, 0.5));
    assert_eq!(links[1].area, rect(0.75, 0.75, 0.25, 0.25), "clamped to the page");
    assert_eq!(
        targets_of(&links),
        [
            to_page(1, Some(0.25)),
            to_page(1, None),
            to_page(1, Some(0.75)),
            to_page(1, None),
            to_page(1, Some(0.5)),
            to_page(1, Some(0.0)),
            to_page(1, Some(1.0)),
            to_page(2, Some(0.25)),
            to_page(0, None),
        ]
    );
}

#[test]
fn links_on_a_cropped_page_map_to_the_crop_box() {
    let mut doc = pages(1);
    doc.page_entry(0, "/CropBox [64 32 192 96]");
    let links = links_of(doc, &[link([96, 48, 128, 64], &dest(0, "/Fit"))]);
    assert_eq!(links[0].area, rect(0.25, 0.5, 0.25, 0.25));
}

#[test]
fn links_on_a_rotated_page_map_to_the_shown_page() {
    let turned = Spec {
        w: 128,
        h: 256,
        rotate: 90,
        content: String::new(),
    };
    // Shown 256 x 128: the page's y runs left to right, its x top to bottom.
    let links = links_of(
        Doc::new(vec![turned]),
        &[
            link([32, 64, 96, 128], &dest(0, "/XYZ 32 200 null")),
            link([0, 0, 1, 1], &dest(0, "/XYZ null 200 0")),
            link([0, 0, 1, 1], &dest(0, "/FitV 64")),
            link([0, 0, 1, 1], &dest(0, "/FitH 100")),
        ],
    );
    assert_eq!(links[0].area, rect(0.25, 0.25, 0.25, 0.5));
    assert_eq!(
        targets_of(&links),
        [
            to_page(0, Some(0.25)),
            to_page(0, None),
            to_page(0, Some(0.5)),
            to_page(0, None),
        ]
    );
}

#[test]
fn only_http_and_https_uris_are_kept() {
    let longest = format!("https://{}", "a".repeat(MAX_URI_BYTES - 8));
    let annots: Vec<String> = [
        uri("(https://example.org/a?b=c)"),
        uri("(HTTP://EXAMPLE.ORG)"),
        uri("( http://padded.example\\n)"),
        uri(&format!("({longest})")),
        uri(&format!("({longest}a)")),
        uri("(javascript:alert\\(1\\))"),
        uri("(file:///etc/passwd)"),
        uri("(ftp://example.org)"),
        uri("(http://a b)"),
        uri("<68747470733A2F2FC3A9>"),
        uri("(https)"),
        uri("5"),
        "/A << /S /Launch /F (calc.exe) >>".to_string(),
        "/A << /S /JavaScript /JS (app.alert\\(1\\)) >>".to_string(),
        "/A << /S /GoToR /F (other.pdf) /D [0 /Fit] >>".to_string(),
        "/A << /URI (https://no.action.type) >>".to_string(),
    ]
    .iter()
    .map(|target| link([0, 0, 10, 10], target))
    .collect();
    assert_eq!(
        targets_of(&links_of(pages(1), &annots)),
        [
            LinkTarget::Uri("https://example.org/a?b=c".to_string()),
            LinkTarget::Uri("HTTP://EXAMPLE.ORG".to_string()),
            LinkTarget::Uri("http://padded.example".to_string()),
            LinkTarget::Uri(longest),
        ]
    );
}

#[test]
fn only_link_annotations_with_an_area_and_a_target_count() {
    let mut doc = pages(1);
    let target = dest(0, "/Fit");
    let indirect = doc.object(&link([0, 0, 64, 64], &target));
    let links = links_of(
        doc,
        &[
            format!("<< /Subtype /Text /Rect [0 0 10 10] {target} >>"),
            link([0, 0, 10, 10], ""),
            format!("<< /Subtype /Link {target} >>"),
            link([300, 0, 400, 10], &target),
            "null".to_string(),
            indirect,
            "99 0 R".to_string(),
            link([0, 0, 10, 10], &format!("/Dest null {}", uri("(https://x.org)"))),
        ],
    );
    assert_eq!(
        links,
        [Link {
            area: rect(0.0, 0.5, 0.25, 0.5),
            target: to_page(0, None)
        }]
    );
}

#[test]
fn links_are_bounded_per_page() {
    let annots = vec![link([0, 0, 10, 10], &dest(0, "/Fit")); MAX_LINKS + 1];
    assert_eq!(links_of(pages(1), &annots).len(), MAX_LINKS);
}

#[test]
fn links_of_missing_pages_fail_and_bare_pages_have_none() {
    let doc = open(&pages(1));
    assert_eq!(doc.links(0), Ok(Vec::new()));
    assert_eq!(doc.links(1), Err(PdfError::NoSuchPage));
    assert_eq!(doc.links(usize::MAX), Err(PdfError::NoSuchPage));
}
