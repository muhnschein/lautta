// SPDX-License-Identifier: LGPL-2.1-or-later
//! Text viewer model (SPEC PRV-4): bounded loading and UTF-8 validation.
//!
//! The loaded `text` is normalised for display: line breaks are `\n`, a BOM
//! is dropped and the single final line break is *not* part of it, so the
//! viewer shows exactly the lines of the file.

use crate::error::Result;
use crate::provider::{read_all, ReadHandle};

/// The viewer reads at most this much (PRV-4).
pub const MAX_TEXT_BYTES: usize = 1024 * 1024;

const BOM: &[u8] = b"\xEF\xBB\xBF";

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TextDoc {
    /// Normalised text (see the module docs); lossy when `valid_utf8` is false.
    pub text: String,
    /// The file is longer than [`MAX_TEXT_BYTES`]; `text` is its beginning.
    pub truncated: bool,
    pub valid_utf8: bool,
}

/// Reads up to [`MAX_TEXT_BYTES`] from `handle` and analyses it.
pub async fn load_text(handle: &dyn ReadHandle) -> Result<TextDoc> {
    let bytes = read_all(handle, MAX_TEXT_BYTES as u64 + 1).await?;
    Ok(analyze(&bytes))
}

/// Analyses raw bytes: `bytes.len() > MAX_TEXT_BYTES` means "truncated".
pub fn analyze(bytes: &[u8]) -> TextDoc {
    let truncated = bytes.len() > MAX_TEXT_BYTES;
    let raw = &bytes[..bytes.len().min(MAX_TEXT_BYTES)];
    let raw = raw.strip_prefix(BOM).unwrap_or(raw);
    let (valid_utf8, decoded) = decode(raw, truncated);
    let normalised = normalise(&decoded);
    let text = match normalised.strip_suffix('\n') {
        Some(rest) if !truncated => rest.to_owned(),
        _ => normalised,
    };
    TextDoc {
        text,
        truncated,
        valid_utf8,
    }
}

/// UTF-8 decoding; a multi-byte sequence cut by truncation is not an error.
fn decode(raw: &[u8], truncated: bool) -> (bool, String) {
    match std::str::from_utf8(raw) {
        Ok(s) => (true, s.to_owned()),
        Err(e) if truncated && e.error_len().is_none() => {
            let ok = std::str::from_utf8(&raw[..e.valid_up_to()]).unwrap_or_default();
            (true, ok.to_owned())
        }
        Err(_) => (false, String::from_utf8_lossy(raw).into_owned()),
    }
}

/// CRLF and lone CR become LF.
fn normalise(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    let mut chars = text.chars().peekable();
    while let Some(c) = chars.next() {
        if c == '\r' {
            if chars.peek() == Some(&'\n') {
                chars.next();
            }
            out.push('\n');
        } else {
            out.push(c);
        }
    }
    out
}

/// How the viewer presents a file by its name.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TextKind {
    /// Prose: proportional font.
    Plain,
    Markdown,
    /// Source code, with a language label for the viewer.
    Code(&'static str),
    Config,
    Log,
    /// CSV, JSON, XML and similar.
    Data,
}

impl TextKind {
    /// Monospace font in the viewer.
    pub fn monospace(self) -> bool {
        !matches!(self, TextKind::Plain | TextKind::Markdown)
    }
}

const CODE: &[(&str, &str)] = &[
    ("rs", "Rust"),
    ("c", "C"),
    ("h", "C"),
    ("cpp", "C++"),
    ("cc", "C++"),
    ("cxx", "C++"),
    ("hpp", "C++"),
    ("py", "Python"),
    ("js", "JavaScript"),
    ("mjs", "JavaScript"),
    ("ts", "TypeScript"),
    ("qml", "QML"),
    ("java", "Java"),
    ("kt", "Kotlin"),
    ("go", "Go"),
    ("sh", "Shell"),
    ("bash", "Shell"),
    ("zsh", "Shell"),
    ("pl", "Perl"),
    ("rb", "Ruby"),
    ("php", "PHP"),
    ("lua", "Lua"),
    ("sql", "SQL"),
    ("css", "CSS"),
    ("html", "HTML"),
    ("htm", "HTML"),
    ("spec", "RPM spec"),
    ("cmake", "CMake"),
    ("pro", "qmake"),
    ("pri", "qmake"),
    ("mk", "Make"),
];

const CONFIG: &[&str] = &[
    "conf",
    "cfg",
    "ini",
    "toml",
    "yaml",
    "yml",
    "desktop",
    "service",
    "rules",
    "properties",
    "env",
    "gitignore",
    "editorconfig",
];

const DATA: &[&str] = &["json", "xml", "csv", "tsv", "svg", "rss", "atom", "xsd", "plist"];

const SPECIAL_NAMES: &[(&str, TextKind)] = &[
    ("makefile", TextKind::Code("Make")),
    ("dockerfile", TextKind::Code("Dockerfile")),
    ("cmakelists.txt", TextKind::Code("CMake")),
    ("readme", TextKind::Plain),
    ("license", TextKind::Plain),
    ("copying", TextKind::Plain),
    ("changelog", TextKind::Plain),
    (".gitignore", TextKind::Config),
    (".bashrc", TextKind::Code("Shell")),
    (".profile", TextKind::Code("Shell")),
];

/// Classifies by file name (extension, then well-known names).
pub fn text_kind(name: &[u8]) -> TextKind {
    let lower = String::from_utf8_lossy(name).to_lowercase();
    if let Some((_, kind)) = SPECIAL_NAMES.iter().find(|(n, _)| *n == lower) {
        return *kind;
    }
    let ext = lower.rsplit_once('.').map(|(_, e)| e).unwrap_or("");
    if let Some((_, lang)) = CODE.iter().find(|(e, _)| *e == ext) {
        return TextKind::Code(lang);
    }
    match ext {
        "md" | "markdown" => TextKind::Markdown,
        "log" => TextKind::Log,
        e if CONFIG.contains(&e) => TextKind::Config,
        e if DATA.contains(&e) => TextKind::Data,
        _ => TextKind::Plain,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::provider::memory::MemoryProvider;
    use crate::provider::{Lane, Provider};
    use crate::vpath::VPath;

    #[test]
    fn line_breaks_are_normalised() {
        let cases: [(&[u8], &str); 4] = [
            (b"a\nb\n", "a\nb"),
            (b"a\r\nb\r\n", "a\nb"),
            (b"a\rb\r", "a\nb"),
            (b"a\r\nb\nc", "a\nb\nc"),
        ];
        for (bytes, text) in cases {
            assert_eq!(analyze(bytes).text, text, "{bytes:?}");
        }
    }

    #[test]
    fn crlf_pair_is_one_break_not_two() {
        let doc = analyze(b"a\r\n\r\nb");
        assert_eq!(doc.text, "a\n\nb");
    }

    #[test]
    fn the_final_newline_and_bom_are_not_in_the_text() {
        assert_eq!(analyze(b"x\ny\n").text, "x\ny");
        assert_eq!(analyze(b"x\ny").text, "x\ny");
        assert_eq!(analyze(b"\xEF\xBB\xBFbom\r\n").text, "bom");
        assert_eq!(analyze(b"\n\n").text, "\n");
    }

    #[test]
    fn invalid_utf8_is_flagged() {
        let doc = analyze(b"caf\xE9\n");
        assert!(!doc.valid_utf8);
        assert!(doc.text.contains('\u{FFFD}'));
        assert!(analyze("café".as_bytes()).valid_utf8);
    }

    #[test]
    fn truncation_cuts_at_a_character_boundary() {
        let mut data = vec![b'a'; MAX_TEXT_BYTES - 1];
        data.extend_from_slice("é".as_bytes());
        data.extend_from_slice(b"tail");
        let doc = analyze(&data);
        assert!(doc.truncated);
        assert!(doc.valid_utf8, "a split multi-byte character is not an error");
        assert_eq!(doc.text.len(), MAX_TEXT_BYTES - 1);
        let exact = vec![b'a'; MAX_TEXT_BYTES];
        assert!(!analyze(&exact).truncated);
    }

    #[test]
    fn truncated_text_keeps_its_last_line_break() {
        let mut data = vec![b'a'; MAX_TEXT_BYTES - 1];
        data.push(b'\n');
        data.push(b'z');
        let doc = analyze(&data);
        assert!(doc.truncated);
        assert!(doc.text.ends_with('\n'));
    }

    #[tokio::test]
    async fn load_text_reads_through_a_handle() {
        let mem = MemoryProvider::default();
        mem.add_file("big.txt", &vec![b'x'; MAX_TEXT_BYTES + 500], 0);
        mem.add_file("small.txt", b"hi\r\n", 0);
        let open = |n: &str| {
            let p = VPath::parse(n.as_bytes()).unwrap_or_default();
            let mem = mem.clone();
            async move { mem.open_read(&p, Lane::Interactive).await.unwrap() }
        };
        let big = load_text(open("big.txt").await.as_ref()).await.unwrap();
        assert!(big.truncated);
        assert_eq!(big.text.len(), MAX_TEXT_BYTES);
        let small = load_text(open("small.txt").await.as_ref()).await.unwrap();
        assert_eq!(small.text, "hi");
    }

    #[test]
    fn kinds_by_name() {
        assert_eq!(text_kind(b"main.RS"), TextKind::Code("Rust"));
        assert_eq!(text_kind(b"Makefile"), TextKind::Code("Make"));
        assert_eq!(text_kind(b"notes.txt"), TextKind::Plain);
        assert_eq!(text_kind(b"README.md"), TextKind::Markdown);
        assert_eq!(text_kind(b"a.log"), TextKind::Log);
        assert_eq!(text_kind(b"a.yaml"), TextKind::Config);
        assert_eq!(text_kind(b"a.json"), TextKind::Data);
        assert_eq!(text_kind(b"noext"), TextKind::Plain);
        assert_eq!(text_kind(b"\xff.rs"), TextKind::Code("Rust"));
        assert!(TextKind::Code("C").monospace() && TextKind::Log.monospace() && TextKind::Data.monospace());
        assert!(!TextKind::Plain.monospace() && !TextKind::Markdown.monospace());
    }
}
