// SPDX-License-Identifier: LGPL-2.1-or-later

use crate::entry::{Entry, Kind};
use std::io::Read;
use std::path::Path;

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
    Database,
    Package,
    Mail,
    Calendar,
    Contact,
    Notebook,
    Key,
    Executable,
    Font,
    DiskImage,
    Cad,
    Other,
}

impl FileCategory {
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
            FileCategory::Mail => "mail",
            FileCategory::Calendar => "calendar",
            FileCategory::Contact => "contact",
            FileCategory::Notebook => "notebook",
            FileCategory::Key => "key",
            FileCategory::Executable => "executable",
            FileCategory::Font => "font",
            FileCategory::DiskImage => "disk",
            FileCategory::Cad => "cad",
            FileCategory::Other => "file",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Viewer {
    Image,
    Text,
    Markdown,
    Prose,
    Audio,
    Video,
    Archive,
    Pdf,
    External,
}

pub fn extension_of(name: &[u8]) -> Option<String> {
    let dot = name.iter().rposition(|b| *b == b'.')?;
    let ext = &name[dot + 1..];
    if dot == 0 || ext.is_empty() || ext.len() > 16 || !ext.is_ascii() {
        return None;
    }
    Some(String::from_utf8_lossy(ext).to_ascii_lowercase())
}

const MARKDOWN_EXTENSIONS: [&str; 6] = ["md", "markdown", "mkd", "mdown", "mdwn", "markdn"];

pub fn is_markdown_extension(ext: &str) -> bool {
    MARKDOWN_EXTENSIONS.contains(&ext)
}

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
        "pbm" => ("image/x-portable-bitmap", Image),
        "pgm" => ("image/x-portable-graymap", Image),
        "ppm" => ("image/x-portable-pixmap", Image),
        "pnm" => ("image/x-portable-anymap", Image),
        "cur" => ("image/x-win-bitmap", Image),
        "tga" => ("image/x-tga", Image),
        "psd" => ("image/vnd.adobe.photoshop", Image),
        "exr" => ("image/x-exr", Image),
        "hdr" => ("image/vnd.radiance", Image),
        "jp2" | "j2k" | "jpf" | "jpx" => ("image/jp2", Image),
        "jpm" => ("image/jpm", Image),
        "dds" => ("image/vnd-ms.dds", Image),
        "sgi" => ("image/x-sgi", Image),
        "pcx" => ("image/vnd.zbrush.pcx", Image),
        "qoi" => ("image/qoi", Image),
        "bpg" => ("image/bpg", Image),
        "djvu" | "djv" => ("image/vnd.djvu", Image),
        "emf" => ("image/emf", Image),
        "wmf" => ("image/wmf", Image),
        "icns" => ("image/x-icns", Image),
        "dng" => ("image/x-adobe-dng", Image),
        "nef" | "nrw" => ("image/x-nikon-nef", Image),
        "cr2" => ("image/x-canon-cr2", Image),
        "cr3" => ("image/x-canon-cr3", Image),
        "crw" => ("image/x-canon-crw", Image),
        "arw" | "srf" | "sr2" => ("image/x-sony-arw", Image),
        "orf" => ("image/x-olympus-orf", Image),
        "rw2" => ("image/x-panasonic-rw2", Image),
        "pef" => ("image/x-pentax-pef", Image),
        "erf" => ("image/x-epson-erf", Image),
        "kdc" => ("image/x-kodak-kdc", Image),
        "raf" => ("image/x-fuji-raf", Image),
        "srw" => ("image/x-samsung-srw", Image),
        "mp4" | "m4v" => ("video/mp4", Video),
        "mkv" => ("video/x-matroska", Video),
        "webm" => ("video/webm", Video),
        "avi" => ("video/x-msvideo", Video),
        "mov" => ("video/quicktime", Video),
        "3gp" => ("video/3gpp", Video),
        "wmv" => ("video/x-ms-wmv", Video),
        "flv" => ("video/x-flv", Video),
        "mpg" | "mpeg" => ("video/mpeg", Video),
        "mts" | "m2ts" => (MPEG_TS, Video),
        "ogv" => ("video/ogg", Video),
        "ivf" => ("video/x-ivf", Video),
        "vob" | "m2v" => ("video/mpeg", Video),
        "qt" => ("video/quicktime", Video),
        "divx" => ("video/x-msvideo", Video),
        "mxf" => ("application/mxf", Video),
        "rm" | "rmvb" => ("application/vnd.rn-realmedia", Video),
        "asf" => ("video/x-ms-asf", Video),
        "wtv" => ("video/x-wtv", Video),
        "nuv" => ("video/x-nuv", Video),
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
        "mka" => ("audio/x-matroska", Audio),
        "aif" | "aiff" | "aifc" => ("audio/x-aiff", Audio),
        "ac3" => ("audio/ac3", Audio),
        "caf" => ("audio/x-caf", Audio),
        "au" | "snd" => ("audio/basic", Audio),
        "spx" => ("audio/x-speex", Audio),
        "w64" => ("audio/x-w64", Audio),
        "voc" => ("audio/x-voc", Audio),
        "wv" => ("audio/x-wavpack", Audio),
        "ape" => ("audio/x-ape", Audio),
        "mpc" => ("audio/x-musepack", Audio),
        "tta" => ("audio/x-tta", Audio),
        "eac3" => ("audio/eac3", Audio),
        "dts" => ("audio/vnd.dts", Audio),
        "ra" => ("audio/vnd.rn-realaudio", Audio),
        "dsf" => ("audio/x-dsf", Audio),
        "dff" => ("audio/x-dff", Audio),
        "tak" => ("audio/x-tak", Audio),
        "shn" => ("audio/x-shorten", Audio),
        "s3m" => ("audio/x-s3m", Audio),
        "xm" => ("audio/x-xm", Audio),
        "it" => ("audio/x-it", Audio),
        "txt" | "log" | "ini" | "conf" | "cfg" | "nfo" => ("text/plain", Text),
        "csv" => ("text/csv", Text),
        "tsv" => ("text/tab-separated-values", Text),
        e if is_markdown_extension(e) => ("text/markdown", Markdown),
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
        "cbz" => ("application/vnd.comicbook+zip", Archive),
        "cb7" => ("application/x-cb7", Archive),
        "cbt" => ("application/x-cbt", Archive),
        "cbr" => ("application/vnd.comicbook-rar", Archive),
        "zipx" => ("application/x-zipx", Archive),
        "z" => ("application/x-compress", Archive),
        "lz4" => ("application/x-lz4", Archive),
        "cab" => ("application/vnd.ms-cab-compressed", Archive),
        "cpio" => ("application/x-cpio", Archive),
        "lzh" | "lha" => ("application/x-lzh-compressed", Archive),
        "xar" => ("application/x-xar", Archive),
        "lzo" => ("application/x-lzop", Archive),
        "lz" => ("application/x-lzip", Archive),
        "lzma" => ("application/x-lzma", Archive),
        "tlz" => ("application/x-lzma-compressed-tar", Archive),
        "ar" => ("application/x-archive", Archive),
        "arj" => ("application/x-arj", Archive),
        "br" => ("application/x-brotli", Archive),
        "warc" => ("application/warc", Archive),
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
        "docm" => ("application/vnd.ms-word.document.macroEnabled.12", Document),
        "dotx" => (
            "application/vnd.openxmlformats-officedocument.wordprocessingml.template",
            Document,
        ),
        "dotm" => ("application/vnd.ms-word.template.macroEnabled.12", Document),
        "fodt" => ("application/vnd.oasis.opendocument.text-flat-xml", Document),
        "sxw" => ("application/vnd.sun.xml.writer", Document),
        "sdw" => ("application/vnd.stardivision.writer", Document),
        "pages" => ("application/vnd.apple.pages", Document),
        "wps" => ("application/vnd.ms-works", Document),
        "wpd" | "wp" => ("application/vnd.wordperfect", Document),
        "wri" => ("application/x-mswrite", Document),
        "hwp" => ("application/x-hwp", Document),
        "xps" => ("application/vnd.ms-xpsdocument", Document),
        "mobi" => ("application/x-mobipocket-ebook", Document),
        "azw3" => ("application/vnd.amazon.mobi8-ebook", Document),
        "fb2" => ("application/x-fictionbook+xml", Document),
        "lit" => ("application/x-ms-reader", Document),
        "lrf" => ("application/x-sony-bbeb", Document),
        "chm" => ("application/vnd.ms-htmlhelp", Document),
        "ibooks" => ("application/x-ibooks+zip", Document),
        "indd" => ("application/x-adobe-indesign", Document),
        "idml" => ("application/vnd.adobe.indesign-idml-package", Document),
        "mht" | "mhtml" => ("application/x-mimearchive", Document),
        "xlsm" => ("application/vnd.ms-excel.sheet.macroEnabled.12", Spreadsheet),
        "xlsb" => (
            "application/vnd.ms-excel.sheet.binary.macroEnabled.12",
            Spreadsheet,
        ),
        "xltx" => (
            "application/vnd.openxmlformats-officedocument.spreadsheetml.template",
            Spreadsheet,
        ),
        "xlt" => ("application/vnd.ms-excel", Spreadsheet),
        "xltm" => ("application/vnd.ms-excel.template.macroEnabled.12", Spreadsheet),
        "numbers" => ("application/vnd.apple.numbers", Spreadsheet),
        "fods" => (
            "application/vnd.oasis.opendocument.spreadsheet-flat-xml",
            Spreadsheet,
        ),
        "sxc" => ("application/vnd.sun.xml.calc", Spreadsheet),
        "wk1" | "wk3" | "wk4" | "wks" | "123" => ("application/vnd.lotus-1-2-3", Spreadsheet),
        "wq1" | "wq2" | "wb1" | "wb2" | "wb3" | "qpw" => ("application/x-quattropro", Spreadsheet),
        "pptm" => (
            "application/vnd.ms-powerpoint.presentation.macroEnabled.12",
            Presentation,
        ),
        "ppsx" => (
            "application/vnd.openxmlformats-officedocument.presentationml.slideshow",
            Presentation,
        ),
        "ppsm" => (
            "application/vnd.ms-powerpoint.slideshow.macroEnabled.12",
            Presentation,
        ),
        "pps" => ("application/vnd.ms-powerpoint", Presentation),
        "potx" => (
            "application/vnd.openxmlformats-officedocument.presentationml.template",
            Presentation,
        ),
        "potm" => (
            "application/vnd.ms-powerpoint.template.macroEnabled.12",
            Presentation,
        ),
        "thmx" => ("application/vnd.ms-officetheme", Presentation),
        "fodp" => (
            "application/vnd.oasis.opendocument.presentation-flat-xml",
            Presentation,
        ),
        "sxi" => ("application/vnd.sun.xml.impress", Presentation),
        "sdd" => ("application/vnd.stardivision.impress", Presentation),
        "mdb" | "accdb" => ("application/x-msaccess", Database),
        "dbf" => ("application/x-dbf", Database),
        "odb" => ("application/vnd.oasis.opendocument.database", Database),
        "sas7bdat" => ("application/x-sas-data", Database),
        "srt" => ("application/x-subrip", Text),
        "vtt" => ("text/vtt", Text),
        "ass" | "ssa" => ("text/x-ssa", Text),
        "lrc" => ("text/x-lrc", Text),
        "eml" | "emlx" => ("message/rfc822", Mail),
        "mbox" => ("application/mbox", Mail),
        "msg" => ("application/vnd.ms-outlook", Mail),
        "pst" => ("application/vnd.ms-outlook-pst", Mail),
        "ics" | "ical" => ("text/calendar", Calendar),
        "vcs" => ("text/x-vcalendar", Calendar),
        "vcf" | "vcard" => ("text/vcard", Contact),
        "one" | "onepkg" | "onetoc2" => ("application/onenote", Notebook),
        "pem" | "crt" => ("application/x-pem-file", Key),
        "asc" => ("application/pgp-keys", Key),
        "cer" | "der" => ("application/pkix-cert", Key),
        "gpg" | "pgp" => ("application/pgp-encrypted", Key),
        "p12" | "pfx" => ("application/x-pkcs12", Key),
        "p7b" | "p7c" => ("application/x-pkcs7-certificates", Key),
        "p7m" => ("application/pkcs7-mime", Key),
        "p7s" => ("application/pkcs7-signature", Key),
        "jks" => ("application/x-java-keystore", Key),
        "kdbx" => ("application/x-keepass2", Key),
        "exe" | "dll" => ("application/x-msdownload", Executable),
        "msi" => ("application/x-msi", Executable),
        "bat" | "cmd" => ("application/x-bat", Executable),
        "class" => ("application/java-vm", Executable),
        "appimage" => ("application/vnd.appimage", Executable),
        "ttf" => ("font/ttf", Font),
        "otf" => ("font/otf", Font),
        "woff" => ("font/woff", Font),
        "woff2" => ("font/woff2", Font),
        "ttc" => ("font/collection", Font),
        "pfb" => ("application/x-font-type1", Font),
        "afm" => ("application/x-font-afm", Font),
        "iso" => ("application/x-iso9660-image", DiskImage),
        "img" => ("application/x-raw-disk-image", DiskImage),
        "dmg" => ("application/x-apple-diskimage", DiskImage),
        "vhd" | "vhdx" => ("application/x-vhd", DiskImage),
        "vmdk" => ("application/x-vmdk", DiskImage),
        "qcow2" => ("application/x-qemu-disk", DiskImage),
        "dwg" => ("image/vnd.dwg", Cad),
        "dxf" => ("image/vnd.dxf", Cad),
        "dgn" => ("application/x-dgn", Cad),
        "stl" => ("model/stl", Cad),
        "3mf" => ("model/3mf", Cad),
        "step" | "stp" => ("model/step", Cad),
        "iges" | "igs" => ("model/iges", Cad),
        "gltf" => ("model/gltf+json", Cad),
        "glb" => ("model/gltf-binary", Cad),
        "skp" => ("application/vnd.sketchup.skp", Cad),
        "sldprt" | "sldasm" | "slddrw" => ("application/x-solidworks", Cad),
        "fcstd" => ("application/x-freecad", Cad),
        "blend" => ("application/x-blender", Cad),
        "rpm" => ("application/x-rpm", Package),
        "apk" => ("application/vnd.android.package-archive", Package),
        _ => return None,
    })
}

const MIME_PREFIXES: &[(&str, FileCategory)] = &[
    ("text/calendar", FileCategory::Calendar),
    ("text/x-vcalendar", FileCategory::Calendar),
    ("text/vcard", FileCategory::Contact),
    ("text/x-vcard", FileCategory::Contact),
    ("text/markdown", FileCategory::Markdown),
    ("text/x-", FileCategory::Code),
    ("text/html", FileCategory::Code),
    ("text/css", FileCategory::Code),
    ("text/javascript", FileCategory::Code),
    ("text/", FileCategory::Text),
    ("image/vnd.dwg", FileCategory::Cad),
    ("image/vnd.dxf", FileCategory::Cad),
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
    ("message/rfc822", FileCategory::Mail),
    ("application/mbox", FileCategory::Mail),
    ("application/vnd.ms-outlook", FileCategory::Mail),
    ("application/onenote", FileCategory::Notebook),
    ("application/x-pem-file", FileCategory::Key),
    ("application/pgp-keys", FileCategory::Key),
    ("application/pkix-cert", FileCategory::Key),
    ("application/x-x509-ca-cert", FileCategory::Key),
    ("application/x-pkcs12", FileCategory::Key),
    ("application/x-pkcs7", FileCategory::Key),
    ("application/pkcs7", FileCategory::Key),
    ("application/x-executable", FileCategory::Executable),
    ("application/x-msdownload", FileCategory::Executable),
    ("application/x-dosexec", FileCategory::Executable),
    ("font/", FileCategory::Font),
    ("application/font-", FileCategory::Font),
    ("application/x-font-", FileCategory::Font),
    ("application/x-iso9660-image", FileCategory::DiskImage),
    ("application/x-apple-diskimage", FileCategory::DiskImage),
    ("model/", FileCategory::Cad),
    ("inode/directory", FileCategory::Folder),
];

pub fn category_from_mime(mime: &str) -> FileCategory {
    let base = mime.split(';').next().unwrap_or("").trim().to_ascii_lowercase();
    MIME_PREFIXES
        .iter()
        .find(|(prefix, _)| base.starts_with(prefix))
        .map_or(FileCategory::Other, |(_, cat)| *cat)
}

fn by_name(name: &[u8], content_type: Option<&str>) -> Option<(&'static str, FileCategory)> {
    let ext = extension_of(name)?;
    if ext == "ts" && content_type.is_some_and(|ct| category_from_mime(ct) == FileCategory::Video) {
        return Some((MPEG_TS, FileCategory::Video));
    }
    from_extension(&ext)
}

pub fn category_of_typed(name: &[u8], content_type: Option<&str>) -> FileCategory {
    match by_name(name, content_type) {
        Some((_, category)) => category,
        None => content_type.map_or(FileCategory::Other, category_from_mime),
    }
}

pub fn category_of_name(name: &[u8]) -> FileCategory {
    extension_of(name)
        .and_then(|e| from_extension(&e))
        .map_or(FileCategory::Other, |(_, c)| c)
}

pub fn category_of(entry: &Entry) -> FileCategory {
    if entry.is_dir() {
        return FileCategory::Folder;
    }
    if entry.kind == Kind::Special {
        return FileCategory::Other;
    }
    category_of_typed(&entry.name, entry.content_type.as_deref())
}

pub fn mime_of(entry: &Entry) -> Option<String> {
    if entry.is_dir() {
        return Some("inode/directory".to_owned());
    }
    by_name(&entry.name, entry.content_type.as_deref())
        .map(|(m, _)| m.to_owned())
        .or_else(|| entry.content_type.clone())
}

pub const ANY_FILE: &str = "application/octet-stream";

pub fn share_type<'a>(types: impl IntoIterator<Item = Option<&'a str>>) -> String {
    let types: Option<Vec<&str>> = types.into_iter().collect();
    let Some((first, rest)) = types.as_deref().and_then(<[&str]>::split_first) else {
        return ANY_FILE.to_owned();
    };
    if rest.iter().all(|t| t == first) {
        return (*first).to_owned();
    }
    let group = |t: &str| t.split_once('/').map(|(g, _)| g.to_owned());
    match group(first) {
        Some(g) if rest.iter().all(|t| group(t).as_ref() == Some(&g)) => format!("{g}/*"),
        _ => ANY_FILE.to_owned(),
    }
}

const MPEG_TS: &str = "video/mp2t";

const MPEG_TS_SNIFF_BYTES: u64 = 1024;

pub fn is_mpeg_ts(head: &[u8]) -> bool {
    [(188, 0), (192, 4)].into_iter().any(|(packet, sync_at)| {
        let syncs: Vec<u8> = head.iter().skip(sync_at).step_by(packet).copied().collect();
        syncs.len() >= 3 && syncs.iter().all(|b| *b == 0x47)
    })
}

pub fn sniff_local(name: &[u8], path: &Path) -> Option<&'static str> {
    if extension_of(name).as_deref() != Some("ts") || !std::fs::metadata(path).ok()?.is_file() {
        return None;
    }
    let mut head = Vec::new();
    std::fs::File::open(path)
        .ok()?
        .take(MPEG_TS_SNIFF_BYTES)
        .read_to_end(&mut head)
        .ok()?;
    is_mpeg_ts(&head).then_some(MPEG_TS)
}

pub fn viewer_of(name: &[u8], mime: &str, local: Option<&Path>) -> Viewer {
    let mime = match local.and_then(|path| sniff_local(name, path)) {
        Some(sniffed) => sniffed,
        None if mime.is_empty() => by_name(name, None).map_or("", |(m, _)| m),
        None => mime,
    };
    viewer_for_name(name, category_of_typed(name, Some(mime)), mime)
}

const TEXT_SNIFF_MAX_CONTROL_PERCENT: usize = 2;

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

pub(crate) fn looks_like_text(head: &[u8]) -> bool {
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

pub fn viewer_for_name(name: &[u8], category: FileCategory, mime: &str) -> Viewer {
    match viewer_for(category, mime) {
        Viewer::Text if crate::preview::text::reads_as_prose(name) => Viewer::Prose,
        viewer => viewer,
    }
}

pub fn viewer_for(category: FileCategory, mime: &str) -> Viewer {
    match category {
        FileCategory::Image | FileCategory::Video | FileCategory::Audio if !shows_in_app(mime) => {
            Viewer::External
        }
        FileCategory::Image => Viewer::Image,
        FileCategory::Text | FileCategory::Code => Viewer::Text,
        FileCategory::Markdown => Viewer::Markdown,
        FileCategory::Audio => Viewer::Audio,
        FileCategory::Video => Viewer::Video,
        FileCategory::Pdf => Viewer::Pdf,
        FileCategory::Archive => archive_viewer(mime),
        FileCategory::Package if mime == "application/vnd.android.package-archive" => Viewer::Archive,
        FileCategory::Other => other_viewer(mime),
        FileCategory::Mail | FileCategory::Key | FileCategory::Executable if TEXT_MIMES.contains(&mime) => {
            Viewer::Text
        }
        FileCategory::Folder
        | FileCategory::Database
        | FileCategory::Package
        | FileCategory::Document
        | FileCategory::Spreadsheet
        | FileCategory::Presentation
        | FileCategory::Mail
        | FileCategory::Calendar
        | FileCategory::Contact
        | FileCategory::Notebook
        | FileCategory::Key
        | FileCategory::Executable
        | FileCategory::Font
        | FileCategory::DiskImage
        | FileCategory::Cad => Viewer::External,
    }
}

const TEXT_MIMES: [&str; 5] = [
    "message/rfc822",
    "application/mbox",
    "application/x-pem-file",
    "application/pgp-keys",
    "application/x-bat",
];

const NOT_IN_APP: [&str; 74] = [
    "image/heic",
    "image/heif",
    "image/heic-sequence",
    "image/heif-sequence",
    "image/avif",
    "image/jxl",
    "image/x-tga",
    "image/vnd.adobe.photoshop",
    "image/x-exr",
    "image/vnd.radiance",
    "image/jp2",
    "image/jpm",
    "image/vnd-ms.dds",
    "image/x-sgi",
    "image/vnd.zbrush.pcx",
    "image/qoi",
    "image/bpg",
    "image/vnd.djvu",
    "image/emf",
    "image/wmf",
    "image/x-icns",
    "image/x-adobe-dng",
    "image/x-nikon-nef",
    "image/x-canon-cr2",
    "image/x-canon-cr3",
    "image/x-canon-crw",
    "image/x-sony-arw",
    "image/x-olympus-orf",
    "image/x-panasonic-rw2",
    "image/x-pentax-pef",
    "image/x-epson-erf",
    "image/x-kodak-kdc",
    "image/x-fuji-raf",
    "image/x-samsung-srw",
    "video/x-ms-wmv",
    "video/x-ms-asf",
    "application/mxf",
    "application/vnd.rn-realmedia",
    "video/x-wtv",
    "video/x-nuv",
    "audio/x-ms-wma",
    "audio/midi",
    "audio/amr",
    "audio/x-wavpack",
    "audio/x-ape",
    "audio/x-musepack",
    "audio/x-tta",
    "audio/eac3",
    "audio/vnd.dts",
    "audio/vnd.rn-realaudio",
    "audio/x-dsf",
    "audio/x-dff",
    "audio/x-tak",
    "audio/x-shorten",
    "audio/x-s3m",
    "audio/x-xm",
    "audio/x-it",
    "application/vnd.rar",
    "application/vnd.comicbook-rar",
    "application/x-zipx",
    "application/x-compress",
    "application/x-lz4",
    "application/vnd.ms-cab-compressed",
    "application/x-cpio",
    "application/x-lzh-compressed",
    "application/x-xar",
    "application/x-lzop",
    "application/x-lzip",
    "application/x-lzma",
    "application/x-lzma-compressed-tar",
    "application/x-archive",
    "application/x-arj",
    "application/x-brotli",
    "application/warc",
];

pub fn shows_in_app(mime: &str) -> bool {
    let base = mime.split(';').next().unwrap_or("").trim().to_ascii_lowercase();
    !NOT_IN_APP.contains(&base.as_str())
}

fn archive_viewer(mime: &str) -> Viewer {
    if !shows_in_app(mime) {
        Viewer::External
    } else {
        Viewer::Archive
    }
}

fn other_viewer(mime: &str) -> Viewer {
    match category_from_mime(mime) {
        FileCategory::Text | FileCategory::Code => Viewer::Text,
        _ => Viewer::External,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn file(name: &str) -> Entry {
        Entry::new(name.as_bytes(), Kind::File)
    }

    #[test]
    fn one_shared_file_keeps_its_type() {
        assert_eq!(share_type([Some("video/mp4")]), "video/mp4");
        assert_eq!(share_type([Some("image/jpeg"), Some("image/jpeg")]), "image/jpeg");
    }

    #[test]
    fn shared_files_of_one_group_share_the_group() {
        assert_eq!(share_type([Some("image/jpeg"), Some("image/png")]), "image/*");
        assert_eq!(
            share_type([Some("video/mp4"), Some("video/webm"), Some("video/mp4")]),
            "video/*"
        );
    }

    #[test]
    fn mixed_or_unknown_shared_files_are_any_file() {
        assert_eq!(share_type([Some("image/jpeg"), Some("video/mp4")]), ANY_FILE);
        assert_eq!(
            share_type([Some("image/jpeg"), Some("image/png"), Some("video/mp4")]),
            ANY_FILE
        );
        assert_eq!(share_type([Some("image/jpeg"), None]), ANY_FILE);
        assert_eq!(share_type([None]), ANY_FILE);
        assert_eq!(share_type([Some("image"), Some("image/png")]), ANY_FILE);
        assert_eq!(share_type(std::iter::empty()), ANY_FILE);
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
            ("a.MTS", FileCategory::Video),
            ("a.m2ts", FileCategory::Video),
            ("a.ts", FileCategory::Code),
            ("a.flac", FileCategory::Audio),
            ("a.txt", FileCategory::Text),
            ("a.rs", FileCategory::Code),
            ("a.MD", FileCategory::Markdown),
            ("a.mkd", FileCategory::Markdown),
            ("a.mdown", FileCategory::Markdown),
            ("a.mdwn", FileCategory::Markdown),
            ("a.Markdn", FileCategory::Markdown),
            ("a.markdown", FileCategory::Markdown),
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
        assert_eq!(
            mime_of(&file("a.tsv")).as_deref(),
            Some("text/tab-separated-values")
        );
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
        e.content_type = Some("image/png; charset=x".into());
        assert_eq!(category_of(&e), FileCategory::Image);
        assert_eq!(mime_of(&e).as_deref(), Some("image/png; charset=x"));
        let mut e = file("a.txt");
        e.content_type = Some("image/png".into());
        assert_eq!(category_of(&e), FileCategory::Text);
        assert_eq!(mime_of(&e).as_deref(), Some("text/plain"));
    }

    #[test]
    fn images_without_qt_decoder_show_the_image_glyph_and_open_with_other_apps() {
        for (name, mime) in [
            ("a.heic", "image/heic"),
            ("a.HEIF", "image/heic"),
            ("a.avif", "image/avif"),
            ("a.jxl", "image/jxl"),
        ] {
            let mut e = file(name);
            assert_eq!(category_of(&e), FileCategory::Image, "{name}");
            assert_eq!(mime_of(&e).as_deref(), Some(mime), "{name}");
            e.content_type = Some(mime.into());
            assert_eq!(category_of(&e), FileCategory::Image, "{name} from server");
            assert_eq!(
                viewer_for_name(name.as_bytes(), FileCategory::Image, mime),
                Viewer::External
            );
        }
        for mime in [
            "image/heic",
            "image/heif",
            "image/avif",
            "image/jxl",
            "image/heic-sequence",
            "IMAGE/HEIC; profile=x",
        ] {
            assert_eq!(viewer_of(b"noext", mime, None), Viewer::External, "{mime}");
            assert!(!shows_in_app(mime), "{mime}");
        }
        assert_eq!(category_from_mime("image/png"), FileCategory::Image);
    }

    #[test]
    fn media_and_archives_get_their_kind_and_open_in_lautta_only_when_the_phone_decodes_them() {
        use FileCategory::*;
        let cases: &[(&[&str], FileCategory, Viewer)] = &[
            (&["pbm", "pgm", "ppm", "pnm", "cur"], Image, Viewer::Image),
            (
                &[
                    "tga", "psd", "exr", "hdr", "jp2", "j2k", "jpf", "jpx", "jpm", "dds", "sgi", "pcx",
                    "qoi", "bpg", "djvu", "djv", "emf", "wmf", "icns", "dng", "nef", "nrw", "cr2", "cr3",
                    "crw", "arw", "srf", "sr2", "orf", "rw2", "pef", "erf", "kdc", "raf", "srw",
                ],
                Image,
                Viewer::External,
            ),
            (&["ivf", "vob", "m2v", "qt", "divx"], Video, Viewer::Video),
            (
                &["mxf", "rm", "rmvb", "asf", "wtv", "nuv", "wmv"],
                Video,
                Viewer::External,
            ),
            (
                &[
                    "mka", "aif", "aiff", "aifc", "ac3", "caf", "au", "snd", "spx", "w64", "voc",
                ],
                Audio,
                Viewer::Audio,
            ),
            (
                &[
                    "wv", "ape", "mpc", "tta", "eac3", "dts", "ra", "dsf", "dff", "tak", "shn", "s3m", "xm",
                    "it", "wma", "mid", "midi", "amr",
                ],
                Audio,
                Viewer::External,
            ),
            (&["cbz", "cb7", "cbt"], Archive, Viewer::Archive),
            (
                &[
                    "cbr", "zipx", "z", "lz4", "cab", "cpio", "lzh", "lha", "xar", "lzo", "lz", "lzma",
                    "tlz", "ar", "arj", "br", "warc", "rar",
                ],
                Archive,
                Viewer::External,
            ),
        ];
        for (exts, category, viewer) in cases {
            for ext in exts.iter() {
                let name = format!("x.{}", ext.to_ascii_uppercase());
                assert_eq!(category_of_name(name.as_bytes()), *category, "{ext}");
                assert_eq!(viewer_of(name.as_bytes(), "", None), *viewer, "{ext}");
            }
        }
        assert_eq!(category_of_name(b"go.mod"), FileCategory::Other);
        assert_eq!(viewer_of(b"a.jpg", "", None), Viewer::Image);
        assert_eq!(viewer_of(b"a.mp3", "", None), Viewer::Audio);
        assert_eq!(viewer_of(b"a.zip", "", None), Viewer::Archive);
    }

    #[test]
    fn ts_is_video_only_by_video_content_type() {
        let mut e = file("rec.ts");
        assert_eq!(mime_of(&e).as_deref(), Some("text/x-typescript"));
        for (ct, cat, mime) in [
            ("video/mp2t", FileCategory::Video, "video/mp2t"),
            ("video/MP2T", FileCategory::Video, "video/mp2t"),
            ("text/x-typescript", FileCategory::Code, "text/x-typescript"),
            (
                "application/octet-stream",
                FileCategory::Code,
                "text/x-typescript",
            ),
        ] {
            e.content_type = Some(ct.into());
            assert_eq!(category_of(&e), cat, "{ct}");
            assert_eq!(mime_of(&e).as_deref(), Some(mime), "{ct}");
        }
        let mut e = file("a.rs");
        e.content_type = Some("video/mp2t".into());
        assert_eq!(
            category_of(&e),
            FileCategory::Code,
            "only .ts asks the content type"
        );
    }

    fn transport_stream(packet: usize, sync_at: usize, count: usize) -> Vec<u8> {
        let mut bytes = vec![b'x'; packet * count];
        for start in (0..bytes.len()).step_by(packet) {
            bytes[start + sync_at] = 0x47;
        }
        bytes
    }

    #[test]
    fn mpeg_ts_needs_three_sync_bytes_a_packet_apart() {
        let ts = transport_stream(188, 0, 5);
        assert!(is_mpeg_ts(&ts));
        assert!(is_mpeg_ts(&ts[..2 * 188 + 1]), "three syncs");
        assert!(!is_mpeg_ts(&ts[..2 * 188]), "two syncs");
        assert!(is_mpeg_ts(&transport_stream(192, 4, 5)), "M2TS");
        assert!(
            !is_mpeg_ts(&transport_stream(192, 0, 5)),
            "192-byte packets sync after time code"
        );
        let mut broken = ts.clone();
        broken[3 * 188] = 0x46;
        assert!(!is_mpeg_ts(&broken));
        assert!(!is_mpeg_ts(b""));
        assert!(!is_mpeg_ts("Gx\n".repeat(300).as_bytes()));
        assert!(!is_mpeg_ts(b"export const answer: number = 42;\n"));
    }

    #[test]
    fn local_ts_is_sniffed_by_content() {
        let d = tempfile::tempdir().unwrap();
        let ts = transport_stream(188, 0, 40);
        let write = |name: &str, bytes: &[u8]| {
            let p = d.path().join(name);
            std::fs::write(&p, bytes).unwrap();
            p
        };
        let video = write("rec.ts", &ts);
        let m2ts = write("cam.ts", &transport_stream(192, 4, 40));
        let code = write("app.ts", b"export function hello(name: string) {}\n");
        let not_ts = write("rec.rs", &ts);
        let dir = d.path().join("dir.ts");
        std::fs::create_dir(&dir).unwrap();
        assert_eq!(sniff_local(b"rec.ts", &video), Some("video/mp2t"));
        assert_eq!(sniff_local(b"cam.TS", &m2ts), Some("video/mp2t"));
        assert_eq!(sniff_local(b"app.ts", &code), None);
        assert_eq!(sniff_local(b"rec.rs", &not_ts), None);
        assert_eq!(sniff_local(b"dir.ts", &dir), None);
        assert_eq!(sniff_local(b"gone.ts", &d.path().join("gone.ts")), None);

        assert_eq!(viewer_of(b"rec.ts", "", Some(&video)), Viewer::Video);
        assert_eq!(
            viewer_of(b"rec.ts", "text/x-typescript", Some(&video)),
            Viewer::Video,
            "file beats a stale listed type"
        );
        assert_eq!(viewer_of(b"app.ts", "", Some(&code)), Viewer::Text);
        assert_eq!(viewer_of(b"rec.rs", "", Some(&not_ts)), Viewer::Text);
    }

    #[test]
    fn viewer_of_without_local_file() {
        assert_eq!(viewer_of(b"a.ts", "", None), Viewer::Text);
        assert_eq!(viewer_of(b"a.ts", "video/mp2t", None), Viewer::Video);
        assert_eq!(viewer_of(b"a.m2ts", "", None), Viewer::Video);
        assert_eq!(viewer_of(b"a.txt", "", None), Viewer::Prose);
        assert_eq!(viewer_of(b"a.rar", "", None), Viewer::External);
        assert_eq!(viewer_of(b"a.apk", "", None), Viewer::Archive);
        assert_eq!(viewer_of(b"noext", "text/plain", None), Viewer::Text);
        assert_eq!(viewer_of(b"noext", "image/png", None), Viewer::Image);
        assert_eq!(viewer_of(b"noext", "image/heic", None), Viewer::External);
        assert_eq!(viewer_of(b"noext", "", None), Viewer::External);
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
            "rpm", "apk", "rs", "html", "m2ts",
        ] {
            let Some((mime, cat)) = from_extension(ext) else {
                panic!("missing {ext}");
            };
            assert_eq!(category_from_mime(mime), cat, "{ext} / {mime}");
        }
    }

    #[test]
    fn office_ebook_database_and_subtitle_variants_get_their_category() {
        use FileCategory::*;
        let want: [(&[&str], FileCategory); 5] = [
            (
                &[
                    "docm", "dotx", "dotm", "fodt", "sxw", "sdw", "pages", "wps", "wpd", "wp", "wri", "hwp",
                    "xps", "mobi", "azw3", "fb2", "lit", "lrf", "chm", "ibooks", "indd", "idml", "mht",
                    "mhtml",
                ],
                Document,
            ),
            (
                &[
                    "xlsm", "xlsb", "xltx", "xlt", "xltm", "numbers", "fods", "sxc", "wk1", "wk3", "wk4",
                    "wks", "123", "wq1", "wq2", "wb1", "wb2", "wb3", "qpw",
                ],
                Spreadsheet,
            ),
            (
                &[
                    "pptm", "ppsx", "ppsm", "pps", "potx", "potm", "thmx", "fodp", "sxi", "sdd",
                ],
                Presentation,
            ),
            (&["mdb", "accdb", "dbf", "odb", "sas7bdat"], Database),
            (&["srt", "vtt", "ass", "ssa", "lrc"], Text),
        ];
        for (exts, category) in want {
            for ext in exts {
                let name = format!("Some file.{}", ext.to_ascii_uppercase());
                assert_eq!(category_of_name(name.as_bytes()), category, "{ext}");
            }
        }
        assert_eq!(viewer_of(b"film.srt", "", None), Viewer::Text);
        assert_eq!(viewer_of(b"sheet.xlsm", "", None), Viewer::External);
    }

    #[test]
    fn mail_calendar_contacts_keys_programs_fonts_disks_and_models_get_their_category() {
        use FileCategory::*;
        let want: [(&[&str], FileCategory); 9] = [
            (&["eml", "emlx", "mbox", "msg", "pst"], Mail),
            (&["ics", "ical", "vcs"], Calendar),
            (&["vcf", "vcard"], Contact),
            (&["one", "onepkg", "onetoc2"], Notebook),
            (
                &[
                    "pem", "crt", "asc", "cer", "der", "gpg", "pgp", "p12", "pfx", "p7b", "p7c", "p7m",
                    "p7s", "jks", "kdbx",
                ],
                Key,
            ),
            (
                &["exe", "dll", "msi", "bat", "cmd", "class", "appimage"],
                Executable,
            ),
            (&["ttf", "otf", "woff", "woff2", "ttc", "pfb", "afm"], Font),
            (&["iso", "img", "dmg", "vhd", "vhdx", "vmdk", "qcow2"], DiskImage),
            (
                &[
                    "dwg", "dxf", "dgn", "stl", "3mf", "step", "stp", "iges", "igs", "gltf", "glb", "skp",
                    "sldprt", "sldasm", "slddrw", "fcstd", "blend",
                ],
                Cad,
            ),
        ];
        for (exts, category) in want {
            for ext in exts {
                let name = format!("x.{}", ext.to_ascii_uppercase());
                assert_eq!(category_of_name(name.as_bytes()), category, "{ext}");
            }
        }
    }

    #[test]
    fn text_based_mail_keys_and_scripts_open_as_text_the_rest_elsewhere() {
        for name in ["a.eml", "a.mbox", "a.pem", "a.crt", "a.asc", "a.bat"] {
            assert_eq!(viewer_of(name.as_bytes(), "", None), Viewer::Text, "{name}");
        }
        for name in [
            "a.msg", "a.ics", "a.vcf", "a.one", "a.p12", "a.der", "a.exe", "a.ttf", "a.iso", "a.stl",
        ] {
            assert_eq!(viewer_of(name.as_bytes(), "", None), Viewer::External, "{name}");
        }
    }

    #[test]
    fn server_types_of_the_new_kinds() {
        use FileCategory::*;
        for (mime, category) in [
            ("message/rfc822", Mail),
            ("text/calendar; charset=utf-8", Calendar),
            ("text/x-vcard", Contact),
            ("application/onenote", Notebook),
            ("application/x-x509-ca-cert", Key),
            ("application/x-dosexec", Executable),
            ("font/woff2", Font),
            ("application/x-font-ttf", Font),
            ("application/x-iso9660-image", DiskImage),
            ("model/stl", Cad),
            ("image/vnd.dwg", Cad),
            ("image/png", Image),
            ("text/plain", Text),
        ] {
            assert_eq!(category_from_mime(mime), category, "{mime}");
        }
        assert_eq!(
            category_from_mime(sniff(b"\x7fELF\x02\x01\x01").unwrap()),
            Executable
        );
    }

    #[test]
    fn names_shared_with_other_formats_stay_unknown() {
        for name in ["id_ed25519.pub", "server.key", "graph.dot", "subs.sub"] {
            assert_eq!(category_of_name(name.as_bytes()), FileCategory::Other, "{name}");
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
            FileCategory::Mail,
            FileCategory::Calendar,
            FileCategory::Contact,
            FileCategory::Notebook,
            FileCategory::Key,
            FileCategory::Executable,
            FileCategory::Font,
            FileCategory::DiskImage,
            FileCategory::Cad,
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
        let snow = "ab\u{2603}".as_bytes();
        assert_eq!(sniff(&snow[..snow.len() - 1]), Some("text/plain"));
        assert_eq!(sniff(b""), None);
        assert_eq!(sniff(b"abc\x00def"), None);
        assert_eq!(sniff(b"abc\xffdef"), None);
        assert_eq!(sniff(&[0x01, 0x02, 0x03, 0x04]), None);
        assert_eq!(sniff(&"\u{2603}".as_bytes()[..2]), None);
    }

    #[test]
    fn viewers() {
        use FileCategory::*;
        assert_eq!(viewer_for(Image, "image/png"), Viewer::Image);
        assert_eq!(viewer_for(Text, "text/plain"), Viewer::Text);
        assert_eq!(viewer_for(Code, "text/x-rust"), Viewer::Text);
        assert_eq!(viewer_for(Markdown, "text/markdown"), Viewer::Markdown);
        assert_eq!(viewer_for_name(b"a.TXT", Text, "text/plain"), Viewer::Prose);
        assert_eq!(viewer_for_name(b"a.log", Text, "text/plain"), Viewer::Text);
        assert_eq!(
            viewer_for_name(b"CMakeLists.txt", Text, "text/plain"),
            Viewer::Text
        );
        assert_eq!(
            viewer_for_name(b"a.txt", Image, "image/png"),
            Viewer::Image,
            "only text turns prose"
        );
        assert_eq!(
            viewer_for_name(b"a.md", Markdown, "text/markdown"),
            Viewer::Markdown
        );
        assert_eq!(viewer_for(Audio, "audio/mpeg"), Viewer::Audio);
        assert_eq!(viewer_for(Video, "video/mp4"), Viewer::Video);
        assert_eq!(viewer_for(Database, "application/vnd.sqlite3"), Viewer::External);
        assert_eq!(viewer_for(Archive, "application/zip"), Viewer::Archive);
        assert_eq!(viewer_for(Archive, "application/vnd.rar"), Viewer::External);
        assert_eq!(
            viewer_for(Package, "application/vnd.android.package-archive"),
            Viewer::Archive
        );
        assert_eq!(viewer_for(Package, "application/x-rpm"), Viewer::External);
        assert_eq!(viewer_for(Pdf, "application/pdf"), Viewer::Pdf);
        assert_eq!(viewer_for(Document, ""), Viewer::External);
        assert_eq!(viewer_for(Spreadsheet, ""), Viewer::External);
        assert_eq!(viewer_for(Presentation, ""), Viewer::External);
        assert_eq!(viewer_for(Folder, ""), Viewer::External);
        assert_eq!(viewer_for(Other, "application/octet-stream"), Viewer::External);
        assert_eq!(viewer_for(Other, "text/plain"), Viewer::Text);
        assert_eq!(viewer_for(Other, "text/x-foo"), Viewer::Text);
    }
}
