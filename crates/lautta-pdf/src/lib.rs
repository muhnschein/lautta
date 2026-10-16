// SPDX-License-Identifier: LGPL-2.1-or-later
//! `lautta-pdf`: PDF pages as pixels for the built-in viewer (SPEC PRV-10).
//!
//! The renderer is [hayro](https://github.com/LaurenzV/hayro), pure Rust.
//! It works on the bytes it is given and nothing else: it has no file,
//! network or process access, runs no scripts and follows no links or
//! actions. The standard fonts and CMaps are compiled in. No unsafe code
//! here (workspace lint).

use hayro::hayro_interpret::InterpreterSettings;
use hayro::hayro_syntax::{LoadPdfError, Pdf};
use hayro::vello_cpu::color::palette::css::WHITE;
use hayro::{render, PixmapSettings, RenderCache, RenderSettings};
use std::fmt;

/// Largest PDF the viewer opens: the whole file is held in memory.
pub const MAX_PDF_BYTES: u64 = 200_000_000;
/// Longest edge of a rendered page, in pixels: at most 64 MiB of RGBA.
pub const MAX_EDGE: u32 = 4096;

/// Why a PDF or a page cannot be shown.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PdfError {
    /// Not a PDF, or damaged beyond repair.
    Invalid,
    /// Encrypted with a password.
    Encrypted,
    /// The document has no pages.
    Empty,
    /// No page with that number.
    NoSuchPage,
    /// The file is larger than [`MAX_PDF_BYTES`].
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

/// A page's size in PDF points (1/72 inch), as it is shown (rotation applied).
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct PageSize {
    pub width: f32,
    pub height: f32,
}

/// An area of a page as it is shown (rotation applied), in fractions of
/// the shown width and height from its top left corner; 0..=1 when the area
/// lies on the page.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct PageRect {
    pub x: f32,
    pub y: f32,
    pub width: f32,
    pub height: f32,
}

/// A rendered page: premultiplied RGBA, 4 bytes per pixel, rows top to
/// bottom, no padding. Pages are drawn on white, so every pixel is opaque.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RenderedPage {
    pub width: u32,
    pub height: u32,
    pub rgba: Vec<u8>,
}

/// An open PDF.
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
    /// Parses a PDF. Damaged cross-reference tables are repaired where
    /// possible; documents encrypted with an empty user password open.
    pub fn open(bytes: Vec<u8>) -> Result<PdfDocument, PdfError> {
        PdfDocument::open_with_password(bytes, "")
    }

    /// Parses a PDF encrypted with `password` (the user or the owner
    /// password). [`PdfError::Encrypted`] when it does not open it.
    pub fn open_with_password(bytes: Vec<u8>, password: &str) -> Result<PdfDocument, PdfError> {
        if bytes.len() as u64 > MAX_PDF_BYTES {
            return Err(PdfError::TooLarge);
        }
        let pdf = Pdf::new_with_password(bytes, password).map_err(|e| match e {
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

    /// Every page's size, in page order.
    pub fn page_sizes(&self) -> &[PageSize] {
        &self.sizes
    }

    /// Renders page `index` (0-based) `width` pixels wide, keeping its
    /// aspect ratio. The result is smaller when an edge would exceed
    /// [`MAX_EDGE`].
    pub fn render(&self, index: usize, width: u32) -> Result<RenderedPage, PdfError> {
        let size = *self.sizes.get(index).ok_or(PdfError::NoSuchPage)?;
        let page = self.pdf.pages().get(index).ok_or(PdfError::NoSuchPage)?;
        let scale = render_scale(size, width);
        let cache = RenderCache::new();
        let settings = InterpreterSettings {
            // Widgets and links are drawn as on paper; nothing is clickable.
            render_annotations: true,
            ..InterpreterSettings::default()
        };
        let pixmap = render(
            page,
            &cache,
            &settings,
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

/// Scale from points to pixels so the page is `width` pixels wide, reduced
/// so that no edge exceeds [`MAX_EDGE`]. Degenerate pages count as at least
/// one point per edge.
pub fn render_scale(size: PageSize, width: u32) -> f32 {
    let w = f64::from(size.width.max(1.0));
    let h = f64::from(size.height.max(1.0));
    let want = f64::from(width.clamp(1, MAX_EDGE)) / w;
    let edge = f64::from(MAX_EDGE) / w.max(h);
    want.min(edge) as f32
}

#[cfg(any(test, feature = "sample"))]
pub mod sample;
#[cfg(test)]
mod tests;
