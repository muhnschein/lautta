// SPDX-License-Identifier: LGPL-2.1-or-later
//! EXIF for the image viewer's info panel and for thumbnails (SPEC PRV-2,
//! PRV-4), read with `kamadak-exif` from the bytes of an image file.
//! Everything is best effort: missing or malformed fields are simply absent.

use exif::{Exif, In, Reader, Tag, Value};
use std::io::Cursor;

/// The fields the panel shows. Text fields are trimmed; numbers are in
/// display units.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct ExifInfo {
    pub make: Option<String>,
    pub model: Option<String>,
    pub lens: Option<String>,
    /// `YYYY-MM-DD HH:MM:SS` as recorded (no time zone).
    pub date_taken: Option<String>,
    /// `1/250 s` or `2 s`.
    pub exposure: Option<String>,
    /// `f/2.8`
    pub aperture: Option<String>,
    pub iso: Option<u32>,
    pub focal_length_mm: Option<f64>,
    pub width: Option<u32>,
    pub height: Option<u32>,
    /// EXIF orientation 1-8 (1 = upright).
    pub orientation: Option<u8>,
    /// Decimal degrees, south and west negative.
    pub gps: Option<(f64, f64)>,
    pub has_thumbnail: bool,
}

fn parse(bytes: &[u8]) -> Option<Exif> {
    Reader::new().read_from_container(&mut Cursor::new(bytes)).ok()
}

fn text(exif: &Exif, tag: Tag) -> Option<String> {
    let field = exif.get_field(tag, In::PRIMARY)?;
    match &field.value {
        Value::Ascii(parts) => {
            let joined: Vec<u8> = parts.first()?.iter().copied().take_while(|b| *b != 0).collect();
            let s = String::from_utf8_lossy(&joined).trim().to_owned();
            (!s.is_empty()).then_some(s)
        }
        _ => None,
    }
}

fn uint(exif: &Exif, tag: Tag, ifd: In) -> Option<u32> {
    exif.get_field(tag, ifd)?.value.get_uint(0)
}

fn rational(exif: &Exif, tag: Tag) -> Option<f64> {
    match &exif.get_field(tag, In::PRIMARY)?.value {
        Value::Rational(v) => {
            let r = v.first()?;
            let f = r.to_f64();
            (r.denom != 0 && f.is_finite()).then_some(f)
        }
        _ => None,
    }
}

/// `1/250 s` for fast shutters, `2 s` / `1.5 s` for long ones.
pub fn format_exposure(seconds: f64) -> Option<String> {
    if !seconds.is_finite() || seconds <= 0.0 {
        return None;
    }
    if seconds >= 1.0 {
        let rounded = (seconds * 10.0).round() / 10.0;
        return Some(format!("{rounded} s"));
    }
    Some(format!("1/{} s", (1.0 / seconds).round() as u64))
}

/// `2024:03:05 12:34:56` to `2024-03-05 12:34:56`.
fn format_date(raw: &str) -> String {
    let mut chars: Vec<char> = raw.chars().collect();
    if chars.len() >= 10 && chars[4] == ':' && chars[7] == ':' {
        chars[4] = '-';
        chars[7] = '-';
    }
    chars.into_iter().collect()
}

/// Degrees/minutes/seconds plus hemisphere reference to signed degrees.
fn gps_coordinate(exif: &Exif, value_tag: Tag, ref_tag: Tag, negative_ref: &str, limit: f64) -> Option<f64> {
    let Value::Rational(dms) = &exif.get_field(value_tag, In::PRIMARY)?.value else {
        return None;
    };
    if dms.len() < 3 || dms.iter().take(3).any(|r| r.denom == 0) {
        return None;
    }
    let deg = dms[0].to_f64() + dms[1].to_f64() / 60.0 + dms[2].to_f64() / 3600.0;
    let sign = if text(exif, ref_tag).as_deref() == Some(negative_ref) {
        -1.0
    } else {
        1.0
    };
    let value = deg * sign;
    (value.is_finite() && value.abs() <= limit).then_some(value)
}

fn gps(exif: &Exif) -> Option<(f64, f64)> {
    let lat = gps_coordinate(exif, Tag::GPSLatitude, Tag::GPSLatitudeRef, "S", 90.0)?;
    let lon = gps_coordinate(exif, Tag::GPSLongitude, Tag::GPSLongitudeRef, "W", 180.0)?;
    Some((lat, lon))
}

fn orientation_of(exif: &Exif) -> Option<u8> {
    let v = uint(exif, Tag::Orientation, In::PRIMARY)?;
    u8::try_from(v).ok().filter(|o| (1..=8).contains(o))
}

/// Reads the panel fields; `None` when the file has no EXIF block at all.
pub fn read_exif(bytes: &[u8]) -> Option<ExifInfo> {
    let exif = parse(bytes)?;
    let width =
        uint(&exif, Tag::PixelXDimension, In::PRIMARY).or_else(|| uint(&exif, Tag::ImageWidth, In::PRIMARY));
    let height =
        uint(&exif, Tag::PixelYDimension, In::PRIMARY).or_else(|| uint(&exif, Tag::ImageLength, In::PRIMARY));
    Some(ExifInfo {
        make: text(&exif, Tag::Make),
        model: text(&exif, Tag::Model),
        lens: text(&exif, Tag::LensModel),
        date_taken: text(&exif, Tag::DateTimeOriginal).map(|d| format_date(&d)),
        exposure: rational(&exif, Tag::ExposureTime).and_then(format_exposure),
        aperture: rational(&exif, Tag::FNumber).map(|f| format!("f/{}", (f * 10.0).round() / 10.0)),
        iso: uint(&exif, Tag::PhotographicSensitivity, In::PRIMARY),
        focal_length_mm: rational(&exif, Tag::FocalLength),
        width,
        height,
        orientation: orientation_of(&exif),
        gps: gps(&exif),
        has_thumbnail: thumbnail_range(&exif).is_some(),
    })
}

/// Display rotation for the viewer: the EXIF orientation, 1 when absent.
pub fn orientation(bytes: &[u8]) -> u8 {
    parse(bytes).and_then(|e| orientation_of(&e)).unwrap_or(1)
}

fn thumbnail_range(exif: &Exif) -> Option<(usize, usize)> {
    let offset = uint(exif, Tag::JPEGInterchangeFormat, In::THUMBNAIL)?;
    let length = uint(exif, Tag::JPEGInterchangeFormatLength, In::THUMBNAIL)?;
    let start = usize::try_from(offset).ok()?;
    let end = start.checked_add(usize::try_from(length).ok()?)?;
    (length > 0 && end <= exif.buf().len()).then_some((start, end))
}

/// The embedded JPEG thumbnail, if there is one.
pub fn exif_thumbnail(bytes: &[u8]) -> Option<Vec<u8>> {
    let exif = parse(bytes)?;
    let (start, end) = thumbnail_range(&exif)?;
    Some(exif.buf().get(start..end)?.to_vec())
}

/// Hand-assembled EXIF for tests of this module and of `thumbs`.
#[cfg(test)]
pub(crate) mod testutil {
    pub struct Spec {
        pub orientation: Option<u16>,
        pub thumbnail: Option<Vec<u8>>,
        pub gps: bool,
    }

    struct Entry {
        tag: u16,
        typ: u16,
        count: u32,
        data: Vec<u8>,
    }

    fn ascii(tag: u16, s: &str) -> Entry {
        let mut data = s.as_bytes().to_vec();
        data.push(0);
        Entry {
            tag,
            typ: 2,
            count: data.len() as u32,
            data,
        }
    }

    fn short(tag: u16, v: u16) -> Entry {
        Entry {
            tag,
            typ: 3,
            count: 1,
            data: v.to_le_bytes().to_vec(),
        }
    }

    fn long(tag: u16, v: u32) -> Entry {
        Entry {
            tag,
            typ: 4,
            count: 1,
            data: v.to_le_bytes().to_vec(),
        }
    }

    fn rationals(tag: u16, values: &[(u32, u32)]) -> Entry {
        let data = values
            .iter()
            .flat_map(|(n, d)| n.to_le_bytes().into_iter().chain(d.to_le_bytes()))
            .collect();
        Entry {
            tag,
            typ: 5,
            count: values.len() as u32,
            data,
        }
    }

    /// Serialises an IFD that starts at TIFF offset `at`.
    fn ifd(mut entries: Vec<Entry>, at: u32, next: u32) -> Vec<u8> {
        entries.sort_by_key(|e| e.tag);
        let table_len = 2 + 12 * entries.len() as u32 + 4;
        let mut table = (entries.len() as u16).to_le_bytes().to_vec();
        let mut extra: Vec<u8> = Vec::new();
        for e in &entries {
            table.extend_from_slice(&e.tag.to_le_bytes());
            table.extend_from_slice(&e.typ.to_le_bytes());
            table.extend_from_slice(&e.count.to_le_bytes());
            if e.data.len() <= 4 {
                let mut inline = e.data.clone();
                inline.resize(4, 0);
                table.extend_from_slice(&inline);
            } else {
                table.extend_from_slice(&(at + table_len + extra.len() as u32).to_le_bytes());
                extra.extend_from_slice(&e.data);
                if extra.len() % 2 == 1 {
                    extra.push(0);
                }
            }
        }
        table.extend_from_slice(&next.to_le_bytes());
        table.extend_from_slice(&extra);
        table
    }

    fn ifd0(spec: &Spec, exif_at: u32, gps_at: u32) -> Vec<Entry> {
        let mut e = vec![
            ascii(0x010F, "TestCam"),
            ascii(0x0110, "Model X1"),
            long(0x8769, exif_at),
        ];
        if let Some(o) = spec.orientation {
            e.push(short(0x0112, o));
        }
        if spec.gps {
            e.push(long(0x8825, gps_at));
        }
        e
    }

    fn exif_ifd() -> Vec<Entry> {
        vec![
            ascii(0x9003, "2024:03:05 12:34:56"),
            rationals(0x829A, &[(1, 250)]),
            rationals(0x829D, &[(28, 10)]),
            short(0x8827, 400),
            rationals(0x920A, &[(50, 1)]),
            long(0xA002, 4000),
            long(0xA003, 3000),
            ascii(0xA434, "Lens 50mm"),
        ]
    }

    fn gps_ifd() -> Vec<Entry> {
        vec![
            ascii(0x0001, "S"),
            rationals(0x0002, &[(60, 1), (10, 1), (30, 1)]),
            ascii(0x0003, "W"),
            rationals(0x0004, &[(24, 1), (56, 1), (24, 1)]),
        ]
    }

    /// A little-endian TIFF structure with IFD0, Exif and GPS IFDs and an
    /// optional IFD1 holding a JPEG thumbnail.
    pub fn tiff(spec: &Spec) -> Vec<u8> {
        let len0 = ifd(ifd0(spec, 0, 0), 8, 0).len() as u32;
        let exif_at = 8 + len0;
        let exif_len = ifd(exif_ifd(), exif_at, 0).len() as u32;
        let gps_at = exif_at + exif_len;
        let gps_len = if spec.gps {
            ifd(gps_ifd(), gps_at, 0).len() as u32
        } else {
            0
        };
        let ifd1_at = gps_at + gps_len;
        let thumb = spec.thumbnail.as_deref();
        let thumb_entries = |thumb_at: u32| {
            vec![
                short(0x0103, 6),
                long(0x0201, thumb_at),
                long(0x0202, thumb.map_or(0, |t| t.len() as u32)),
            ]
        };
        let ifd1_len = if thumb.is_some() {
            ifd(thumb_entries(0), ifd1_at, 0).len() as u32
        } else {
            0
        };
        let next = if thumb.is_some() { ifd1_at } else { 0 };
        let mut out = vec![b'I', b'I', 42, 0, 8, 0, 0, 0];
        out.extend(ifd(ifd0(spec, exif_at, gps_at), 8, next));
        out.extend(ifd(exif_ifd(), exif_at, 0));
        if spec.gps {
            out.extend(ifd(gps_ifd(), gps_at, 0));
        }
        if let Some(t) = thumb {
            out.extend(ifd(thumb_entries(ifd1_at + ifd1_len), ifd1_at, 0));
            out.extend_from_slice(t);
        }
        out
    }

    /// The APP1 segment (marker, length, `Exif\0\0`, TIFF).
    pub fn app1(spec: &Spec) -> Vec<u8> {
        let tiff = tiff(spec);
        let mut seg = vec![0xFF, 0xE1];
        seg.extend_from_slice(&((tiff.len() + 8) as u16).to_be_bytes());
        seg.extend_from_slice(b"Exif\0\0");
        seg.extend_from_slice(&tiff);
        seg
    }

    /// Inserts `app1` right after the SOI of a real JPEG.
    pub fn with_exif(jpeg: &[u8], spec: &Spec) -> Vec<u8> {
        let mut out = jpeg[..2].to_vec();
        out.extend(app1(spec));
        out.extend_from_slice(&jpeg[2..]);
        out
    }

    /// A JPEG that is only SOI, the EXIF segment and EOI.
    pub fn bare_jpeg(spec: &Spec) -> Vec<u8> {
        let mut out = vec![0xFF, 0xD8];
        out.extend(app1(spec));
        out.extend_from_slice(&[0xFF, 0xD9]);
        out
    }
}

#[cfg(test)]
mod tests {
    use super::testutil::*;
    use super::*;

    fn spec() -> Spec {
        Spec {
            orientation: Some(6),
            thumbnail: None,
            gps: true,
        }
    }

    #[test]
    fn reads_every_panel_field() {
        let info = read_exif(&bare_jpeg(&spec())).unwrap();
        assert_eq!(info.make.as_deref(), Some("TestCam"));
        assert_eq!(info.model.as_deref(), Some("Model X1"));
        assert_eq!(info.lens.as_deref(), Some("Lens 50mm"));
        assert_eq!(info.date_taken.as_deref(), Some("2024-03-05 12:34:56"));
        assert_eq!(info.exposure.as_deref(), Some("1/250 s"));
        assert_eq!(info.aperture.as_deref(), Some("f/2.8"));
        assert_eq!(info.iso, Some(400));
        assert_eq!(info.focal_length_mm, Some(50.0));
        assert_eq!((info.width, info.height), (Some(4000), Some(3000)));
        assert_eq!(info.orientation, Some(6));
        assert!(!info.has_thumbnail);
    }

    #[test]
    fn gps_is_decimal_with_hemisphere_signs() {
        let (lat, lon) = read_exif(&bare_jpeg(&spec())).unwrap().gps.unwrap();
        assert!((lat - -60.175).abs() < 1e-9, "{lat}");
        assert!((lon - -24.94).abs() < 1e-9, "{lon}");
        let no_gps = Spec { gps: false, ..spec() };
        assert_eq!(read_exif(&bare_jpeg(&no_gps)).unwrap().gps, None);
    }

    #[test]
    fn orientation_defaults_to_upright() {
        assert_eq!(orientation(&bare_jpeg(&spec())), 6);
        let none = Spec {
            orientation: None,
            ..spec()
        };
        assert_eq!(orientation(&bare_jpeg(&none)), 1);
        assert_eq!(orientation(b"not an image"), 1);
        let bogus = Spec {
            orientation: Some(42),
            ..spec()
        };
        assert_eq!(
            orientation(&bare_jpeg(&bogus)),
            1,
            "out-of-range values are ignored"
        );
        assert_eq!(read_exif(&bare_jpeg(&bogus)).unwrap().orientation, None);
    }

    #[test]
    fn embedded_thumbnail_is_extracted() {
        let thumb = vec![0xFF, 0xD8, 1, 2, 3, 4, 5, 0xFF, 0xD9];
        let with = Spec {
            thumbnail: Some(thumb.clone()),
            ..spec()
        };
        let jpeg = bare_jpeg(&with);
        assert_eq!(exif_thumbnail(&jpeg), Some(thumb));
        assert!(read_exif(&jpeg).unwrap().has_thumbnail);
        assert_eq!(exif_thumbnail(&bare_jpeg(&spec())), None);
    }

    #[test]
    fn files_without_exif_give_none() {
        assert_eq!(read_exif(b""), None);
        assert_eq!(read_exif(&[0xFF, 0xD8, 0xFF, 0xD9]), None);
        assert_eq!(read_exif(b"\x89PNG\r\n\x1a\n"), None);
        assert_eq!(exif_thumbnail(b"junk"), None);
    }

    #[test]
    fn truncated_exif_does_not_panic() {
        let full = bare_jpeg(&spec());
        for cut in [10, 20, 40, 80, full.len() - 3] {
            let _ = read_exif(&full[..cut]);
            let _ = exif_thumbnail(&full[..cut]);
            let _ = orientation(&full[..cut]);
        }
    }

    #[test]
    fn exposure_formatting() {
        assert_eq!(format_exposure(1.0 / 250.0).as_deref(), Some("1/250 s"));
        assert_eq!(format_exposure(2.0).as_deref(), Some("2 s"));
        assert_eq!(format_exposure(1.5).as_deref(), Some("1.5 s"));
        assert_eq!(format_exposure(0.0), None);
        assert_eq!(format_exposure(f64::NAN), None);
    }

    #[test]
    fn date_formatting_only_touches_well_formed_dates() {
        assert_eq!(format_date("2024:03:05 12:34:56"), "2024-03-05 12:34:56");
        assert_eq!(format_date("yesterday"), "yesterday");
    }
}
