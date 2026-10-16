// SPDX-License-Identifier: LGPL-2.1-or-later
//! Navigation data of an open PDF: the outline (bookmarks) and the links of
//! a page. Only page destinations and http(s) URIs are read; every other
//! action (Launch, JavaScript, GoToR, file: URIs, ...) is dropped, so a PDF
//! cannot cause file or network access. Nothing here does I/O.

use crate::{PageRect, PdfDocument, PdfError};
use hayro::hayro_syntax::object::{Array, Dict, MaybeRef, Name, Null, ObjRef, Object};
use hayro::hayro_syntax::object::{Rect as PdfRect, String as PdfString};
use hayro::hayro_syntax::page::{Page, Pages};
use hayro::hayro_syntax::xref::XRef;
use hayro::hayro_syntax::Pdf;
use hayro::kurbo::{Affine, Point, Rect};
use std::cell::OnceCell;
use std::collections::{HashMap, HashSet};

/// Deepest outline level read; deeper items are left out.
pub(crate) const MAX_OUTLINE_DEPTH: u32 = 32;
/// Most outline items read.
pub(crate) const MAX_OUTLINE_ITEMS: usize = 10_000;
/// Most links read on one page.
pub(crate) const MAX_LINKS: usize = 1_000;
/// Longest URI kept, in bytes.
pub(crate) const MAX_URI_BYTES: usize = 2048;
/// Most named destinations read.
pub(crate) const MAX_NAMED_DESTS: usize = 100_000;
/// Longest title read, in bytes of the PDF string; the rest is cut.
pub(crate) const MAX_TITLE_BYTES: usize = 2048;

/// One entry of the document outline (bookmarks), depth first.
#[derive(Debug, Clone, PartialEq)]
pub struct OutlineItem {
    pub title: String,
    /// 0 for the top level.
    pub level: u32,
    /// 0-based target page; `None` when the target cannot be resolved.
    pub page: Option<usize>,
}

/// Where a link goes.
#[derive(Debug, Clone, PartialEq)]
pub enum LinkTarget {
    /// Page `index` (0-based); `top` is the fraction of the shown page's
    /// height from its top, when the destination gives one.
    Page { index: usize, top: Option<f32> },
    /// An `http` or `https` URI, for the user's browser after they confirm.
    Uri(String),
}

/// A link of a page: its area on the shown page and its target.
#[derive(Debug, Clone, PartialEq)]
pub struct Link {
    pub area: PageRect,
    pub target: LinkTarget,
}

impl PdfDocument {
    /// The outline, depth first. Empty when there is none.
    pub fn outline(&self) -> Vec<OutlineItem> {
        Nav::new(&self.pdf).outline()
    }

    /// The links of page `index` (0-based) that go to a page of this
    /// document or to an http(s) URI.
    pub fn links(&self, index: usize) -> Result<Vec<Link>, PdfError> {
        let page = self.pdf.pages().get(index).ok_or(PdfError::NoSuchPage)?;
        Ok(Nav::new(&self.pdf).links(page))
    }
}

/// A resolved page destination, in the target page's user space.
struct Dest {
    page: usize,
    left: Option<f64>,
    top: Option<f64>,
}

enum Target {
    Dest(Dest),
    Uri(String),
}

/// Reads destinations and actions of one document.
struct Nav<'a> {
    xref: &'a XRef,
    pages: &'a Pages<'a>,
    page_index: HashMap<ObjRef, usize>,
    catalog: Dict<'a>,
    named: OnceCell<NamedDests<'a>>,
}

impl<'a> Nav<'a> {
    fn new(pdf: &'a Pdf) -> Nav<'a> {
        let pages = pdf.pages();
        let page_index = pages
            .iter()
            .enumerate()
            .filter_map(|(i, p)| Some((ObjRef::from(p.raw().obj_id()?), i)))
            .collect();
        let xref = pdf.xref();
        Nav {
            xref,
            pages,
            page_index,
            catalog: xref.get(xref.root_id()).unwrap_or_default(),
            named: OnceCell::new(),
        }
    }

    fn outline(&self) -> Vec<OutlineItem> {
        let mut items = Vec::new();
        let mut visited = HashSet::new();
        let mut stack = Vec::new();
        let outlines = self.catalog.get::<Dict<'a>>(b"Outlines").unwrap_or_default();
        stack.extend(self.linked(&outlines, b"First", &mut visited).map(|d| (d, 0)));
        while items.len() < MAX_OUTLINE_ITEMS {
            let Some((item, level)) = stack.pop() else {
                break;
            };
            // The sibling goes below the first child, so children come first.
            stack.extend(self.linked(&item, b"Next", &mut visited).map(|d| (d, level)));
            if level + 1 < MAX_OUTLINE_DEPTH {
                stack.extend(self.linked(&item, b"First", &mut visited).map(|d| (d, level + 1)));
            }
            items.push(OutlineItem {
                title: item
                    .get::<PdfString<'_>>(b"Title")
                    .map_or_else(String::new, |t| text(&t)),
                level,
                page: match self.target(&item) {
                    Some(Target::Dest(dest)) => Some(dest.page),
                    _ => None,
                },
            });
        }
        items
    }

    fn links(&self, page: &Page<'a>) -> Vec<Link> {
        let shown = Shown::of(page);
        let annots = page.raw().get::<Array<'a>>(b"Annots").unwrap_or_default();
        annots
            .raw_iter()
            .filter_map(|a| self.resolve(a)?.into_dict())
            .filter(|a| a.get::<Name<'_>>(b"Subtype").as_deref() == Some(b"Link".as_slice()))
            .filter_map(|a| self.link(&shown, &a))
            .take(MAX_LINKS)
            .collect()
    }

    fn link(&self, shown: &Shown, annot: &Dict<'a>) -> Option<Link> {
        let area = shown.area(annot.get::<PdfRect>(b"Rect")?)?;
        let target = match self.target(annot)? {
            Target::Uri(uri) => LinkTarget::Uri(uri),
            Target::Dest(dest) => LinkTarget::Page {
                index: dest.page,
                top: Shown::of(&self.pages[dest.page]).top(dest.left, dest.top),
            },
        };
        Some(Link { area, target })
    }

    /// The target of an outline item or link: `/Dest`, else `/A`.
    fn target(&self, dict: &Dict<'a>) -> Option<Target> {
        if let Some(dest) = dict.get::<Object<'a>>(b"Dest") {
            return self.dest(dest).map(Target::Dest);
        }
        let action = dict.get::<Dict<'a>>(b"A")?;
        match &*action.get::<Name<'_>>(b"S")? {
            b"GoTo" => self.dest(action.get(b"D")?).map(Target::Dest),
            b"URI" => web_uri(&action.get::<PdfString<'_>>(b"URI")?).map(Target::Uri),
            _ => None,
        }
    }

    /// An explicit destination, or a name or string naming one.
    fn dest(&self, dest: Object<'a>) -> Option<Dest> {
        let array = match dest {
            Object::Array(array) => array,
            Object::Name(name) => self.named(&name)?,
            Object::String(string) => self.named(&string)?,
            _ => return None,
        };
        let mut items = array.raw_iter();
        let page = self.page_of(items.next()?)?;
        let kind = items.next().and_then(|k| self.resolve(k)?.into_name());
        let mut nums = [None; 4];
        for (num, item) in nums.iter_mut().zip(items) {
            *num = self.resolve(item).and_then(Object::into_f32).map(f64::from);
        }
        let (left, top) = match kind.as_deref() {
            Some(b"XYZ") => (nums[0], nums[1]),
            Some(b"FitH" | b"FitBH") => (None, nums[0]),
            Some(b"FitV" | b"FitBV") => (nums[0], None),
            Some(b"FitR") => (nums[0], nums[3]),
            _ => (None, None),
        };
        Some(Dest { page, left, top })
    }

    /// The page of a destination: a page reference, or (in some broken
    /// files) a 0-based page number.
    fn page_of(&self, item: MaybeRef<Object<'a>>) -> Option<usize> {
        match item {
            MaybeRef::Ref(r) => self.page_index.get(&r).copied(),
            MaybeRef::NotRef(o) => usize::try_from(o.into_i32()?)
                .ok()
                .filter(|&i| i < self.pages.len()),
        }
    }

    fn named(&self, key: &[u8]) -> Option<Array<'a>> {
        let named = self.named.get_or_init(|| self.named_dests());
        match named.0.get(key)?.clone() {
            Object::Dict(dict) => dict.get(b"D"),
            other => other.into_array(),
        }
    }

    /// The `/Dests` dictionary (PDF 1.1) and the `/Names /Dests` name tree.
    fn named_dests(&self) -> NamedDests<'a> {
        let mut named = NamedDests::default();
        let dests = self.catalog.get::<Dict<'a>>(b"Dests").unwrap_or_default();
        for key in dests.keys() {
            let value = dests.get(&*key).unwrap_or(Object::Null(Null));
            if !named.add(&key, value) {
                return named;
            }
        }
        let names = self.catalog.get::<Dict<'a>>(b"Names").unwrap_or_default();
        let mut stack: Vec<Dict<'a>> = names.get(b"Dests").into_iter().collect();
        let mut visited = HashSet::new();
        while let Some(node) = stack.pop() {
            if !self.add_leaf(&node, &mut named) {
                break;
            }
            let kids = node.get::<Array<'a>>(b"Kids").unwrap_or_default();
            stack.extend(kids.raw_iter().filter_map(|k| self.unvisited(k, &mut visited)));
        }
        named
    }

    /// Adds the `/Names [key value ...]` of a name tree node; false once full.
    fn add_leaf(&self, node: &Dict<'a>, named: &mut NamedDests<'a>) -> bool {
        let names = node.get::<Array<'a>>(b"Names").unwrap_or_default();
        let mut items = names.raw_iter().map(|i| self.resolve(i));
        while let (Some(key), Some(value)) = (items.next(), items.next()) {
            let Some(Object::String(key)) = key else {
                continue;
            };
            if !named.add(&key, value.unwrap_or(Object::Null(Null))) {
                return false;
            }
        }
        true
    }

    /// The dictionary under `key`, unless it is a reference seen before.
    fn linked(&self, dict: &Dict<'a>, key: &[u8], visited: &mut HashSet<ObjRef>) -> Option<Dict<'a>> {
        self.unvisited(dict.get_raw(key)?, visited)
    }

    fn unvisited(&self, item: MaybeRef<Object<'a>>, visited: &mut HashSet<ObjRef>) -> Option<Dict<'a>> {
        if let MaybeRef::Ref(r) = item {
            if !visited.insert(r) {
                return None;
            }
        }
        self.resolve(item)?.into_dict()
    }

    fn resolve(&self, item: MaybeRef<Object<'a>>) -> Option<Object<'a>> {
        match item {
            MaybeRef::Ref(r) => self.xref.get(r.into()),
            MaybeRef::NotRef(o) => Some(o),
        }
    }
}

/// Named destinations by name, at most [`MAX_NAMED_DESTS`].
#[derive(Default)]
struct NamedDests<'a>(HashMap<Vec<u8>, Object<'a>>);

impl<'a> NamedDests<'a> {
    /// Adds a destination; false when there is no room for it.
    fn add(&mut self, key: &[u8], value: Object<'a>) -> bool {
        let room = self.0.len() < MAX_NAMED_DESTS;
        if room {
            self.0.insert(key.to_vec(), value);
        }
        room
    }
}

/// Maps a page's user space to fractions of the page as shown.
struct Shown(Affine);

impl Shown {
    fn of(page: &Page<'_>) -> Shown {
        let (w, h) = page.render_dimensions();
        let to_points = Affine::new(page.initial_transform(true).as_coeffs());
        Shown(Affine::scale_non_uniform(1.0 / f64::from(w), 1.0 / f64::from(h)) * to_points)
    }

    /// The part of `rect` on the shown page; `None` when nothing is.
    fn area(&self, rect: PdfRect) -> Option<PageRect> {
        let rect = Rect::new(rect.x0, rect.y0, rect.x1, rect.y1);
        let area = self
            .0
            .transform_rect_bbox(rect)
            .intersect(Rect::new(0.0, 0.0, 1.0, 1.0));
        (!area.is_zero_area()).then(|| PageRect {
            x: area.x0 as f32,
            y: area.y0 as f32,
            width: area.width() as f32,
            height: area.height() as f32,
        })
    }

    /// The shown height fraction of a destination's `left`/`top`. Rotated
    /// by 90 or 270 degrees, the shown vertical comes from `left`.
    fn top(&self, left: Option<f64>, top: Option<f64>) -> Option<f32> {
        let [_, from_x, _, from_y, _, _] = self.0.as_coeffs();
        let x = if from_x == 0.0 { 0.0 } else { left? };
        let y = if from_y == 0.0 { 0.0 } else { top? };
        Some((self.0 * Point::new(x, y)).y.clamp(0.0, 1.0) as f32)
    }
}

/// `uri` when it is an http or https URI of printable ASCII, at most
/// [`MAX_URI_BYTES`] long.
fn web_uri(uri: &[u8]) -> Option<String> {
    let uri = std::str::from_utf8(uri.trim_ascii()).ok()?;
    let (scheme, _) = uri.split_once(':')?;
    let web = scheme.eq_ignore_ascii_case("http") || scheme.eq_ignore_ascii_case("https");
    let clean = uri.len() <= MAX_URI_BYTES && uri.bytes().all(|b| b.is_ascii_graphic());
    (web && clean).then(|| uri.to_string())
}

/// A PDF text string (UTF-16BE or UTF-8 with a byte order mark, else
/// PDFDocEncoding) without control characters, trimmed.
fn text(bytes: &[u8]) -> String {
    let bytes = &bytes[..bytes.len().min(MAX_TITLE_BYTES)];
    let decoded: String = match bytes {
        [0xFE, 0xFF, utf16 @ ..] => {
            let units = utf16.chunks_exact(2).map(|u| u16::from_be_bytes([u[0], u[1]]));
            char::decode_utf16(units)
                .map(|c| c.unwrap_or(char::REPLACEMENT_CHARACTER))
                .collect()
        }
        [0xEF, 0xBB, 0xBF, utf8 @ ..] => String::from_utf8_lossy(utf8).into_owned(),
        _ => bytes.iter().map(|&b| pdf_doc_char(b)).collect(),
    };
    let visible: String = decoded.chars().filter(|c| !c.is_control()).collect();
    visible.trim().to_string()
}

/// PDFDocEncoding: Latin-1 but for 0x18..=0x1F and 0x80..=0xA0.
fn pdf_doc_char(b: u8) -> char {
    const ACCENTS: [char; 8] = ['˘', 'ˇ', 'ˆ', '˙', '˝', '˛', '˚', '˜'];
    const HIGH: [char; 33] = [
        '•', '†', '‡', '…', '—', '–', 'ƒ', '⁄', '‹', '›', '−', '‰', '„', '“', '”', '‘', '’', '‚', '™', 'ﬁ',
        'ﬂ', 'Ł', 'Œ', 'Š', 'Ÿ', 'Ž', 'ı', 'ł', 'œ', 'š', 'ž', '\u{FFFD}', '€',
    ];
    match b {
        0x18..=0x1F => ACCENTS[usize::from(b - 0x18)],
        0x80..=0xA0 => HIGH[usize::from(b - 0x80)],
        _ => char::from(b),
    }
}
