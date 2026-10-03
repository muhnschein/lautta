// SPDX-License-Identifier: LGPL-2.1-or-later
//! MIME type and file category from extension and magic (BRW-1), and the
//! viewer that handles a file (PRV-4).

use crate::entry::{Entry, Kind};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub enum FileCategory {
    Folder,
    Image,
    Video,
    Audio,
    Text,
    Code,
    Markdown,
    Pdf,
    Archive,
    Document,
    Spreadsheet,
    Presentation,
    /// SQLite databases.
    Database,
    /// rpm and apk packages.
    Package,
    Other,
}

impl FileCategory {
    /// Icon name for the UI (maps to a Silica/theme icon in QML).
    pub fn icon_name(self) -> &'static str {
        match self {
            FileCategory::Folder => "folder",
            FileCategory::Image => "image",
            FileCategory::Video => "video",
            FileCategory::Audio => "audio",
            FileCategory::Text => "text",
            FileCategory::Code => "code",
            FileCategory::Markdown => "markdown",
            FileCategory::Pdf => "pdf",
            FileCategory::Archive => "archive",
            FileCategory::Document => "document",
            FileCategory::Spreadsheet => "spreadsheet",
            FileCategory::Presentation => "presentation",
            FileCategory::Database => "database",
            FileCategory::Package => "package",
            FileCategory::Other => "file",
        }
    }
}

/// Which built-in viewer opens a file (PRV-4). PDFs and office documents go
/// to *Open with* (`External`); poppler is not allowed.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Viewer {
    Image,
    Text,
    Markdown,
    Audio,
    Video,
    Archive,
    Sqlite,
    Hex,
    External,
}

/// Lowercased ASCII extension of a file name. A leading dot alone (`.bashrc`)
/// is not an extension; non-ASCII or very long suffixes are ignored.
pub fn extension_of(name: &[u8]) -> Option<String> {
    let dot = name.iter().rposition(|b| *b == b'.')?;
    let ext = &name[dot + 1..];
    if dot == 0 || ext.is_empty() || ext.len() > 16 || !ext.is_ascii() {
        return None;
    }
    Some(String::from_utf8_lossy(ext).to_ascii_lowercase())
}

/// Mime type and category for a lowercase extension.
pub fn from_extension(ext: &str) -> Option<(&'static str, FileCategory)> {
    use FileCategory::*;
    Some(match ext {
        "png" => ("image/png", Image),
        "jpg" | "jpeg" | "jpe" => ("image/jpeg", Image),
        "gif" => ("image/gif", Image),
        "webp" => ("image/webp", Image),
        "bmp" => ("image/bmp", Image),
        "svg" | "svgz" => ("image/svg+xml", Image),
        "ico" => ("image/vnd.microsoft.icon", Image),
        "tif" | "tiff" => ("image/tiff", Image),
        "heic" | "heif" => ("image/heic", Image),
        "avif" => ("image/avif", Image),
        "jxl" => ("image/jxl", Image),
        "mp4" | "m4v" => ("video/mp4", Video),
        "mkv" => ("video/x-matroska", Video),
        "webm" => ("video/webm", Video),
        "avi" => ("video/x-msvideo", Video),
        "mov" => ("video/quicktime", Video),
        "3gp" => ("video/3gpp", Video),
        "wmv" => ("video/x-ms-wmv", Video),
        "flv" => ("video/x-flv", Video),
        "mpg" | "mpeg" => ("video/mpeg", Video),
        "ogv" => ("video/ogg", Video),
        "mp3" => ("audio/mpeg", Audio),
        "flac" => ("audio/flac", Audio),
        "ogg" | "oga" => ("audio/ogg", Audio),
        "opus" => ("audio/opus", Audio),
        "wav" => ("audio/x-wav", Audio),
        "m4a" => ("audio/mp4", Audio),
        "aac" => ("audio/aac", Audio),
        "wma" => ("audio/x-ms-wma", Audio),
        "mid" | "midi" => ("audio/midi", Audio),
        "amr" => ("audio/amr", Audio),
        "txt" | "log" | "ini" | "conf" | "cfg" | "nfo" => ("text/plain", Text),
        "csv" => ("text/csv", Text),
        "tsv" => ("text/tab-separated-values", Text),
        "md" | "markdown" => ("text/markdown", Markdown),
        "pdf" => ("application/pdf", Pdf),
        "json" => ("application/json", Code),
        "xml" => ("application/xml", Code),
        "html" | "htm" => ("text/html", Code),
        "css" => ("text/css", Code),
        "js" | "mjs" => ("text/javascript", Code),
        "yaml" | "yml" => ("application/yaml", Code),
        "toml" => ("application/toml", Code),
        "sh" | "bash" => ("application/x-shellscript", Code),
        "rs" => ("text/x-rust", Code),
        "c" | "h" => ("text/x-csrc", Code),
        "cpp" | "cc" | "cxx" | "hpp" => ("text/x-c++src", Code),
        "py" => ("text/x-python", Code),
        "ts" => ("text/x-typescript", Code),
        "qml" => ("text/x-qml", Code),
        "java" => ("text/x-java", Code),
        "kt" => ("text/x-kotlin", Code),
        "go" => ("text/x-go", Code),
        "sql" => ("application/sql", Code),
        "diff" | "patch" => ("text/x-diff", Code),
        "pro" | "pri" | "spec" | "desktop" | "cmake" | "lua" | "rb" | "pl" | "php" => ("text/x-source", Code),
        "zip" => ("application/zip", Archive),
        "tar" => ("application/x-tar", Archive),
        "gz" | "tgz" => ("application/gzip", Archive),
        "xz" | "txz" => ("application/x-xz", Archive),
        "bz2" | "tbz2" => ("application/x-bzip2", Archive),
        "zst" => ("application/zstd", Archive),
        "7z" => ("application/x-7z-compressed", Archive),
        "rar" => ("application/vnd.rar", Archive),
        "jar" => ("application/java-archive", Archive),
        "doc" => ("application/msword", Document),
        "docx" => (
            "application/vnd.openxmlformats-officedocument.wordprocessingml.document",
            Document,
        ),
        "odt" => ("application/vnd.oasis.opendocument.text", Document),
        "rtf" => ("application/rtf", Document),
        "epub" => ("application/epub+zip", Document),
        "xls" => ("application/vnd.ms-excel", Spreadsheet),
        "xlsx" => (
            "application/vnd.openxmlformats-officedocument.spreadsheetml.sheet",
            Spreadsheet,
        ),
        "ods" => ("application/vnd.oasis.opendocument.spreadsheet", Spreadsheet),
        "ppt" => ("application/vnd.ms-powerpoint", Presentation),
        "pptx" => (
            "application/vnd.openxmlformats-officedocument.presentationml.presentation",
            Presentation,
        ),
        "odp" => ("application/vnd.oasis.opendocument.presentation", Presentation),
        "sqlite" | "sqlite3" | "db" => ("application/vnd.sqlite3", Database),
        "rpm" => ("application/x-rpm", Package),
        "apk" => ("application/vnd.android.package-archive", Package),
        _ => return None,
    })
}

/// Longest-prefix-first table for provider-supplied MIME types.
const MIME_PREFIXES: &[(&str, FileCategory)] = &[
    ("text/markdown", FileCategory::Markdown),
    ("text/x-", FileCategory::Code),
    ("text/html", FileCategory::Code),
    ("text/css", FileCategory::Code),
    ("text/javascript", FileCategory::Code),
    ("text/", FileCategory::Text),
    ("image/", FileCategory::Image),
    ("video/", FileCategory::Video),
    ("audio/", FileCategory::Audio),
    ("application/pdf", FileCategory::Pdf),
    ("application/json", FileCategory::Code),
    ("application/xml", FileCategory::Code),
    ("application/javascript", FileCategory::Code),
    ("application/x-sh", FileCategory::Code),
    ("application/zip", FileCategory::Archive),
    ("application/gzip", FileCategory::Archive),
    ("application/x-tar", FileCategory::Archive),
    ("application/x-xz", FileCategory::Archive),
    ("application/x-bzip2", FileCategory::Archive),
    ("application/zstd", FileCategory::Archive),
    ("application/x-7z", FileCategory::Archive),
    ("application/vnd.rar", FileCategory::Archive),
    ("application/vnd.sqlite3", FileCategory::Database),
    ("application/x-sqlite3", FileCategory::Database),
    ("application/x-rpm", FileCategory::Package),
    ("application/vnd.android.package-archive", FileCategory::Package),
    ("application/msword", FileCategory::Document),
    (
        "application/vnd.openxmlformats-officedocument.wordprocessingml",
        FileCategory::Document,
    ),
    ("application/vnd.oasis.opendocument.text", FileCategory::Document),
    ("application/vnd.ms-excel", FileCategory::Spreadsheet),
    (
        "application/vnd.openxmlformats-officedocument.spreadsheetml",
        FileCategory::Spreadsheet,
    ),
    (
        "application/vnd.oasis.opendocument.spreadsheet",
        FileCategory::Spreadsheet,
    ),
    ("application/vnd.ms-powerpoint", FileCategory::Presentation),
    (
        "application/vnd.openxmlformats-officedocument.presentationml",
        FileCategory::Presentation,
    ),
    (
        "application/vnd.oasis.opendocument.presentation",
        FileCategory::Presentation,
    ),
    ("inode/directory", FileCategory::Folder),
];

/// Category for a MIME type given by a provider (parameters are ignored).
pub fn category_from_mime(mime: &str) -> FileCategory {
    let base = mime.split(';').next().unwrap_or("").trim().to_ascii_lowercase();
    MIME_PREFIXES
        .iter()
        .find(|(prefix, _)| base.starts_with(prefix))
        .map_or(FileCategory::Other, |(_, cat)| *cat)
}

/// Category by file name only (no entry kind, no provider type).
pub fn category_of_name(name: &[u8]) -> FileCategory {
    extension_of(name)
        .and_then(|e| from_extension(&e))
        .map_or(FileCategory::Other, |(_, c)| c)
}

/// Category of an entry: folders (following symlinks), then the extension,
/// then the provider's content type.
pub fn category_of(entry: &Entry) -> FileCategory {
    if entry.is_dir() {
        return FileCategory::Folder;
    }
    if entry.kind == Kind::Special {
        return FileCategory::Other;
    }
    let by_name = category_of_name(&entry.name);
    if by_name != FileCategory::Other {
        return by_name;
    }
    entry
        .content_type
        .as_deref()
        .map_or(FileCategory::Other, category_from_mime)
}

/// MIME type of an entry: extension first, then the provider's content type.
pub fn mime_of(entry: &Entry) -> Option<String> {
    if entry.is_dir() {
        return Some("inode/directory".to_owned());
    }
    extension_of(&entry.name)
        .and_then(|e| from_extension(&e))
        .map(|(m, _)| m.to_owned())
        .or_else(|| entry.content_type.clone())
}

const TEXT_SNIFF_MAX_CONTROL_PERCENT: usize = 2;

/// Magic sniffing from the first bytes of a file (BRW-1, local files).
pub fn sniff(head: &[u8]) -> Option<&'static str> {
    const MAGIC: &[(&[u8], &str)] = &[
        (b"\x89PNG\r\n\x1a\n", "image/png"),
        (b"\xff\xd8\xff", "image/jpeg"),
        (b"GIF87a", "image/gif"),
        (b"GIF89a", "image/gif"),
        (b"%PDF-", "application/pdf"),
        (b"PK\x03\x04", "application/zip"),
        (b"PK\x05\x06", "application/zip"),
        (b"\x1f\x8b", "application/gzip"),
        (b"\xfd7zXZ\x00", "application/x-xz"),
        (b"BZh", "application/x-bzip2"),
        (b"\x28\xb5\x2f\xfd", "application/zstd"),
        (b"7z\xbc\xaf\x27\x1c", "application/x-7z-compressed"),
        (b"SQLite format 3\x00", "application/vnd.sqlite3"),
        (b"ID3", "audio/mpeg"),
        (b"\xff\xfb", "audio/mpeg"),
        (b"\xff\xf3", "audio/mpeg"),
        (b"\xff\xf2", "audio/mpeg"),
        (b"OggS", "audio/ogg"),
        (b"fLaC", "audio/flac"),
        (b"\x7fELF", "application/x-executable"),
    ];
    if let Some((_, mime)) = MAGIC.iter().find(|(m, _)| head.starts_with(m)) {
        return Some(mime);
    }
    if head.len() >= 12 && &head[..4] == b"RIFF" && &head[8..12] == b"WEBP" {
        return Some("image/webp");
    }
    if head.len() >= 8 && &head[4..8] == b"ftyp" {
        return Some("video/mp4");
    }
    looks_like_text(head).then_some("text/plain")
}

/// UTF-8 (a multi-byte character cut at the end of the sample is fine), no
/// NUL, and almost no control characters.
fn looks_like_text(head: &[u8]) -> bool {
    if head.is_empty() {
        return false;
    }
    let valid = match std::str::from_utf8(head) {
        Ok(s) => s,
        Err(e) if e.error_len().is_none() => {
            std::str::from_utf8(&head[..e.valid_up_to()]).unwrap_or_default()
        }
        Err(_) => return false,
    };
    if valid.is_empty() || valid.contains('\0') {
        return false;
    }
    let control = valid
        .chars()
        .filter(|c| c.is_control() && !matches!(c, '\n' | '\r' | '\t' | '\x0c' | '\x1b'))
        .count();
    control * 100 <= valid.chars().count() * TEXT_SNIFF_MAX_CONTROL_PERCENT
}

/// The built-in viewer for a file (PRV-4). `mime` is the best known type
/// (extension, provider or sniffed).
pub fn viewer_for(category: FileCategory, mime: &str) -> Viewer {
    match category {
        FileCategory::Image => Viewer::Image,
        FileCategory::Text | FileCategory::Code => Viewer::Text,
        FileCategory::Markdown => Viewer::Markdown,
        FileCategory::Audio => Viewer::Audio,
        FileCategory::Video => Viewer::Video,
        FileCategory::Database => Viewer::Sqlite,
        FileCategory::Archive => archive_viewer(mime),
        FileCategory::Package if mime == "application/vnd.android.package-archive" => Viewer::Archive,
        FileCategory::Package => Viewer::Hex,
        FileCategory::Other => other_viewer(mime),
        FileCategory::Folder
        | FileCategory::Pdf
        | FileCategory::Document
        | FileCategory::Spreadsheet
        | FileCategory::Presentation => Viewer::External,
    }
}

/// Archive formats the core can open as locations (rar is not one of them).
fn archive_viewer(mime: &str) -> Viewer {
    if mime == "application/vnd.rar" {
        Viewer::External
    } else {
        Viewer::Archive
    }
}

fn other_viewer(mime: &str) -> Viewer {
    match category_from_mime(mime) {
        FileCategory::Text | FileCategory::Code => Viewer::Text,
        _ => Viewer::Hex,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn file(name: &str) -> Entry {
        Entry::new(name.as_bytes(), Kind::File)
    }

    #[test]
    fn extension_rules() {
        assert_eq!(extension_of(b"a.PNG").as_deref(), Some("png"));
        assert_eq!(extension_of(b"a.tar.gz").as_deref(), Some("gz"));
        assert_eq!(extension_of(b".bashrc"), None);
        assert_eq!(extension_of(b"noext"), None);
        assert_eq!(extension_of(b"trailing."), None);
        assert_eq!(extension_of("a.\u{e9}".as_bytes()), None);
        assert_eq!(extension_of(b"a.waytoolongextensionname"), None);
        assert_eq!(extension_of(b"..png").as_deref(), Some("png"));
    }

    #[test]
    fn categories_by_extension() {
        let cases = [
            ("a.jpg", FileCategory::Image),
            ("a.mkv", FileCategory::Video),
            ("a.flac", FileCategory::Audio),
            ("a.txt", FileCategory::Text),
            ("a.rs", FileCategory::Code),
            ("a.MD", FileCategory::Markdown),
            ("a.pdf", FileCategory::Pdf),
            ("a.7z", FileCategory::Archive),
            ("a.docx", FileCategory::Document),
            ("a.ods", FileCategory::Spreadsheet),
            ("a.pptx", FileCategory::Presentation),
            ("a.sqlite", FileCategory::Database),
            ("a.rpm", FileCategory::Package),
            ("a.apk", FileCategory::Package),
            ("a.unknownext", FileCategory::Other),
        ];
        for (name, cat) in cases {
            assert_eq!(category_of(&file(name)), cat, "{name}");
        }
    }

    #[test]
    fn folders_and_symlinks() {
        assert_eq!(
            category_of(&Entry::new(b"x.png", Kind::Dir)),
            FileCategory::Folder
        );
        let mut l = Entry::new(b"l", Kind::Symlink);
        assert_eq!(category_of(&l), FileCategory::Other);
        l.target_kind = Kind::Dir;
        assert_eq!(category_of(&l), FileCategory::Folder);
        assert_eq!(mime_of(&l).as_deref(), Some("inode/directory"));
        assert_eq!(
            category_of(&Entry::new(b"fifo.png", Kind::Special)),
            FileCategory::Other
        );
    }

    #[test]
    fn content_type_is_the_fallback() {
        let mut e = file("noext");
        assert_eq!(mime_of(&e), None);
        e.content_type = Some("image/heic; charset=x".into());
        assert_eq!(category_of(&e), FileCategory::Image);
        assert_eq!(mime_of(&e).as_deref(), Some("image/heic; charset=x"));
        // The extension wins over the provider type.
        let mut e = file("a.txt");
        e.content_type = Some("image/png".into());
        assert_eq!(category_of(&e), FileCategory::Text);
        assert_eq!(mime_of(&e).as_deref(), Some("text/plain"));
    }

    #[test]
    fn mime_table_categories() {
        let cases = [
            ("text/markdown", FileCategory::Markdown),
            ("text/x-python", FileCategory::Code),
            ("text/plain", FileCategory::Text),
            ("video/mp4", FileCategory::Video),
            ("audio/ogg", FileCategory::Audio),
            ("application/pdf", FileCategory::Pdf),
            ("application/zip", FileCategory::Archive),
            ("application/vnd.sqlite3", FileCategory::Database),
            ("application/x-rpm", FileCategory::Package),
            (
                "application/vnd.oasis.opendocument.spreadsheet",
                FileCategory::Spreadsheet,
            ),
            ("application/octet-stream", FileCategory::Other),
            ("APPLICATION/PDF; x=y", FileCategory::Pdf),
            ("", FileCategory::Other),
        ];
        for (mime, cat) in cases {
            assert_eq!(category_from_mime(mime), cat, "{mime}");
        }
    }

    #[test]
    fn extension_table_agrees_with_mime_table() {
        for ext in [
            "png", "mp4", "mp3", "txt", "md", "pdf", "json", "zip", "docx", "xlsx", "pptx", "odt", "sqlite",
            "rpm", "apk", "rs", "html",
        ] {
            let Some((mime, cat)) = from_extension(ext) else {
                panic!("missing {ext}");
            };
            assert_eq!(category_from_mime(mime), cat, "{ext} / {mime}");
        }
    }

    #[test]
    fn icon_names_are_distinct_per_category() {
        let all = [
            FileCategory::Folder,
            FileCategory::Image,
            FileCategory::Video,
            FileCategory::Audio,
            FileCategory::Text,
            FileCategory::Code,
            FileCategory::Markdown,
            FileCategory::Pdf,
            FileCategory::Archive,
            FileCategory::Document,
            FileCategory::Spreadsheet,
            FileCategory::Presentation,
            FileCategory::Database,
            FileCategory::Package,
            FileCategory::Other,
        ];
        let names: std::collections::HashSet<_> = all.iter().map(|c| c.icon_name()).collect();
        assert_eq!(names.len(), all.len());
        assert_eq!(FileCategory::Other.icon_name(), "file");
        assert_eq!(FileCategory::Folder.icon_name(), "folder");
    }

    #[test]
    fn sniff_magic() {
        let cases: [(&[u8], &str); 16] = [
            (b"\x89PNG\r\n\x1a\n....", "image/png"),
            (b"\xff\xd8\xff\xe0", "image/jpeg"),
            (b"GIF89a..", "image/gif"),
            (b"RIFF\x00\x00\x00\x00WEBPVP8 ", "image/webp"),
            (b"%PDF-1.7", "application/pdf"),
            (b"PK\x03\x04rest", "application/zip"),
            (b"\x1f\x8b\x08", "application/gzip"),
            (b"\xfd7zXZ\x00x", "application/x-xz"),
            (b"BZh91AY", "application/x-bzip2"),
            (b"\x28\xb5\x2f\xfd\x00", "application/zstd"),
            (b"7z\xbc\xaf\x27\x1c\x00", "application/x-7z-compressed"),
            (b"SQLite format 3\x00\x10", "application/vnd.sqlite3"),
            (b"ID3\x04", "audio/mpeg"),
            (b"OggS\x00", "audio/ogg"),
            (b"\x00\x00\x00\x18ftypmp42", "video/mp4"),
            (b"\x7fELF\x02", "application/x-executable"),
        ];
        for (bytes, mime) in cases {
            assert_eq!(sniff(bytes), Some(mime), "{bytes:?}");
        }
        assert_eq!(sniff(b"fLaC\x00"), Some("audio/flac"));
        assert_eq!(sniff(b"\xff\xfb\x90"), Some("audio/mpeg"));
    }

    #[test]
    fn sniff_text_heuristic() {
        assert_eq!(sniff(b"hello world\nline two\n"), Some("text/plain"));
        assert_eq!(sniff("caf\u{e9} \u{2603}".as_bytes()), Some("text/plain"));
        // A multi-byte character cut by the sample end is still text.
        let snow = "ab\u{2603}".as_bytes();
        assert_eq!(sniff(&snow[..snow.len() - 1]), Some("text/plain"));
        assert_eq!(sniff(b""), None);
        assert_eq!(sniff(b"abc\x00def"), None);
        assert_eq!(sniff(b"abc\xffdef"), None);
        assert_eq!(sniff(&[0x01, 0x02, 0x03, 0x04]), None);
        // Only a cut character: nothing valid to judge.
        assert_eq!(sniff(&"\u{2603}".as_bytes()[..2]), None);
    }

    #[test]
    fn viewers() {
        use FileCategory::*;
        assert_eq!(viewer_for(Image, "image/png"), Viewer::Image);
        assert_eq!(viewer_for(Text, "text/plain"), Viewer::Text);
        assert_eq!(viewer_for(Code, "text/x-rust"), Viewer::Text);
        assert_eq!(viewer_for(Markdown, "text/markdown"), Viewer::Markdown);
        assert_eq!(viewer_for(Audio, "audio/mpeg"), Viewer::Audio);
        assert_eq!(viewer_for(Video, "video/mp4"), Viewer::Video);
        assert_eq!(viewer_for(Database, "application/vnd.sqlite3"), Viewer::Sqlite);
        assert_eq!(viewer_for(Archive, "application/zip"), Viewer::Archive);
        assert_eq!(viewer_for(Archive, "application/vnd.rar"), Viewer::External);
        assert_eq!(
            viewer_for(Package, "application/vnd.android.package-archive"),
            Viewer::Archive
        );
        assert_eq!(viewer_for(Package, "application/x-rpm"), Viewer::Hex);
        assert_eq!(viewer_for(Pdf, "application/pdf"), Viewer::External);
        assert_eq!(viewer_for(Document, ""), Viewer::External);
        assert_eq!(viewer_for(Spreadsheet, ""), Viewer::External);
        assert_eq!(viewer_for(Presentation, ""), Viewer::External);
        assert_eq!(viewer_for(Folder, ""), Viewer::External);
        assert_eq!(viewer_for(Other, "application/octet-stream"), Viewer::Hex);
        assert_eq!(viewer_for(Other, "text/plain"), Viewer::Text);
        assert_eq!(viewer_for(Other, "text/x-foo"), Viewer::Text);
    }
}
