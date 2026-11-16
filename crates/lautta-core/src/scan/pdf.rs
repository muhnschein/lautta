// SPDX-License-Identifier: LGPL-2.1-or-later
//! Writes scanned pages as PDF 1.4: each page one picture filling it. Streams to any `Write`;
//! objects in file order, cross-reference table at the end.

use crate::error::Result;
use flate2::write::ZlibEncoder;
use flate2::Compression;
use std::io::Write;

/// Longer side of every page, in points: A4's 297 mm. Shorter side follows the picture, since a
/// photo says nothing about the paper's real size.
pub const LONG_SIDE_PT: f32 = 841.89;

const CATALOG: usize = 1;
const PAGES: usize = 2;
const INFO: usize = 3;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Pixels {
    /// Baseline JPEG, gray or RGB.
    Jpeg { data: Vec<u8>, gray: bool },
    /// One bit per pixel as from [`super::tone::pack_bits`].
    Bits(Vec<u8>),
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PageImage {
    pub width: u32,
    pub height: u32,
    pub pixels: Pixels,
}

/// Page size in points for a picture of `width` × `height` pixels.
pub fn page_points(width: u32, height: u32) -> (f32, f32) {
    let (w, h) = (width.max(1) as f32, height.max(1) as f32);
    let scale = LONG_SIDE_PT / w.max(h);
    (w * scale, h * scale)
}

/// Now, as PDF writes dates.
pub fn created_now() -> String {
    chrono::Utc::now().format("D:%Y%m%d%H%M%SZ").to_string()
}

/// PDF text string: UTF-16BE with byte order mark, as hex.
fn text_string(text: &str) -> String {
    let mut out = String::from("<FEFF");
    for unit in text.encode_utf16() {
        out.push_str(&format!("{unit:04X}"));
    }
    out.push('>');
    out
}

pub struct PdfWriter<W: Write> {
    out: W,
    written: u64,
    /// Byte offset of each object, index = object number.
    offsets: Vec<u64>,
    pages: Vec<usize>,
}

impl<W: Write> PdfWriter<W> {
    pub fn new(out: W) -> Result<PdfWriter<W>> {
        let mut writer = PdfWriter {
            out,
            written: 0,
            offsets: vec![0; INFO + 1],
            pages: Vec::new(),
        };
        // Second line: four bytes above 127 mark the file as binary.
        writer.put(b"%PDF-1.4\n%\xE2\xE3\xCF\xD3\n")?;
        Ok(writer)
    }

    fn put(&mut self, bytes: &[u8]) -> Result<()> {
        self.out.write_all(bytes)?;
        self.written += bytes.len() as u64;
        Ok(())
    }

    fn begin(&mut self, number: usize) -> Result<()> {
        self.offsets[number] = self.written;
        self.put(format!("{number} 0 obj\n").as_bytes())
    }

    fn object(&mut self, number: usize, body: &str) -> Result<()> {
        self.begin(number)?;
        self.put(format!("{body}\nendobj\n").as_bytes())
    }

    fn stream(&mut self, dict: &str, data: &[u8]) -> Result<usize> {
        let number = self.offsets.len();
        self.offsets.push(0);
        self.begin(number)?;
        self.put(format!("<< {dict} /Length {} >>\nstream\n", data.len()).as_bytes())?;
        self.put(data)?;
        self.put(b"\nendstream\nendobj\n")?;
        Ok(number)
    }

    pub fn add_page(&mut self, image: &PageImage) -> Result<()> {
        let size = format!(
            "/Type /XObject /Subtype /Image /Width {} /Height {}",
            image.width, image.height
        );
        let picture = match &image.pixels {
            Pixels::Jpeg { data, gray } => {
                let space = if *gray { "/DeviceGray" } else { "/DeviceRGB" };
                let dict = format!("{size} /ColorSpace {space} /BitsPerComponent 8 /Filter /DCTDecode");
                self.stream(&dict, data)?
            }
            Pixels::Bits(bits) => {
                let mut packed = ZlibEncoder::new(Vec::new(), Compression::best());
                packed.write_all(bits)?;
                let dict = format!("{size} /ColorSpace /DeviceGray /BitsPerComponent 1 /Filter /FlateDecode");
                self.stream(&dict, &packed.finish()?)?
            }
        };
        let (w, h) = page_points(image.width, image.height);
        let draw = format!("q {w:.2} 0 0 {h:.2} 0 0 cm /Im0 Do Q");
        let contents = self.stream("", draw.as_bytes())?;
        let page = self.offsets.len();
        self.offsets.push(0);
        self.object(
            page,
            &format!(
                "<< /Type /Page /Parent {PAGES} 0 R /MediaBox [0 0 {w:.2} {h:.2}] \
                 /Resources << /XObject << /Im0 {picture} 0 R >> >> /Contents {contents} 0 R >>"
            ),
        )?;
        self.pages.push(page);
        Ok(())
    }

    /// `created`: PDF date, `D:YYYYMMDDHHmmSSZ`.
    pub fn finish(mut self, title: &str, created: &str) -> Result<W> {
        let kids: Vec<String> = self.pages.iter().map(|p| format!("{p} 0 R")).collect();
        self.object(
            PAGES,
            &format!(
                "<< /Type /Pages /Count {} /Kids [{}] >>",
                kids.len(),
                kids.join(" ")
            ),
        )?;
        self.object(CATALOG, &format!("<< /Type /Catalog /Pages {PAGES} 0 R >>"))?;
        self.object(
            INFO,
            &format!(
                "<< /Title {} /Producer (Lautta) /CreationDate ({created}) >>",
                text_string(title)
            ),
        )?;
        let start = self.written;
        let count = self.offsets.len();
        let mut table = format!("xref\n0 {count}\n0000000000 65535 f \n");
        for offset in &self.offsets[1..] {
            table.push_str(&format!("{offset:010} 00000 n \n"));
        }
        table.push_str(&format!(
            "trailer\n<< /Size {count} /Root {CATALOG} 0 R /Info {INFO} 0 R >>\nstartxref\n{start}\n%%EOF\n"
        ));
        self.put(table.as_bytes())?;
        self.out.flush()?;
        Ok(self.out)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::scan::tone::pack_bits;
    use image::codecs::jpeg::JpegEncoder;
    use image::{GrayImage, Luma, Rgb, RgbImage};
    use lautta_pdf::PdfDocument;

    fn jpeg_rgb(img: &RgbImage) -> PageImage {
        let mut data = Vec::new();
        JpegEncoder::new_with_quality(&mut data, 95)
            .encode_image(img)
            .unwrap();
        PageImage {
            width: img.width(),
            height: img.height(),
            pixels: Pixels::Jpeg { data, gray: false },
        }
    }

    fn jpeg_gray(img: &GrayImage) -> PageImage {
        let mut data = Vec::new();
        JpegEncoder::new_with_quality(&mut data, 95)
            .encode_image(img)
            .unwrap();
        PageImage {
            width: img.width(),
            height: img.height(),
            pixels: Pixels::Jpeg { data, gray: true },
        }
    }

    fn bits(img: &GrayImage) -> PageImage {
        PageImage {
            width: img.width(),
            height: img.height(),
            pixels: Pixels::Bits(pack_bits(img)),
        }
    }

    fn write(pages: &[PageImage]) -> Vec<u8> {
        let mut writer = PdfWriter::new(Vec::new()).unwrap();
        for p in pages {
            writer.add_page(p).unwrap();
        }
        writer.finish("Scan", "D:20261112101500Z").unwrap()
    }

    /// Left half one colour, right half another.
    fn halves(w: u32, h: u32, left: [u8; 3], right: [u8; 3]) -> RgbImage {
        RgbImage::from_fn(w, h, |x, _| Rgb(if x < w / 2 { left } else { right }))
    }

    fn near(a: &[u8], b: [u8; 3]) -> bool {
        a.iter().zip(b).all(|(x, y)| x.abs_diff(y) <= 12)
    }

    #[test]
    fn page_keeps_the_picture_shape_with_long_side_of_a4() {
        let (w, h) = page_points(1000, 2000);
        assert!((h - 841.89).abs() < 0.01 && (w - 420.945).abs() < 0.01);
        let (w, h) = page_points(3000, 1000);
        assert!((w - 841.89).abs() < 0.01 && (h - 280.63).abs() < 0.01);
        let (w, h) = page_points(0, 0);
        assert!((w - 841.89).abs() < 0.01 && (h - 841.89).abs() < 0.01);
    }

    #[test]
    fn creation_date_is_utc_in_pdf_form() {
        let now = created_now();
        let year: i32 = now[2..6].parse().unwrap();
        assert!(
            now.starts_with("D:") && now.ends_with('Z') && now.len() == 17,
            "{now}"
        );
        assert!(
            now[2..16].bytes().all(|b| b.is_ascii_digit()) && year >= 2026,
            "{now}"
        );
    }

    #[test]
    fn titles_are_written_as_utf16() {
        assert_eq!(text_string("Aä"), "<FEFF004100E4>");
        assert_eq!(text_string(""), "<FEFF>");
        assert_eq!(text_string("😀"), "<FEFFD83DDE00>");
    }

    #[test]
    fn pages_open_in_the_reader_in_order_with_their_sizes() {
        let pdf = write(&[
            jpeg_rgb(&halves(200, 400, [255, 0, 0], [0, 0, 255])),
            jpeg_gray(&GrayImage::from_pixel(300, 100, Luma([128]))),
            bits(&GrayImage::from_pixel(64, 64, Luma([255]))),
        ]);
        let doc = PdfDocument::open(pdf).unwrap();
        assert_eq!(doc.page_count(), 3);
        let sizes: Vec<(i32, i32)> = doc
            .page_sizes()
            .iter()
            .map(|s| (s.width.round() as i32, s.height.round() as i32))
            .collect();
        assert_eq!(sizes, vec![(421, 842), (842, 281), (842, 842)]);
    }

    #[test]
    fn colour_page_fills_the_page_upright() {
        let pdf = write(&[jpeg_rgb(&halves(200, 400, [255, 0, 0], [0, 0, 255]))]);
        let page = PdfDocument::open(pdf).unwrap().render(0, 100).unwrap();
        assert_eq!(page.width, 100);
        assert!((199..=200).contains(&page.height), "height {}", page.height);
        let at = |x: u32, y: u32| {
            let i = ((y * page.width + x) * 4) as usize;
            page.rgba[i..i + 3].to_vec()
        };
        for y in [5, 100, 194] {
            assert!(near(&at(5, y), [255, 0, 0]), "left at {y}: {:?}", at(5, y));
            assert!(near(&at(95, y), [0, 0, 255]), "right at {y}: {:?}", at(95, y));
        }
    }

    #[test]
    fn top_of_the_picture_is_top_of_the_page() {
        let img = RgbImage::from_fn(100, 100, |_, y| {
            Rgb(if y < 50 { [0, 200, 0] } else { [250, 250, 250] })
        });
        let page = PdfDocument::open(write(&[jpeg_rgb(&img)]))
            .unwrap()
            .render(0, 50)
            .unwrap();
        assert!(near(&page.rgba[(5 * 50 + 25) * 4..][..3], [0, 200, 0]));
        assert!(near(&page.rgba[(45 * 50 + 25) * 4..][..3], [250, 250, 250]));
    }

    #[test]
    fn gray_and_black_and_white_pages_draw_their_pixels() {
        let gray = GrayImage::from_fn(80, 80, |x, _| Luma([if x < 40 { 60 } else { 200 }]));
        let bw = GrayImage::from_fn(80, 80, |x, _| Luma([if x < 40 { 0 } else { 255 }]));
        let doc = PdfDocument::open(write(&[jpeg_gray(&gray), bits(&bw)])).unwrap();
        let page = doc.render(0, 80).unwrap();
        assert!(near(&page.rgba[(40 * 80 + 10) * 4..][..3], [60, 60, 60]));
        assert!(near(&page.rgba[(40 * 80 + 70) * 4..][..3], [200, 200, 200]));
        let page = doc.render(1, 80).unwrap();
        assert!(near(&page.rgba[(40 * 80 + 10) * 4..][..3], [0, 0, 0]));
        assert!(near(&page.rgba[(40 * 80 + 70) * 4..][..3], [255, 255, 255]));
    }

    #[test]
    fn file_structure_is_plain_pdf() {
        let pdf = write(&[bits(&GrayImage::from_pixel(8, 8, Luma([255])))]);
        assert!(pdf.starts_with(b"%PDF-1.4\n%\xE2\xE3\xCF\xD3\n"));
        let text = String::from_utf8_lossy(&pdf).into_owned();
        assert!(text.ends_with("%%EOF\n"));
        assert!(text
            .contains("/Title <FEFF005300630061006E> /Producer (Lautta) /CreationDate (D:20261112101500Z)"));
        assert!(text.contains("/Type /Pages /Count 1 /Kids [6 0 R]"));
        assert!(text.contains("/BitsPerComponent 1 /Filter /FlateDecode"));
        // Every table entry points at its object. Offsets count bytes, so the table is cut from
        // the bytes: its text is plain ASCII.
        let marker = pdf.windows(10).rposition(|w| w == b"startxref\n").unwrap();
        let start: usize = String::from_utf8_lossy(&pdf[marker + 10..])
            .lines()
            .next()
            .and_then(|n| n.parse().ok())
            .unwrap();
        let table = String::from_utf8(pdf[start..].to_vec()).unwrap();
        assert!(table.starts_with("xref\n0 7\n0000000000 65535 f \n"));
        let offsets: Vec<usize> = table
            .lines()
            .skip(3)
            .take(6)
            .map(|l| l[..10].parse().unwrap())
            .collect();
        for (i, offset) in offsets.iter().enumerate() {
            assert!(
                pdf[*offset..].starts_with(format!("{} 0 obj\n", i + 1).as_bytes()),
                "object {}",
                i + 1
            );
        }
        assert!(text.contains("trailer\n<< /Size 7 /Root 1 0 R /Info 3 0 R >>"));
    }

    #[test]
    fn stream_lengths_match_their_data() {
        let pdf = write(&[jpeg_rgb(&halves(16, 16, [9, 9, 9], [200, 200, 200]))]);
        let text = String::from_utf8_lossy(&pdf[..200]).into_owned();
        let length: usize = text
            .split("/Length ")
            .nth(1)
            .and_then(|t| t.split(' ').next())
            .and_then(|n| n.parse().ok())
            .unwrap();
        let data = pdf.windows(7).position(|w| w == b"stream\n").unwrap() + 7;
        assert!(pdf[data..].starts_with(&[0xff, 0xd8]));
        assert!(pdf[data + length..].starts_with(b"\nendstream\nendobj\n"));
    }

    struct Full;

    impl Write for Full {
        fn write(&mut self, _: &[u8]) -> std::io::Result<usize> {
            Err(std::io::Error::from(std::io::ErrorKind::StorageFull))
        }

        fn flush(&mut self) -> std::io::Result<()> {
            Ok(())
        }
    }

    #[test]
    fn write_errors_come_back() {
        assert!(PdfWriter::new(Full).is_err());
    }
}
