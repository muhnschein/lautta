// SPDX-License-Identifier: LGPL-2.1-or-later
//! Text search. The interpreter runs with a device that only keeps each
//! glyph's Unicode text and an approximate box; paths, images and Type 3
//! glyph programs are skipped, nothing is decoded or drawn.

use crate::{interpreter_settings, PageRect, PageSize, PdfDocument, PdfError};
use hayro::hayro_interpret::font::{Glyph, GlyphRun};
use hayro::hayro_interpret::hayro_cmap::BfString;
use hayro::hayro_interpret::hayro_syntax::page::Page;
use hayro::hayro_interpret::{
    interpret_page, BlendMode, ClipPath, Context, Device, DrawMode, DrawProps, Image, ImageDrawProps,
    InterpreterCache, SoftMask, TransformExt,
};
use hayro::kurbo::{Affine, BezPath, Point, Rect};

/// Most characters read from one page; text after them is not searched.
pub const MAX_PAGE_CHARS: usize = 200_000;
/// Most hits reported for one page.
pub const MAX_PAGE_HITS: usize = 1_000;

// Lengths in glyph space: 1000 units per em.
/// Bottom of a glyph's box: an approximate descender.
const DESCENT: f64 = -200.0;
/// Top of a glyph's box: an approximate ascender.
const ASCENT: f64 = 800.0;
/// Width of a glyph whose font gives none (Type 3 fonts).
const DEFAULT_ADVANCE: f32 = 500.0;
/// A wider gap after a glyph separates words.
const WORD_GAP: f64 = 150.0;
/// A baseline moved further up or down starts a new line.
const LINE_SHIFT: f64 = 500.0;
/// A step back further than this starts a new line too.
const LINE_BACK: f64 = 1000.0;

/// One occurrence of the query on a page: the areas of its characters, one
/// per line it spans.
#[derive(Debug, Clone, PartialEq)]
pub struct TextHit {
    pub areas: Vec<PageRect>,
}

impl PdfDocument {
    /// Finds `query` on page `index` (0-based), in content stream order.
    /// Case-insensitive (Unicode lowercase); a whitespace run in the query
    /// matches any whitespace run in the text, leading and trailing
    /// whitespace is ignored. An empty query finds nothing. At most
    /// [`MAX_PAGE_HITS`] hits in the first [`MAX_PAGE_CHARS`] characters.
    pub fn search_page(&self, index: usize, query: &str) -> Result<Vec<TextHit>, PdfError> {
        let size = *self.sizes.get(index).ok_or(PdfError::NoSuchPage)?;
        let page = self.pdf.pages().get(index).ok_or(PdfError::NoSuchPage)?;
        let mut needle = String::new();
        fold(query.trim().chars(), |c, _| needle.push(c));
        if needle.is_empty() {
            return Ok(Vec::new());
        }
        Ok(find(&page_text(page), &needle, size))
    }
}

/// A character of a page, in shown page points. The spaces put between
/// words and lines have no area.
#[derive(Debug, Clone, Copy, PartialEq)]
struct TextChar {
    ch: char,
    area: Option<Rect>,
    line: u64,
}

/// The page's characters in content stream order.
fn page_text(page: &Page<'_>) -> Vec<TextChar> {
    let (width, height) = page.render_dimensions();
    let cache = InterpreterCache::new();
    let mut context = Context::new(
        page.initial_transform(true).to_kurbo(),
        Rect::new(0.0, 0.0, f64::from(width), f64::from(height)),
        &cache,
        page.xref(),
        interpreter_settings(),
    );
    let mut sink = TextSink::default();
    interpret_page(page, &mut context, &mut sink);
    sink.chars
}

/// The last glyph read: the map from page back to its glyph space, and its
/// width there.
#[derive(Debug, Clone, Copy)]
struct Placed {
    inverse: Affine,
    advance: f64,
}

/// What lies between two glyphs.
#[derive(Debug, PartialEq)]
enum Gap {
    None,
    Word,
    Line,
}

/// The gap between the `last` glyph and the next one, drawn at `origin`.
fn gap(last: Placed, origin: Point) -> Gap {
    let at = last.inverse * origin;
    let along = at.x - last.advance;
    // Written so that a degenerate transform (NaN) breaks the line.
    let same_line = at.y.abs() <= LINE_SHIFT && along >= -LINE_BACK;
    if !same_line {
        Gap::Line
    } else if along > WORD_GAP {
        Gap::Word
    } else {
        Gap::None
    }
}

/// A glyph's width in glyph space.
fn advance(glyph: &Glyph<'_>) -> f64 {
    let width = match glyph {
        Glyph::Outline(g) => g.advance_width(),
        Glyph::Type3(_) => None,
    };
    f64::from(width.unwrap_or(DEFAULT_ADVANCE))
}

/// A device that collects the text of the glyphs drawn on it.
#[derive(Default)]
struct TextSink {
    chars: Vec<TextChar>,
    line: u64,
    last: Option<Placed>,
    last_run: Vec<Affine>,
}

impl TextSink {
    fn push(&mut self, ch: char, area: Option<Rect>) {
        if self.chars.len() < MAX_PAGE_CHARS {
            self.chars.push(TextChar {
                ch,
                area,
                line: self.line,
            });
        }
    }

    /// Adds a glyph drawn with `transform` from glyph space to the page.
    fn glyph(&mut self, glyph: &Glyph<'_>, transform: Affine) {
        let gap = self.last.map(|last| gap(last, transform * Point::ORIGIN));
        if gap == Some(Gap::Line) {
            self.line += 1;
        }
        if matches!(gap, Some(Gap::Word | Gap::Line)) {
            self.push(' ', None);
        }
        let advance = advance(glyph);
        let area = transform.transform_rect_bbox(Rect::new(0.0, DESCENT, advance, ASCENT));
        if let Some(text) = glyph.as_unicode() {
            self.push_text(&text, area);
        }
        self.last = Some(Placed {
            inverse: transform.inverse(),
            advance,
        });
    }

    /// Adds a glyph's text; the characters of a ligature share its area.
    fn push_text(&mut self, text: &BfString, area: Rect) {
        let mut buf = [0; 4];
        let text = match text {
            BfString::Char(c) => c.encode_utf8(&mut buf),
            BfString::String(s) => s.as_str(),
        };
        for ch in text.chars() {
            self.push(ch, Some(area));
        }
    }
}

impl<'a> Device<'a> for TextSink {
    fn draw_glyph_run(&mut self, run: &GlyphRun<'_, 'a>, props: DrawProps<'a>, _: &DrawMode) {
        let transforms: Vec<Affine> = run
            .glyphs()
            .iter()
            .map(|g| props.transform * g.transform())
            .collect();
        // Fill and stroke draws a run twice; overprinted (fake bold) text
        // repeats it too.
        if transforms != self.last_run {
            for (glyph, transform) in run.glyphs().iter().zip(&transforms) {
                self.glyph(glyph, *transform);
            }
            self.last_run = transforms;
        }
    }

    fn draw_path(&mut self, _: &BezPath, _: DrawProps<'a>, _: &DrawMode) {}
    fn draw_rect(&mut self, _: &Rect, _: DrawProps<'a>, _: &DrawMode) {}
    fn draw_image(&mut self, _: Image<'a, '_>, _: ImageDrawProps<'a>) {}
    fn push_clip_path(&mut self, _: &ClipPath) {}
    fn push_clip_rect(&mut self, _: &Rect) {}
    fn pop_clip(&mut self) {}
    fn push_transparency_group(&mut self, _: f32, _: Option<SoftMask<'a>>, _: BlendMode) {}
    fn pop_transparency_group(&mut self) {}
}

/// Lowercases `chars` and turns each whitespace run into one space, passing
/// on every resulting char with the index of the char it comes from.
fn fold(chars: impl Iterator<Item = char>, mut out: impl FnMut(char, usize)) {
    let mut after_space = false;
    for (i, c) in chars.enumerate() {
        let space = c.is_whitespace();
        if !space {
            c.to_lowercase().for_each(|l| out(l, i));
        } else if !after_space {
            out(' ', i);
        }
        after_space = space;
    }
}

/// The hits of `needle` (folded) in `chars` on a page of `size` points.
fn find(chars: &[TextChar], needle: &str, size: PageSize) -> Vec<TextHit> {
    let mut text = String::new();
    // Per folded char: where it starts in `text`, and its char in `chars`.
    let mut starts = Vec::new();
    let mut sources = Vec::new();
    fold(chars.iter().map(|c| c.ch), |c, i| {
        starts.push(text.len());
        sources.push(i);
        text.push(c);
    });
    let len = needle.chars().count();
    text.match_indices(needle)
        .take(MAX_PAGE_HITS)
        .map(|(at, _)| {
            let first = starts.partition_point(|&s| s < at);
            let hit = sources[first..first + len].iter().map(|&i| chars[i]);
            TextHit {
                areas: areas(hit, size),
            }
        })
        .collect()
}

/// The areas of a hit's characters, consecutive ones on a line merged.
fn areas(hit: impl Iterator<Item = TextChar>, size: PageSize) -> Vec<PageRect> {
    let mut lines: Vec<(u64, Rect)> = Vec::new();
    for c in hit {
        let Some(area) = c.area else { continue };
        match lines.last_mut() {
            Some((line, rect)) if *line == c.line => *rect = rect.union(area),
            _ => lines.push((c.line, area)),
        }
    }
    lines.into_iter().map(|(_, r)| fraction(r, size)).collect()
}

/// `r`, in points, as fractions of a page of `size` points.
fn fraction(r: Rect, size: PageSize) -> PageRect {
    PageRect {
        x: r.x0 as f32 / size.width,
        y: r.y0 as f32 / size.height,
        width: r.width() as f32 / size.width,
        height: r.height() as f32 / size.height,
    }
}

#[cfg(test)]
#[path = "text_tests.rs"]
mod tests;
