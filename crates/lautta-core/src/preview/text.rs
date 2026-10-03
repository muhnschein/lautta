// SPDX-License-Identifier: LGPL-2.1-or-later
//! Text viewer and editor model (SPEC PRV-4, EDT-4): bounded loading, UTF-8
//! validation, line-ending and final-newline detection, and a save that puts
//! the original conventions back.
//!
//! The loaded `text` is normalised for an editor widget: line breaks are
//! `\n` and the single final line break is *not* part of it (it is recorded
//! in [`TextMeta`]), so the widget shows exactly the lines of the file.

use crate::error::Result;
use crate::provider::{read_all, ReadHandle};

/// The viewer reads at most this much (PRV-4); the editor only opens files
/// that fit completely (EDT-4).
pub const MAX_TEXT_BYTES: usize = 1024 * 1024;

const BOM: &[u8] = b"\xEF\xBB\xBF";

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum LineEnding {
    /// No line break in the file.
    #[default]
    None,
    Lf,
    CrLf,
    Cr,
    /// More than one convention; saving uses the most frequent one.
    Mixed,
}

/// What must be put back when the text is saved.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct TextMeta {
    pub line_ending: LineEnding,
    /// The convention used for new line breaks (dominant one when mixed).
    pub newline: Newline,
    pub final_newline: bool,
    pub bom: bool,
}

impl Default for TextMeta {
    fn default() -> Self {
        TextMeta {
            line_ending: LineEnding::None,
            newline: Newline::Lf,
            final_newline: false,
            bom: false,
        }
    }
}

/// A concrete line break sequence.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Newline {
    Lf,
    CrLf,
    Cr,
}

impl Newline {
    pub fn as_str(self) -> &'static str {
        match self {
            Newline::Lf => "\n",
            Newline::CrLf => "\r\n",
            Newline::Cr => "\r",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TextDoc {
    /// Normalised text (see the module docs); lossy when `valid_utf8` is false.
    pub text: String,
    /// The file is longer than [`MAX_TEXT_BYTES`]; `text` is its beginning.
    pub truncated: bool,
    pub valid_utf8: bool,
    pub meta: TextMeta,
}

impl TextDoc {
    /// The editor may open it: complete and valid UTF-8 (EDT-4).
    pub fn editable(&self) -> bool {
        self.valid_utf8 && !self.truncated
    }
}

/// Reads up to [`MAX_TEXT_BYTES`] from `handle` and analyses it.
pub async fn load_text(handle: &dyn ReadHandle) -> Result<TextDoc> {
    let bytes = read_all(handle, MAX_TEXT_BYTES as u64 + 1).await?;
    Ok(analyze(&bytes))
}

/// Analyses raw bytes: `bytes.len() > MAX_TEXT_BYTES` means "truncated".
pub fn analyze(bytes: &[u8]) -> TextDoc {
    let truncated = bytes.len() > MAX_TEXT_BYTES;
    let mut raw = &bytes[..bytes.len().min(MAX_TEXT_BYTES)];
    let bom = raw.starts_with(BOM);
    if bom {
        raw = &raw[BOM.len()..];
    }
    let (valid_utf8, decoded) = decode(raw, truncated);
    let counts = count_breaks(&decoded);
    let normalised = normalise(&decoded);
    let (body, final_newline) = match normalised.strip_suffix('\n') {
        Some(rest) if !truncated => (rest.to_owned(), true),
        _ => (normalised, false),
    };
    TextDoc {
        text: body,
        truncated,
        valid_utf8,
        meta: TextMeta {
            line_ending: counts.kind(),
            newline: counts.dominant(),
            final_newline,
            bom,
        },
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

#[derive(Debug, Default, Clone, Copy)]
struct BreakCounts {
    lf: usize,
    crlf: usize,
    cr: usize,
}

impl BreakCounts {
    fn kind(self) -> LineEnding {
        match (self.lf > 0, self.crlf > 0, self.cr > 0) {
            (false, false, false) => LineEnding::None,
            (true, false, false) => LineEnding::Lf,
            (false, true, false) => LineEnding::CrLf,
            (false, false, true) => LineEnding::Cr,
            _ => LineEnding::Mixed,
        }
    }

    fn dominant(self) -> Newline {
        if self.crlf > self.lf && self.crlf >= self.cr {
            Newline::CrLf
        } else if self.cr > self.lf {
            Newline::Cr
        } else {
            Newline::Lf
        }
    }
}

fn count_breaks(text: &str) -> BreakCounts {
    let mut counts = BreakCounts::default();
    let mut chars = text.bytes().peekable();
    while let Some(b) = chars.next() {
        match b {
            b'\n' => counts.lf += 1,
            b'\r' if chars.peek() == Some(&b'\n') => {
                chars.next();
                counts.crlf += 1;
            }
            b'\r' => counts.cr += 1,
            _ => {}
        }
    }
    counts
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

/// Bytes to write for edited `content` (EDT-4): line breaks in the file's
/// own convention, the final newline and BOM as they were. Line breaks in
/// `content` may be any of LF, CRLF or CR.
pub fn save_text(content: &str, meta: &TextMeta) -> Vec<u8> {
    let newline = meta.newline.as_str();
    let mut body = normalise(content);
    if meta.final_newline {
        body.push('\n');
    }
    let mut out = Vec::with_capacity(body.len() + 8);
    if meta.bom {
        out.extend_from_slice(BOM);
    }
    if newline == "\n" {
        out.extend_from_slice(body.as_bytes());
    } else {
        out.extend_from_slice(body.replace('\n', newline).as_bytes());
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
    fn detects_each_line_ending() {
        let cases: [(&[u8], LineEnding, Newline); 5] = [
            (b"a\nb\n", LineEnding::Lf, Newline::Lf),
            (b"a\r\nb\r\n", LineEnding::CrLf, Newline::CrLf),
            (b"a\rb\r", LineEnding::Cr, Newline::Cr),
            (b"single line", LineEnding::None, Newline::Lf),
            (b"a\r\nb\r\nc\nd", LineEnding::Mixed, Newline::CrLf),
        ];
        for (bytes, kind, newline) in cases {
            let doc = analyze(bytes);
            assert_eq!(doc.meta.line_ending, kind, "{bytes:?}");
            assert_eq!(doc.meta.newline, newline, "{bytes:?}");
        }
    }

    #[test]
    fn crlf_pair_is_one_break_not_two() {
        let doc = analyze(b"a\r\n\r\nb");
        assert_eq!(doc.meta.line_ending, LineEnding::CrLf);
        assert_eq!(doc.text, "a\n\nb");
    }

    #[test]
    fn round_trip_preserves_bytes() {
        let samples: [&[u8]; 9] = [
            b"",
            b"\n",
            b"one",
            b"one\n",
            b"a\r\nb\r\n",
            b"a\r\nb",
            b"a\rb\r",
            b"\xEF\xBB\xBFbom\r\ntext\r\n",
            b"\n\n\nx\n\n",
        ];
        for bytes in samples {
            let doc = analyze(bytes);
            assert_eq!(save_text(&doc.text, &doc.meta), bytes, "{bytes:?}");
        }
    }

    #[test]
    fn final_newline_is_not_in_the_text_but_survives_saving() {
        let doc = analyze(b"x\ny\n");
        assert_eq!(doc.text, "x\ny");
        assert!(doc.meta.final_newline);
        assert_eq!(save_text("x\ny\nz", &doc.meta), b"x\ny\nz\n");
        let none = analyze(b"x\ny");
        assert!(!none.meta.final_newline);
        assert_eq!(save_text("x\ny\nz", &none.meta), b"x\ny\nz");
    }

    #[test]
    fn edits_use_the_files_convention() {
        let doc = analyze(b"a\r\nb\r\n");
        assert_eq!(
            save_text("a\nnew line\r\nb", &doc.meta),
            b"a\r\nnew line\r\nb\r\n"
        );
        let cr = analyze(b"a\rb");
        assert_eq!(save_text("a\nb\nc", &cr.meta), b"a\rb\rc");
    }

    #[test]
    fn mixed_files_are_saved_with_the_dominant_ending() {
        let doc = analyze(b"a\r\nb\r\nc\nd");
        assert_eq!(doc.meta.line_ending, LineEnding::Mixed);
        assert_eq!(save_text(&doc.text, &doc.meta), b"a\r\nb\r\nc\r\nd");
    }

    #[test]
    fn invalid_utf8_is_flagged_and_not_editable() {
        let doc = analyze(b"caf\xE9\n");
        assert!(!doc.valid_utf8);
        assert!(!doc.editable());
        assert!(doc.text.contains('\u{FFFD}'));
        assert!(analyze("café".as_bytes()).editable());
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
        assert!(!doc.editable());
        let exact = vec![b'a'; MAX_TEXT_BYTES];
        let doc = analyze(&exact);
        assert!(!doc.truncated);
        assert!(doc.editable());
    }

    #[test]
    fn truncated_text_never_claims_a_final_newline() {
        let mut data = vec![b'a'; MAX_TEXT_BYTES - 1];
        data.push(b'\n');
        data.push(b'z');
        let doc = analyze(&data);
        assert!(doc.truncated);
        assert!(!doc.meta.final_newline);
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
        assert_eq!(
            (small.text.as_str(), small.meta.line_ending),
            ("hi", LineEnding::CrLf)
        );
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
