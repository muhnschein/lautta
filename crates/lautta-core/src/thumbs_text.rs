// SPDX-License-Identifier: LGPL-2.1-or-later
//! Text thumbnails: first lines of a text, code or Markdown file drawn by the grid in QML, plus the
//! type icon. [`crate::thumbs::thumbnail_source`] picks the files; this turns the first
//! [`TEXT_THUMB_BYTES`] into the text shown.

use crate::mime::looks_like_text;
use crate::preview::text::text_kind;

/// Tile shows about five lines.
pub const TEXT_THUMB_BYTES: u64 = 512;
/// Spaces per tab.
const TAB: &str = "    ";

/// `None` when not text (NUL, control characters, not UTF-8) or blank: the glyph shows. Character
/// cut at end of head is dropped; line breaks become `\n`, tabs spaces, other control characters
/// go.
pub fn thumb_text(head: &[u8]) -> Option<String> {
    let head = head.strip_prefix(b"\xEF\xBB\xBF").unwrap_or(head);
    if !looks_like_text(head) {
        return None;
    }
    let valid = match std::str::from_utf8(head) {
        Ok(s) => s,
        Err(e) => std::str::from_utf8(&head[..e.valid_up_to()]).ok()?,
    };
    let mut out = String::with_capacity(valid.len());
    let mut chars = valid.chars().peekable();
    while let Some(c) = chars.next() {
        match c {
            '\r' => {
                if chars.peek() != Some(&'\n') {
                    out.push('\n');
                }
            }
            '\t' => out.push_str(TAB),
            '\n' => out.push('\n'),
            c if c.is_control() => {}
            c => out.push(c),
        }
    }
    let trimmed = out.trim_end();
    (!trimmed.trim_start().is_empty()).then(|| trimmed.to_owned())
}

/// Monospace for code, configuration, logs and data, as the text viewer shows them.
pub fn thumb_monospace(name: &[u8]) -> bool {
    text_kind(name).monospace()
}

/// The type icon of a text thumbnail: an app icon name, `qml/icons/types/`.
pub fn type_icon(name: &[u8]) -> &'static str {
    let lower = String::from_utf8_lossy(name).to_lowercase();
    let ext = lower.rsplit_once('.').map_or("", |(_, e)| e);
    match ext {
        "md" | "markdown" => "types/markdown",
        "sh" | "bash" | "zsh" => "types/shell",
        "toml" => "types/toml",
        "json" => "types/json",
        "xml" => "types/xml",
        "html" | "htm" => "types/html",
        "css" => "types/css",
        "js" | "mjs" => "types/javascript",
        "yaml" | "yml" => "types/yaml",
        "csv" | "tsv" => "types/csv",
        "sql" => "types/sql",
        "diff" | "patch" => "types/patch",
        "java" => "types/java",
        "ini" | "conf" | "cfg" | "properties" => "types/config",
        "rs" => "types/rust",
        _ => "types/text",
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn text_keeps_its_lines() {
        assert_eq!(
            thumb_text(b"# Notes\n\nFerry at 06:00\n").as_deref(),
            Some("# Notes\n\nFerry at 06:00")
        );
    }

    #[test]
    fn line_breaks_tabs_and_controls_are_tidied() {
        assert_eq!(thumb_text(b"a\r\nb\rc").as_deref(), Some("a\nb\nc"));
        assert_eq!(
            thumb_text(b"fn main() {\n\tx\n}").as_deref(),
            Some("fn main() {\n    x\n}")
        );
        assert_eq!(thumb_text(b"a\x1b[1mb\x0cc").as_deref(), Some("a[1mbc"));
    }

    #[test]
    fn a_byte_order_mark_is_dropped() {
        assert_eq!(thumb_text(b"\xEF\xBB\xBFhei").as_deref(), Some("hei"));
    }

    #[test]
    fn a_character_cut_by_the_read_is_dropped() {
        // "Utö" with its last byte missing.
        assert_eq!(thumb_text(b"Ut\xC3").as_deref(), Some("Ut"));
        assert_eq!(thumb_text("Utö".as_bytes()).as_deref(), Some("Utö"));
    }

    #[test]
    fn leading_blank_lines_stay_trailing_ones_go() {
        assert_eq!(thumb_text(b"\n\n  x  \n\n").as_deref(), Some("\n\n  x"));
    }

    #[test]
    fn what_is_no_text_gets_none() {
        assert_eq!(thumb_text(b""), None);
        assert_eq!(thumb_text(b" \n\t\r\n"), None);
        assert_eq!(thumb_text(b"PK\x03\x04\0\0binary"), None);
        assert_eq!(thumb_text(b"caf\xE9 latin-1 text"), None);
        assert_eq!(thumb_text(&[0x01; 40]), None);
    }

    #[test]
    fn code_and_data_are_monospace_prose_is_not() {
        for name in [
            "main.rs",
            "backup.sh",
            "config.toml",
            "data.csv",
            "boot.log",
            "Makefile",
        ] {
            assert!(thumb_monospace(name.as_bytes()), "{name}");
        }
        for name in ["notes.txt", "README.md", "README"] {
            assert!(!thumb_monospace(name.as_bytes()), "{name}");
        }
    }

    #[test]
    fn type_icons_by_extension() {
        let cases = [
            ("README.md", "types/markdown"),
            ("notes.MARKDOWN", "types/markdown"),
            ("backup.sh", "types/shell"),
            ("run.bash", "types/shell"),
            (".zsh", "types/shell"),
            ("Cargo.toml", "types/toml"),
            ("a.json", "types/json"),
            ("a.xml", "types/xml"),
            ("index.HTM", "types/html"),
            ("index.html", "types/html"),
            ("a.css", "types/css"),
            ("a.js", "types/javascript"),
            ("a.mjs", "types/javascript"),
            ("a.yml", "types/yaml"),
            ("a.yaml", "types/yaml"),
            ("a.csv", "types/csv"),
            ("a.tsv", "types/csv"),
            ("a.sql", "types/sql"),
            ("a.diff", "types/patch"),
            ("a.patch", "types/patch"),
            ("A.java", "types/java"),
            ("a.ini", "types/config"),
            ("a.conf", "types/config"),
            ("a.cfg", "types/config"),
            ("a.properties", "types/config"),
            ("main.rs", "types/rust"),
            ("notes.txt", "types/text"),
            ("main.py", "types/text"),
            ("README", "types/text"),
        ];
        for (name, icon) in cases {
            assert_eq!(type_icon(name.as_bytes()), icon, "{name}");
        }
    }
}
