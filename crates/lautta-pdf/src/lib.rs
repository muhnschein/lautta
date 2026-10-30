// SPDX-License-Identifier: LGPL-2.1-or-later
//! PDF pages as pixels, via [hayro](https://github.com/LaurenzV/hayro) (pure Rust). Works on given
//! bytes only: no file, network or process access, no scripts, links and actions not followed.
//! Standard fonts and CMaps compiled in.

use hayro::hayro_interpret::InterpreterSettings;
use hayro::hayro_syntax::{LoadPdfError, Pdf};
use hayro::vello_cpu::color::palette::css::WHITE;
use hayro::{render, PixmapSettings, RenderCache, RenderSettings};
use std::fmt;

mod nav;
pub use nav::{Link, LinkTarget, OutlineItem};

pub const MAX_PDF_BYTES: u64 = 200_000_000;
pub const MAX_EDGE: u32 = 4096;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PdfError {
    /// Not a PDF, or damaged beyond repair.
    Invalid,
    /// Password-protected.
    Encrypted,
    Empty,
    NoSuchPage,
    TooLarge,
}

impl fmt::Display for PdfError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(match self {
            PdfError::Invalid => "not a readable PDF",
            PdfError::Encrypted => "the PDF is protected with a password",
            PdfError::Empty => "the PDF has no pages",
            PdfError::NoSuchPage => "no such page",
            PdfError::TooLarge => "the PDF is too large to show",
        })
    }
}

impl std::error::Error for PdfError {}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct PageSize {
    pub width: f32,
    pub height: f32,
}

/// As shown (rotation applied), in fractions of shown width and height from the top left; 0..=1 on
/// the page.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct PageRect {
    pub x: f32,
    pub y: f32,
    pub width: f32,
    pub height: f32,
}

/// Premultiplied RGBA, 4 bytes per pixel, rows top to bottom, no padding. Drawn on white: all
/// pixels opaque.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RenderedPage {
    pub width: u32,
    pub height: u32,
    pub rgba: Vec<u8>,
}

pub struct PdfDocument {
    pdf: Pdf,
    sizes: Vec<PageSize>,
}

impl fmt::Debug for PdfDocument {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("PdfDocument")
            .field("pages", &self.sizes.len())
            .finish()
    }
}

impl PdfDocument {
    /// Damaged cross-reference tables are repaired where possible; empty user password opens.
    pub fn open(bytes: Vec<u8>) -> Result<PdfDocument, PdfError> {
        PdfDocument::load(bytes, Pdf::new)
    }

    /// `password` may be user or owner password. [`PdfError::Encrypted`] when it does not open it.
    pub fn open_with_password(bytes: Vec<u8>, password: &str) -> Result<PdfDocument, PdfError> {
        PdfDocument::load(bytes, |b| Pdf::new_with_password(b, password))
    }

    fn load(
        bytes: Vec<u8>,
        parse: impl FnOnce(Vec<u8>) -> Result<Pdf, LoadPdfError>,
    ) -> Result<PdfDocument, PdfError> {
        if bytes.len() as u64 > MAX_PDF_BYTES {
            return Err(PdfError::TooLarge);
        }
        let pdf = parse(bytes).map_err(|e| match e {
            LoadPdfError::Decryption(_) => PdfError::Encrypted,
            _ => PdfError::Invalid,
        })?;
        let sizes: Vec<PageSize> = pdf
            .pages()
            .iter()
            .map(|p| {
                let (width, height) = p.render_dimensions();
                PageSize { width, height }
            })
            .collect();
        if sizes.is_empty() {
            return Err(PdfError::Empty);
        }
        Ok(PdfDocument { pdf, sizes })
    }

    pub fn page_count(&self) -> usize {
        self.sizes.len()
    }

    pub fn page_sizes(&self) -> &[PageSize] {
        &self.sizes
    }

    /// `width` pixels wide, aspect ratio kept; smaller when an edge would exceed [`MAX_EDGE`].
    pub fn render(&self, index: usize, width: u32) -> Result<RenderedPage, PdfError> {
        let size = *self.sizes.get(index).ok_or(PdfError::NoSuchPage)?;
        let page = self.pdf.pages().get(index).ok_or(PdfError::NoSuchPage)?;
        let scale = render_scale(size, width);
        let cache = RenderCache::new();
        let pixmap = render(
            page,
            &cache,
            // default draws widgets and links as on paper; nothing clickable
            &InterpreterSettings::default(),
            &RenderSettings::default(),
            &PixmapSettings {
                x_scale: scale,
                y_scale: scale,
                bg_color: WHITE,
            },
        );
        Ok(RenderedPage {
            width: u32::from(pixmap.width()),
            height: u32::from(pixmap.height()),
            rgba: pixmap.data_as_u8_slice().to_vec(),
        })
    }
}

/// Page ends up `width` pixels wide, reduced so no edge exceeds [`MAX_EDGE`]. Degenerate pages
/// count as at least one point per edge.
pub fn render_scale(size: PageSize, width: u32) -> f32 {
    let w = f64::from(size.width.max(1.0));
    let h = f64::from(size.height.max(1.0));
    let want = f64::from(width.clamp(1, MAX_EDGE)) / w;
    let edge = f64::from(MAX_EDGE) / w.max(h);
    want.min(edge) as f32
}

mod text;
pub use text::{TextHit, MAX_PAGE_CHARS, MAX_PAGE_HITS};

#[cfg(test)]
mod nav_tests;
#[cfg(any(test, feature = "sample"))]
pub mod sample;
#[cfg(test)]
mod tests;
