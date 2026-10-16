// SPDX-License-Identifier: LGPL-2.1-or-later
//! Small, valid PDFs built in code, for tests here and in the crates that
//! use this one (feature `sample`).

/// One page of a test PDF: media box size, `/Rotate` and content stream.
/// Every page has Helvetica (WinAnsiEncoding) as font `/F1`.
#[derive(Clone)]
pub struct Spec {
    pub w: u32,
    pub h: u32,
    pub rotate: u32,
    pub content: String,
}

/// Fills a red 50 pt square in the bottom left corner.
pub const RED_SQUARE: &str = "1 0 0 rg 0 0 50 50 re f";

/// An upright page.
pub fn page(w: u32, h: u32, content: &str) -> Spec {
    Spec {
        w,
        h,
        rotate: 0,
        content: content.to_string(),
    }
}

/// Content that shows `s` in 12 pt Helvetica, its baseline starting at
/// `x`, `y` (PDF points from the bottom left). Latin-1 letters such as ä
/// are written in WinAnsiEncoding.
pub fn text(x: u32, y: u32, s: &str) -> String {
    let s: String = s
        .chars()
        .map(|c| match c {
            '\\' | '(' | ')' => format!("\\{c}"),
            _ if c.is_ascii() => c.to_string(),
            _ => format!("\\{:03o}", u32::from(c) & 0xff),
        })
        .collect();
    format!("BT /F1 12 Tf {x} {y} Td ({s}) Tj ET\n")
}

const FONT: &str =
    "<< /Font << /F1 << /Type /Font /Subtype /Type1 /BaseFont /Helvetica /Encoding /WinAnsiEncoding >> >> >>";

/// A minimal, valid PDF with a correct cross-reference table.
pub fn build(pages: &[Spec]) -> Vec<u8> {
    Doc::new(pages.to_vec()).build()
}

/// The reference to page `index` of a [`Doc`] or [`build`] PDF.
pub fn page_ref(index: usize) -> String {
    format!("{} 0 R", 3 + 2 * index)
}

/// A link annotation over `rect` (`x0 y0 x1 y1` in points) with `target`,
/// e.g. `/Dest [3 0 R /Fit]` or `/A << /S /URI /URI (https://example.org) >>`.
pub fn link(rect: [i32; 4], target: &str) -> String {
    let [x0, y0, x1, y1] = rect;
    format!("<< /Type /Annot /Subtype /Link /Rect [{x0} {y0} {x1} {y1}] {target} >>")
}

/// A test PDF with more than pages: catalog and page entries, further
/// objects and an outline.
pub struct Doc {
    pages: Vec<Spec>,
    catalog: String,
    page_entries: Vec<String>,
    objects: Vec<String>,
}

impl Doc {
    pub fn new(pages: Vec<Spec>) -> Doc {
        let page_entries = vec![String::new(); pages.len()];
        Doc {
            pages,
            catalog: String::new(),
            page_entries,
            objects: Vec::new(),
        }
    }

    /// Adds `entry` (e.g. `/Dests << ... >>`) to the catalog.
    pub fn catalog(&mut self, entry: &str) -> &mut Doc {
        self.catalog.push(' ');
        self.catalog.push_str(entry);
        self
    }

    /// Adds `entry` (e.g. `/Annots [...]`) to page `index`.
    pub fn page_entry(&mut self, index: usize, entry: &str) -> &mut Doc {
        self.page_entries[index].push(' ');
        self.page_entries[index].push_str(entry);
        self
    }

    /// The reference the object added `k` objects from now gets: `ahead(0)`
    /// is the next [`Doc::object`].
    pub fn ahead(&self, k: usize) -> String {
        format!("{} 0 R", 3 + 2 * self.pages.len() + self.objects.len() + k)
    }

    /// Adds an object and returns its reference.
    pub fn object(&mut self, body: &str) -> String {
        let r = self.ahead(0);
        self.objects.push(body.to_string());
        r
    }

    /// Adds an outline of one or more `(title, level, target)` items in
    /// reading order, starting at level 0. Titles are PDF strings such as `(Intro)`, targets
    /// entries such as `/Dest [3 0 R /Fit]`. Items get only `/Title`,
    /// `/First` and `/Next`.
    pub fn outline(&mut self, items: &[(&str, u32, &str)]) -> &mut Doc {
        let root = self.ahead(0);
        let refs: Vec<String> = (0..items.len()).map(|k| self.ahead(1 + k)).collect();
        self.catalog(&format!("/Outlines {root}"));
        self.object(&format!("<< /Type /Outlines /First {} >>", refs[0]));
        for (k, &(title, level, target)) in items.iter().enumerate() {
            let later = &items[k + 1..];
            let mut item = format!("<< /Title {title} {target}");
            if later.first().is_some_and(|i| i.1 == level + 1) {
                item.push_str(&format!(" /First {}", refs[k + 1]));
            }
            let next = later.iter().position(|i| i.1 <= level);
            if let Some(j) = next.filter(|&j| later[j].1 == level) {
                item.push_str(&format!(" /Next {}", refs[k + 1 + j]));
            }
            item.push_str(" >>");
            self.object(&item);
        }
        self
    }

    /// The PDF, with a correct cross-reference table.
    pub fn build(&self) -> Vec<u8> {
        let n = self.pages.len();
        let mut objects = vec![
            format!("<< /Type /Catalog /Pages 2 0 R{} >>", self.catalog),
            format!(
                "<< /Type /Pages /Kids [{}] /Count {n} >>",
                (0..n).map(page_ref).collect::<Vec<_>>().join(" ")
            ),
        ];
        for (i, p) in self.pages.iter().enumerate() {
            objects.push(format!(
                "<< /Type /Page /Parent 2 0 R /MediaBox [0 0 {} {}] /Rotate {} /Resources {FONT} /Contents {} 0 R{} >>",
                p.w,
                p.h,
                p.rotate,
                4 + 2 * i,
                self.page_entries[i]
            ));
            objects.push(format!(
                "<< /Length {} >>\nstream\n{}\nendstream",
                p.content.len(),
                p.content
            ));
        }
        objects.extend(self.objects.iter().cloned());
        let mut out = b"%PDF-1.4\n".to_vec();
        let mut offsets = Vec::new();
        for (i, o) in objects.iter().enumerate() {
            offsets.push(out.len());
            out.extend_from_slice(format!("{} 0 obj\n{o}\nendobj\n", i + 1).as_bytes());
        }
        let xref = out.len();
        out.extend_from_slice(format!("xref\n0 {}\n0000000000 65535 f \n", objects.len() + 1).as_bytes());
        for off in offsets {
            out.extend_from_slice(format!("{off:010} 00000 n \n").as_bytes());
        }
        out.extend_from_slice(
            format!(
                "trailer\n<< /Size {} /Root 1 0 R >>\nstartxref\n{xref}\n%%EOF\n",
                objects.len() + 1
            )
            .as_bytes(),
        );
        out
    }
}

/// Password that opens [`LOCKED_AES256`] and [`LOCKED_RC4`] as a reader.
pub const LOCKED_PASSWORD: &str = "sesame";
/// The owner password, which opens [`LOCKED_AES256`] too.
pub const LOCKED_OWNER_PASSWORD: &str = "owner-key";
/// Two pages as in [`RED_SQUARE`] on 100×200 and an empty 300×150,
/// encrypted with AES-256 (made with pypdf).
pub const LOCKED_AES256: &[u8] = include_bytes!("fixtures/locked-aes256.pdf");
/// The same, encrypted with 128-bit RC4.
pub const LOCKED_RC4: &[u8] = include_bytes!("fixtures/locked-rc4.pdf");
