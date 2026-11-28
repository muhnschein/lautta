// SPDX-License-Identifier: LGPL-2.1-or-later

use std::io::Read;

pub const MAX_SVG_DEPTH: usize = 256;
pub const MAX_SVGZ_BYTES: u64 = 32 * 1024 * 1024;

const GZIP: &[u8] = &[0x1f, 0x8b];
const XML_HEAD: usize = 64;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SvgRefusal {
    TooDeep,
    TooLarge,
}

impl SvgRefusal {
    pub fn message(self) -> &'static str {
        match self {
            SvgRefusal::TooDeep => "image nested too deeply to show",
            SvgRefusal::TooLarge => "image too large to show",
        }
    }
}

pub fn svg_refusal(bytes: &[u8]) -> Option<SvgRefusal> {
    if bytes.starts_with(GZIP) {
        let mut text = Vec::new();
        let _ = flate2::read::MultiGzDecoder::new(bytes)
            .take(MAX_SVGZ_BYTES + 1)
            .read_to_end(&mut text);
        if text.len() as u64 > MAX_SVGZ_BYTES {
            return Some(SvgRefusal::TooLarge);
        }
        return too_deep(&text).then_some(SvgRefusal::TooDeep);
    }
    (looks_like_xml(bytes) && too_deep(bytes)).then_some(SvgRefusal::TooDeep)
}

fn looks_like_xml(bytes: &[u8]) -> bool {
    let head = &bytes[..bytes.len().min(XML_HEAD)];
    [&b"<?xml"[..], b"<svg", b"<!--", b"<!DOCTYPE"]
        .iter()
        .any(|tag| find(head, tag).is_some())
}

fn find(hay: &[u8], needle: &[u8]) -> Option<usize> {
    hay.windows(needle.len()).position(|w| w == needle)
}

fn skip_past(text: &[u8], from: usize, end: &[u8]) -> usize {
    find(&text[from..], end).map_or(text.len(), |at| from + at + end.len())
}

fn tag_end(text: &[u8], from: usize) -> usize {
    let mut quote = None;
    for (i, &b) in text[from..].iter().enumerate() {
        match quote {
            Some(q) if b == q => quote = None,
            Some(_) => {}
            None if b == b'"' || b == b'\'' => quote = Some(b),
            None if b == b'>' => return from + i,
            None => {}
        }
    }
    text.len()
}

fn too_deep(text: &[u8]) -> bool {
    let mut depth = 0usize;
    let mut at = 0;
    while let Some(lt) = text[at..].iter().position(|&b| b == b'<') {
        let start = at + lt + 1;
        let rest = &text[start..];
        if rest.starts_with(b"!--") {
            at = skip_past(text, start + 3, b"-->");
        } else if rest.starts_with(b"![CDATA[") {
            at = skip_past(text, start + 8, b"]]>");
        } else if rest.starts_with(b"!") || rest.starts_with(b"?") {
            at = tag_end(text, start).saturating_add(1).min(text.len());
        } else if rest.starts_with(b"/") {
            depth = depth.saturating_sub(1);
            at = tag_end(text, start).saturating_add(1).min(text.len());
        } else {
            let end = tag_end(text, start);
            if end >= text.len() || text[end - 1] != b'/' {
                depth += 1;
                if depth > MAX_SVG_DEPTH {
                    return true;
                }
            }
            at = end.saturating_add(1).min(text.len());
        }
    }
    false
}

#[cfg(test)]
mod tests {
    use super::*;
    use flate2::write::GzEncoder;
    use std::io::Write;

    fn nested(depth: usize) -> Vec<u8> {
        let mut svg = String::from(
            r#"<?xml version="1.0"?><svg xmlns="http://www.w3.org/2000/svg" width="8" height="8">"#,
        );
        svg.push_str(&"<g>".repeat(depth - 1));
        svg.push_str(r#"<rect width="8" height="8" fill="red"/>"#);
        svg.push_str(&"</g>".repeat(depth - 1));
        svg.push_str("</svg>");
        svg.into_bytes()
    }

    fn gzip(bytes: &[u8]) -> Vec<u8> {
        let mut enc = GzEncoder::new(Vec::new(), flate2::Compression::fast());
        enc.write_all(bytes).unwrap();
        enc.finish().unwrap()
    }

    #[test]
    fn svgs_nested_up_to_the_limit_pass_and_one_level_more_is_refused() {
        assert_eq!(svg_refusal(&nested(MAX_SVG_DEPTH)), None);
        assert_eq!(svg_refusal(&nested(MAX_SVG_DEPTH + 1)), Some(SvgRefusal::TooDeep));
        assert_eq!(svg_refusal(&nested(100_000)), Some(SvgRefusal::TooDeep));
    }

    #[test]
    fn empty_elements_comments_cdata_declarations_and_quotes_add_no_depth() {
        let mut svg = String::from(r#"<!DOCTYPE svg [ <!ENTITY a "<g>"> ]><svg>"#);
        for _ in 0..MAX_SVG_DEPTH * 2 {
            svg.push_str(
                r#"<path d="M0 0"/><!-- a > <g> --><![CDATA[ a > <g> ]]><?pi <g>?><text x="a>b" y='<g>'>t</text>"#,
            );
        }
        svg.push_str("</svg>");
        assert_eq!(svg_refusal(svg.as_bytes()), None);
    }

    #[test]
    fn a_deep_svgz_is_refused_and_a_flat_one_passes() {
        assert_eq!(
            svg_refusal(&gzip(&nested(MAX_SVG_DEPTH + 1))),
            Some(SvgRefusal::TooDeep)
        );
        assert_eq!(svg_refusal(&gzip(&nested(8))), None);
    }

    #[test]
    fn a_cut_svgz_is_judged_by_what_it_holds() {
        let deep = gzip(&nested(MAX_SVG_DEPTH * 4));
        assert_eq!(svg_refusal(&deep[..deep.len() / 2]), Some(SvgRefusal::TooDeep));
    }

    #[test]
    fn an_svgz_inflating_past_its_cap_is_refused() {
        let spaces = vec![b' '; usize::try_from(MAX_SVGZ_BYTES).unwrap() + 1];
        assert_eq!(svg_refusal(&gzip(&spaces)), Some(SvgRefusal::TooLarge));
        assert_eq!(SvgRefusal::TooLarge.message(), "image too large to show");
    }

    #[test]
    fn other_images_and_text_are_not_judged_as_svgs() {
        let deep = "<g>".repeat(MAX_SVG_DEPTH + 1);
        for head in [&b"\x89PNG\r\n\x1a\n"[..], b"GIF89a", b"plain text "] {
            let mut bytes = head.to_vec();
            bytes.extend(deep.as_bytes());
            assert_eq!(svg_refusal(&bytes), None, "{head:?}");
        }
        assert_eq!(svg_refusal(b""), None);
    }

    #[test]
    fn an_svg_whose_root_starts_late_is_still_checked() {
        let mut svg = format!("{}<svg>", " ".repeat(XML_HEAD - 5)).into_bytes();
        svg.extend("<g>".repeat(MAX_SVG_DEPTH + 1).as_bytes());
        assert_eq!(svg_refusal(&svg), Some(SvgRefusal::TooDeep));
    }
}
