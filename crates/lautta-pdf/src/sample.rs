// SPDX-License-Identifier: LGPL-2.1-or-later
//! Small, valid PDFs built in code, for tests here and in the crates that
//! use this one (feature `sample`).

/// One page of a test PDF: media box size, `/Rotate` and content stream.
pub struct Spec {
    pub w: u32,
    pub h: u32,
    pub rotate: u32,
    pub content: &'static str,
}

/// Fills a red 50 pt square in the bottom left corner.
pub const RED_SQUARE: &str = "1 0 0 rg 0 0 50 50 re f";

/// An upright page.
pub fn page(w: u32, h: u32, content: &'static str) -> Spec {
    Spec {
        w,
        h,
        rotate: 0,
        content,
    }
}

/// A minimal, valid PDF with a correct cross-reference table.
pub fn build(pages: &[Spec]) -> Vec<u8> {
    let n = pages.len();
    let mut objects = vec![
        "<< /Type /Catalog /Pages 2 0 R >>".to_string(),
        format!(
            "<< /Type /Pages /Kids [{}] /Count {n} >>",
            (0..n)
                .map(|i| format!("{} 0 R", 3 + 2 * i))
                .collect::<Vec<_>>()
                .join(" ")
        ),
    ];
    for (i, p) in pages.iter().enumerate() {
        objects.push(format!(
            "<< /Type /Page /Parent 2 0 R /MediaBox [0 0 {} {}] /Rotate {} /Contents {} 0 R >>",
            p.w,
            p.h,
            p.rotate,
            4 + 2 * i
        ));
        objects.push(format!(
            "<< /Length {} >>\nstream\n{}\nendstream",
            p.content.len(),
            p.content
        ));
    }
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
