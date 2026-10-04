// SPDX-License-Identifier: LGPL-2.1-or-later
//! Archive provider tests: archives are built in memory with the same crates
//! the readers use (zip, tar, flate2, bzip2, xz2, zstd, sevenz-rust).

use super::*;
use crate::provider::memory::MemoryProvider;
use crate::provider::{list_all, no_progress, read_all};
use std::io::Cursor;

fn vp(s: &str) -> VPath {
    VPath::parse(s.as_bytes()).unwrap_or_default()
}

/// Incompressible deterministic bytes.
fn noise(len: usize, mut seed: u64) -> Vec<u8> {
    (0..len)
        .map(|_| {
            seed ^= seed << 13;
            seed ^= seed >> 7;
            seed ^= seed << 17;
            (seed >> 24) as u8
        })
        .collect()
}

fn zip_bytes(files: &[(&str, &[u8])], method: zip::CompressionMethod) -> Vec<u8> {
    let mut w = zip::ZipWriter::new(Cursor::new(Vec::new()));
    let opts = zip::write::FileOptions::default()
        .compression_method(method)
        .unix_permissions(0o640);
    for (name, data) in files {
        if name.ends_with('/') {
            w.add_directory(*name, opts).unwrap();
        } else {
            w.start_file(*name, opts).unwrap();
            w.write_all(data).unwrap();
        }
    }
    w.finish().unwrap().into_inner()
}

fn tar_add(b: &mut tar::Builder<Vec<u8>>, name: &str, data: &[u8], kind: tar::EntryType) {
    let mut h = tar::Header::new_gnu();
    h.set_size(data.len() as u64);
    h.set_mode(0o644);
    h.set_mtime(1_700_000_000);
    h.set_entry_type(kind);
    // Raw name bytes: the builder's own setters refuse `..`, hostile
    // archives do not.
    h.as_old_mut().name[..name.len()].copy_from_slice(name.as_bytes());
    h.set_cksum();
    b.append(&h, data).unwrap();
}

fn tar_bytes(files: &[(&str, &[u8])]) -> Vec<u8> {
    let mut b = tar::Builder::new(Vec::new());
    for (name, data) in files {
        let kind = if name.ends_with('/') {
            tar::EntryType::Directory
        } else {
            tar::EntryType::Regular
        };
        tar_add(&mut b, name, data, kind);
    }
    b.into_inner().unwrap()
}

fn compressed(comp: Compression, data: &[u8]) -> Vec<u8> {
    match comp {
        Compression::None => data.to_vec(),
        Compression::Gzip => {
            let mut e = flate2::write::GzEncoder::new(Vec::new(), flate2::Compression::fast());
            e.write_all(data).unwrap();
            e.finish().unwrap()
        }
        Compression::Bzip2 => {
            let mut e = bzip2::write::BzEncoder::new(Vec::new(), bzip2::Compression::fast());
            e.write_all(data).unwrap();
            e.finish().unwrap()
        }
        Compression::Xz => {
            let mut e = xz2::write::XzEncoder::new(Vec::new(), 1);
            e.write_all(data).unwrap();
            e.finish().unwrap()
        }
        Compression::Zstd => zstd::stream::encode_all(data, 1).unwrap(),
    }
}

fn sevenz_bytes(files: &[(&str, &[u8])]) -> Vec<u8> {
    let mut w = sevenz_rust::SevenZWriter::new(Cursor::new(Vec::new())).unwrap();
    for (name, data) in files {
        let is_dir = name.ends_with('/');
        let mut entry = sevenz_rust::SevenZArchiveEntry::new();
        entry.name = name.trim_end_matches('/').to_owned();
        entry.has_stream = !is_dir;
        entry.is_directory = is_dir;
        let reader = (!is_dir).then_some(*data);
        w.push_archive_entry(entry, reader).unwrap();
    }
    w.finish().unwrap().into_inner()
}

async fn open_named(name: &str, data: &[u8], cache: &Path) -> Result<ArchiveProvider> {
    let mem = MemoryProvider::default();
    mem.add_file(name, data, 0);
    ArchiveProvider::open(Arc::new(mem), vp(name), cache.to_path_buf()).await
}

async fn read_entry(p: &ArchiveProvider, path: &str) -> Vec<u8> {
    let h = p.open_read(&vp(path), Lane::Bulk).await.unwrap();
    read_all(h.as_ref(), 1 << 30).await.unwrap()
}

fn names(entries: &[Entry]) -> Vec<String> {
    let mut v: Vec<String> = entries.iter().map(Entry::display_name).collect();
    v.sort();
    v
}

const SAMPLE: [(&str, &[u8]); 3] = [
    ("top.txt", b"top level"),
    ("docs/readme.md", b"# hello"),
    ("docs/deep/er/file.bin", b"\x00\x01\x02\x03"),
];

#[tokio::test]
async fn zip_lists_with_implicit_directories_and_reads() {
    let tmp = tempfile::tempdir().unwrap();
    let data = zip_bytes(&SAMPLE, zip::CompressionMethod::Deflated);
    let p = open_named("a.zip", &data, tmp.path()).await.unwrap();
    assert_eq!(p.format(), ArchiveFormat::Zip);
    let root = list_all(&p, &VPath::root(), Lane::Interactive).await.unwrap();
    assert_eq!(names(&root), ["docs", "top.txt"]);
    let docs = root.iter().find(|e| e.name == b"docs").unwrap();
    assert_eq!(docs.kind, Kind::Dir, "implicit directory");
    let inner = list_all(&p, &vp("docs"), Lane::Interactive).await.unwrap();
    assert_eq!(names(&inner), ["deep", "readme.md"]);
    let st = p
        .stat(&vp("docs/readme.md"), false, Lane::Interactive)
        .await
        .unwrap();
    assert_eq!((st.kind, st.size, st.mode), (Kind::File, Some(7), Some(0o640)));
    assert!(st.modified.is_some());
    assert_eq!(read_entry(&p, "docs/deep/er/file.bin").await, [0, 1, 2, 3]);
    assert_eq!(read_entry(&p, "top.txt").await, b"top level");
}

#[tokio::test]
async fn zip_needs_only_ranged_reads() {
    let tmp = tempfile::tempdir().unwrap();
    let blobs: Vec<Vec<u8>> = (0..40).map(|i| noise(100_000, 7 + i)).collect();
    let names_list: Vec<String> = (0..40).map(|i| format!("dir/f{i:02}.bin")).collect();
    let files: Vec<(&str, &[u8])> = names_list
        .iter()
        .zip(&blobs)
        .map(|(n, b)| (n.as_str(), b.as_slice()))
        .collect();
    let data = zip_bytes(&files, zip::CompressionMethod::Stored);
    let total_blocks = data.len() as u64 / BLOCK;
    assert!(total_blocks > 50);
    let p = open_named("big.zip", &data, tmp.path()).await.unwrap();
    let after_open = p.source_reads();
    assert!(
        after_open <= 3,
        "opening read {after_open} ranges of {total_blocks} blocks"
    );
    assert_eq!(
        list_all(&p, &vp("dir"), Lane::Interactive).await.unwrap().len(),
        40
    );
    assert_eq!(p.source_reads(), after_open, "listing is served from the index");
    assert_eq!(read_entry(&p, "dir/f20.bin").await, blobs[20]);
    let after_read = p.source_reads() - after_open;
    assert!(
        (1..=4).contains(&after_read),
        "one 100 kB entry cost {after_read} reads"
    );
}

#[tokio::test]
async fn zip_corruption_is_detected() {
    let tmp = tempfile::tempdir().unwrap();
    let mut data = zip_bytes(&[("f.txt", b"abcdefghij")], zip::CompressionMethod::Stored);
    let at = data.windows(10).position(|w| w == b"abcdefghij").unwrap();
    data[at + 3] ^= 0xFF;
    let p = open_named("c.zip", &data, tmp.path()).await.unwrap();
    let h = p.open_read(&vp("f.txt"), Lane::Bulk).await;
    assert_eq!(h.err().map(|e| e.kind), Some(ErrorKind::ProtocolError));
}

#[tokio::test]
async fn truncated_zip_is_an_error() {
    let tmp = tempfile::tempdir().unwrap();
    let data = zip_bytes(&SAMPLE, zip::CompressionMethod::Deflated);
    let err = open_named("t.zip", &data[..data.len() - 30], tmp.path())
        .await
        .err();
    assert_eq!(err.map(|e| e.kind), Some(ErrorKind::ProtocolError));
}

#[tokio::test]
async fn plain_tar_is_indexed_through_ranges() {
    let tmp = tempfile::tempdir().unwrap();
    let big = noise(3_000_000, 3);
    let files: Vec<(&str, &[u8])> = vec![
        ("a/", b""),
        ("a/small.txt", b"small"),
        ("big.bin", &big),
        ("z/last.txt", b"last"),
    ];
    let data = tar_bytes(&files);
    let p = open_named("x.tar", &data, tmp.path()).await.unwrap();
    assert_eq!(p.format(), ArchiveFormat::Tar(Compression::None));
    assert!(p.source_reads() < 12, "headers only: {} reads", p.source_reads());
    assert_eq!(read_entry(&p, "a/small.txt").await, b"small");
    assert_eq!(read_entry(&p, "z/last.txt").await, b"last");
    assert_eq!(read_entry(&p, "big.bin").await.len(), 3_000_000);
}

#[tokio::test]
async fn compressed_tars_download_index_and_stream() {
    for (comp, name) in [
        (Compression::Gzip, "x.tar.gz"),
        (Compression::Bzip2, "x.tar.bz2"),
        (Compression::Xz, "x.tar.xz"),
        (Compression::Zstd, "x.tar.zst"),
    ] {
        let tmp = tempfile::tempdir().unwrap();
        let cache = tmp.path().join("cache");
        let data = compressed(comp, &tar_bytes(&SAMPLE));
        let p = open_named(name, &data, &cache).await.unwrap();
        assert_eq!(p.format(), ArchiveFormat::Tar(comp), "{name}");
        assert_eq!(p.source_reads(), 0);
        assert_eq!(read_entry(&p, "docs/readme.md").await, b"# hello", "{name}");
        assert_eq!(read_entry(&p, "top.txt").await, b"top level", "{name}");
        assert_eq!(
            read_entry(&p, "docs/deep/er/file.bin").await,
            [0, 1, 2, 3],
            "{name}"
        );
        assert_eq!(
            std::fs::read_dir(&cache).unwrap().count(),
            1,
            "downloaded copy kept while open"
        );
        drop(p);
        assert_eq!(
            std::fs::read_dir(&cache).unwrap().count(),
            0,
            "cache file removed on close"
        );
    }
}

#[tokio::test]
async fn compressed_tar_is_recognised_without_tar_extension() {
    let tmp = tempfile::tempdir().unwrap();
    let data = compressed(Compression::Gzip, &tar_bytes(&SAMPLE));
    let p = open_named("backup.dat", &data, tmp.path()).await.unwrap();
    assert_eq!(p.format(), ArchiveFormat::Tar(Compression::Gzip));
    let plain_gz = compressed(Compression::Gzip, b"just some text, not a tar");
    let err = open_named("notes.txt.gz", &plain_gz, tmp.path()).await.err();
    assert_eq!(err.map(|e| e.kind), Some(ErrorKind::Unsupported));
}

#[tokio::test]
async fn sevenz_lists_and_reads() {
    let tmp = tempfile::tempdir().unwrap();
    let files: [(&str, &[u8]); 4] = [
        ("docs/", b""),
        ("docs/readme.md", b"# hello"),
        ("top.txt", b"top level"),
        ("empty.txt", b""),
    ];
    let data = sevenz_bytes(&files);
    let p = open_named("a.7z", &data, tmp.path()).await.unwrap();
    assert_eq!(p.format(), ArchiveFormat::SevenZ);
    let root = list_all(&p, &VPath::root(), Lane::Interactive).await.unwrap();
    assert_eq!(names(&root), ["docs", "empty.txt", "top.txt"]);
    assert_eq!(read_entry(&p, "docs/readme.md").await, b"# hello");
    assert_eq!(read_entry(&p, "top.txt").await, b"top level");
    assert!(read_entry(&p, "empty.txt").await.is_empty());
}

fn hostile_entries() -> Vec<(&'static str, &'static [u8])> {
    vec![
        ("../evil.txt", b"x"),
        ("/abs/evil.txt", b"x"),
        ("a/../../evil.txt", b"x"),
        ("C:/evil.txt", b"x"),
        ("win\\..\\evil.txt", b"x"),
        ("good/ok.txt", b"fine"),
        ("./dotted.txt", b"dot"),
    ]
}

fn assert_only_safe(p: &ArchiveProvider) {
    let paths: Vec<String> = p.entries().iter().map(|e| e.path.display()).collect();
    assert_eq!(paths, ["dotted.txt", "good", "good/ok.txt"]);
    assert_eq!(p.skipped_entries(), 5);
}

#[tokio::test]
async fn tar_path_traversal_entries_are_not_indexed() {
    let tmp = tempfile::tempdir().unwrap();
    let p = open_named("h.tar", &tar_bytes(&hostile_entries()), tmp.path())
        .await
        .unwrap();
    assert_only_safe(&p);
    let miss = p.open_read(&vp("evil.txt"), Lane::Bulk).await;
    assert_eq!(miss.err().map(|e| e.kind), Some(ErrorKind::NotFound));
    assert_eq!(read_entry(&p, "good/ok.txt").await, b"fine");
}

#[tokio::test]
async fn zip_path_traversal_entries_are_not_indexed() {
    let tmp = tempfile::tempdir().unwrap();
    let data = zip_bytes(&hostile_entries(), zip::CompressionMethod::Deflated);
    let p = open_named("h.zip", &data, tmp.path()).await.unwrap();
    assert_only_safe(&p);
}

#[test]
fn sanitize_cases() {
    assert_eq!(sanitize(b"a/b"), Some(vp("a/b")));
    assert_eq!(sanitize(b"./a//b/"), Some(vp("a/b")));
    for bad in [&b"../a"[..], b"a/..", b"/a", b"a\\..\\b", b"c:\\x", b"a\0b"] {
        assert_eq!(sanitize(bad), None, "{}", String::from_utf8_lossy(bad));
    }
    assert_eq!(
        sanitize(b"..a/b..").map(|p| p.display()),
        Some("..a/b..".to_owned())
    );
}

#[test]
fn link_resolution_stays_inside() {
    assert_eq!(resolve_link(&vp("a/b"), b"../c"), Some(vp("a/c")));
    assert_eq!(resolve_link(&vp("a"), b"/etc/x"), Some(vp("etc/x")));
    assert_eq!(resolve_link(&vp("a"), b"../../x"), None);
    assert_eq!(resolve_link(&vp(""), b"./y/./z"), Some(vp("y/z")));
}

#[tokio::test]
async fn conflicting_entries_are_skipped_and_later_duplicates_win() {
    let tmp = tempfile::tempdir().unwrap();
    let data = tar_bytes(&[
        ("dir/file", b"1"),
        ("dir", b"a file where a folder is"),
        ("same.txt", b"old"),
        ("same.txt", b"newer"),
        ("same.txt/child", b"under a file"),
    ]);
    let p = open_named("c.tar", &data, tmp.path()).await.unwrap();
    assert_eq!(p.skipped_entries(), 2);
    assert_eq!(read_entry(&p, "same.txt").await, b"newer");
    assert_eq!(read_entry(&p, "dir/file").await, b"1");
}

#[tokio::test]
async fn tar_symlinks_resolve_inside_the_archive() {
    let tmp = tempfile::tempdir().unwrap();
    let mut b = tar::Builder::new(Vec::new());
    tar_add(&mut b, "real/", b"", tar::EntryType::Directory);
    tar_add(&mut b, "real/f.txt", b"payload", tar::EntryType::Regular);
    for (name, target) in [("link", "real"), ("dangling", "nowhere"), ("escape", "../../x")] {
        let mut h = tar::Header::new_gnu();
        h.set_entry_type(tar::EntryType::Symlink);
        h.set_size(0);
        b.append_link(&mut h, name, target).unwrap();
    }
    let p = open_named("l.tar", &b.into_inner().unwrap(), tmp.path())
        .await
        .unwrap();
    let root = list_all(&p, &VPath::root(), Lane::Interactive).await.unwrap();
    let find = |n: &str| root.iter().find(|e| e.name == n.as_bytes()).unwrap().clone();
    assert_eq!(find("link").target_kind, Kind::Dir);
    assert!(find("link").is_dir());
    assert!(find("dangling").flags.contains(EntryFlags::TARGET_UNKNOWN));
    assert!(find("escape").flags.contains(EntryFlags::TARGET_UNKNOWN));
    assert_eq!(p.read_link(&vp("link")).await.unwrap(), b"real");
    let st = p.stat(&vp("link"), true, Lane::Interactive).await.unwrap();
    assert_eq!((st.kind, st.name.as_slice()), (Kind::Dir, &b"link"[..]));
    let through = p.stat(&vp("link/f.txt"), false, Lane::Interactive).await;
    assert_eq!(through.err().map(|e| e.kind), Some(ErrorKind::NotFound));
    assert_eq!(
        p.read_link(&vp("real")).await.err().map(|e| e.kind),
        Some(ErrorKind::InvalidArgument)
    );
}

#[tokio::test]
async fn read_only_and_capabilities() {
    let tmp = tempfile::tempdir().unwrap();
    let data = zip_bytes(&SAMPLE, zip::CompressionMethod::Deflated);
    let p = open_named("a.zip", &data, tmp.path()).await.unwrap();
    let caps = p.capabilities();
    assert!(caps.has(cap::READ_ONLY) && caps.has(cap::RANDOM_READ) && !caps.writable());
    let ro = |r: Result<()>| r.err().map(|e| e.kind);
    let want = Some(ErrorKind::ReadOnlyFilesystem);
    assert_eq!(ro(p.make_dir(&vp("n"), true).await), want);
    assert_eq!(ro(p.make_file(&vp("n")).await), want);
    assert_eq!(ro(p.remove_file(&vp("top.txt")).await), want);
    assert_eq!(ro(p.remove_dir(&vp("docs")).await), want);
    assert_eq!(
        ro(p.rename(&vp("top.txt"), &vp("t"), RenameMode::NoReplace).await),
        want
    );
    assert_eq!(ro(p.make_symlink(b"x", &vp("l")).await), want);
    assert_eq!(ro(p.make_hardlink(&vp("top.txt"), &vp("h")).await), want);
    assert_eq!(
        ro(p.set_attributes(&vp("top.txt"), AttributeChanges::default())
            .await),
        want
    );
    let copy = p
        .server_copy(&vp("top.txt"), &vp("c"), CopyOptions::default())
        .await;
    assert_eq!(ro(copy), want);
    assert_eq!(
        p.stat(&vp("missing"), false, Lane::Bulk)
            .await
            .err()
            .map(|e| e.kind),
        Some(ErrorKind::NotFound)
    );
    let (tx, _rx) = mpsc::channel(1);
    assert_eq!(
        p.list(&vp("top.txt"), Lane::Bulk, tx).await.err().map(|e| e.kind),
        Some(ErrorKind::NotADirectory)
    );
    assert_eq!(
        p.open_read(&vp("docs"), Lane::Bulk).await.err().map(|e| e.kind),
        Some(ErrorKind::IsADirectory)
    );
    assert!(p.space(&VPath::root()).await.is_err());
}

#[tokio::test]
async fn download_into_streams_with_offset_and_progress() {
    let tmp = tempfile::tempdir().unwrap();
    let payload = noise(300_000, 11);
    let data = zip_bytes(&[("p.bin", &payload)], zip::CompressionMethod::Deflated);
    let p = open_named("a.zip", &data, tmp.path()).await.unwrap();
    let out_path = tmp.path().join("out.bin");
    let seen = Arc::new(std::sync::Mutex::new(Vec::new()));
    let sink: ProgressSink = {
        let seen = Arc::clone(&seen);
        Arc::new(move |done, total| seen.lock().unwrap().push((done, total)))
    };
    let fd = OwnedFd::from(File::create(&out_path).unwrap());
    p.download_into(&vp("p.bin"), fd, ReadOptions { offset: 1000 }, sink)
        .await
        .unwrap();
    assert_eq!(std::fs::read(&out_path).unwrap(), payload[1000..]);
    {
        let seen = seen.lock().unwrap();
        assert!(seen.len() >= 2, "progress is reported while streaming");
        assert_eq!(seen.last().copied(), Some((299_000, Some(299_000))));
    }
    let fd = OwnedFd::from(File::create(&out_path).unwrap());
    let err = p
        .download_into(&vp("nothing"), fd, ReadOptions::default(), no_progress())
        .await
        .err();
    assert_eq!(err.map(|e| e.kind), Some(ErrorKind::NotFound));
}

#[tokio::test]
async fn large_entries_go_to_an_unlinked_temp_file() {
    let tmp = tempfile::tempdir().unwrap();
    let cache = tmp.path().join("cache");
    let size = MEMORY_LIMIT as usize + 4096;
    let mut big = vec![0u8; size];
    big[size - 1] = 9;
    big[MEMORY_LIMIT as usize] = 5;
    let data = zip_bytes(&[("big.bin", &big)], zip::CompressionMethod::Deflated);
    let p = open_named("a.zip", &data, &cache).await.unwrap();
    let h = p.open_read(&vp("big.bin"), Lane::Bulk).await.unwrap();
    assert_eq!(h.size(), Some(size as u64));
    assert_eq!(h.read_at(MEMORY_LIMIT, 3).await.unwrap(), [5, 0, 0]);
    assert_eq!(h.read_at(size as u64 - 1, 10).await.unwrap(), [9]);
    assert!(h.read_at(size as u64, 10).await.unwrap().is_empty());
    assert_eq!(
        std::fs::read_dir(&cache).map(|d| d.count()).unwrap_or(0),
        0,
        "no file left behind"
    );
}

#[tokio::test]
async fn checksum_is_sha256_of_the_entry() {
    let tmp = tempfile::tempdir().unwrap();
    let data = zip_bytes(&SAMPLE, zip::CompressionMethod::Deflated);
    let p = open_named("a.zip", &data, tmp.path()).await.unwrap();
    let sum = p.checksum(&vp("top.txt"), "sha256").await.unwrap();
    assert_eq!(sum, Sha256::digest(b"top level").to_vec());
    let md5 = p.checksum(&vp("top.txt"), "md5").await;
    assert_eq!(md5.err().map(|e| e.kind), Some(ErrorKind::Unsupported));
}

#[tokio::test]
async fn big_listings_arrive_in_batches() {
    let tmp = tempfile::tempdir().unwrap();
    let owned: Vec<String> = (0..600).map(|i| format!("f{i:03}")).collect();
    let files: Vec<(&str, &[u8])> = owned.iter().map(|n| (n.as_str(), &b"x"[..])).collect();
    let data = zip_bytes(&files, zip::CompressionMethod::Stored);
    let p = open_named("m.zip", &data, tmp.path()).await.unwrap();
    let (tx, mut rx) = mpsc::channel(8);
    let root = VPath::root();
    let lister = p.list(&root, Lane::Interactive, tx);
    let collect = async {
        let mut sizes = Vec::new();
        while let Some(b) = rx.recv().await {
            sizes.push(b.len());
        }
        sizes
    };
    let (res, sizes) = tokio::join!(lister, collect);
    res.unwrap();
    assert_eq!(sizes, [256, 256, 88]);
}

#[test]
fn detection_by_magic_and_extension() {
    let zip = zip_bytes(&SAMPLE, zip::CompressionMethod::Stored);
    assert_eq!(detect_format(&zip, b"x.bin"), Some(ArchiveFormat::Zip));
    assert_eq!(detect_format(b"PK\x05\x06", b"e"), Some(ArchiveFormat::Zip));
    let sz = sevenz_bytes(&[("a", b"b")]);
    assert_eq!(detect_format(&sz, b"x"), Some(ArchiveFormat::SevenZ));
    let tar = tar_bytes(&SAMPLE);
    assert_eq!(
        detect_format(&tar, b"x"),
        Some(ArchiveFormat::Tar(Compression::None))
    );
    assert_eq!(
        detect_format(&[0u8; 600], b"old.TAR"),
        Some(ArchiveFormat::Tar(Compression::None))
    );
    assert_eq!(detect_format(&[0u8; 600], b"old.bin"), None);
    for comp in [
        Compression::Gzip,
        Compression::Bzip2,
        Compression::Xz,
        Compression::Zstd,
    ] {
        let c = compressed(comp, &tar);
        assert_eq!(detect_format(&c, b"x"), Some(ArchiveFormat::Tar(comp)));
    }
    assert_eq!(detect_format(b"plain text", b"x.zip"), None);
    assert!(!is_archive(b"", b""));
    assert!(is_archive(&zip, b""));
}

#[test]
fn location_ids_are_stable_and_distinct() {
    let a = archive_location_id_str("srv:/a.zip");
    assert!(a.starts_with("arc-") && a.len() == 4 + 16);
    assert_eq!(a, archive_location_id_str("srv:/a.zip"));
    assert_ne!(a, archive_location_id_str("srv:/b.zip"));
    let uri = Uri::new("srv", vp("a.zip"));
    assert_eq!(
        archive_location_id(&uri),
        archive_location_id_str(&uri.to_string())
    );
}

#[tokio::test]
async fn unsupported_zip_method_and_encryption_are_refused() {
    let tmp = tempfile::tempdir().unwrap();
    let central = [0x50, 0x4b, 0x01, 0x02];
    let mut data = zip_bytes(&[("f", b"hello")], zip::CompressionMethod::Stored);
    let cd = data.windows(4).rposition(|w| w == central).unwrap();
    data[cd + 8] |= 1; // general purpose flag bit 0: encrypted
    let p = open_named("e.zip", &data, tmp.path()).await.unwrap();
    let err = p.open_read(&vp("f"), Lane::Bulk).await.err();
    assert_eq!(err.map(|e| e.kind), Some(ErrorKind::Unsupported));
    let mut data = zip_bytes(&[("f", b"hello")], zip::CompressionMethod::Stored);
    let cd = data.windows(4).rposition(|w| w == central).unwrap();
    data[cd + 10] = 12; // compression method: bzip2
    let p = open_named("m.zip", &data, tmp.path()).await.unwrap();
    let err = p.open_read(&vp("f"), Lane::Bulk).await.err();
    assert_eq!(err.map(|e| e.kind), Some(ErrorKind::Unsupported));
}
