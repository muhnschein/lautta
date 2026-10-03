// SPDX-License-Identifier: LGPL-2.1-or-later
//! Compression tests: archives are produced through `compress`, then read
//! back both by independent readers (the `zip`, `tar` and `flate2` crates)
//! and by `ArchiveProvider`.

use super::zipstream::ZipStream;
use super::*;
use crate::provider::archive::ArchiveFormat;
use crate::provider::memory::MemoryProvider;
use crate::provider::{list_all, read_all, StaticResolver};
use std::sync::Mutex;

fn vp(s: &str) -> VPath {
    VPath::parse(s.as_bytes()).unwrap_or_default()
}

fn uri(loc: &str, path: &str) -> Uri {
    Uri::new(loc, vp(path))
}

/// A `Write` sink the test can read afterwards.
#[derive(Clone, Default)]
struct SharedBuf(Arc<Mutex<Vec<u8>>>);

impl Write for SharedBuf {
    fn write(&mut self, buf: &[u8]) -> io::Result<usize> {
        self.0.lock().unwrap().extend_from_slice(buf);
        Ok(buf.len())
    }

    fn flush(&mut self) -> io::Result<()> {
        Ok(())
    }
}

impl SharedBuf {
    fn bytes(&self) -> Vec<u8> {
        self.0.lock().unwrap().clone()
    }
}

/// Fails after `limit` bytes, like a pipe whose reader went away.
struct BrokenPipe {
    limit: usize,
    written: usize,
}

impl Write for BrokenPipe {
    fn write(&mut self, buf: &[u8]) -> io::Result<usize> {
        if self.written + buf.len() > self.limit {
            return Err(io::Error::from(io::ErrorKind::BrokenPipe));
        }
        self.written += buf.len();
        Ok(buf.len())
    }

    fn flush(&mut self) -> io::Result<()> {
        Ok(())
    }
}

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

struct Fixture {
    mem: MemoryProvider,
    resolver: StaticResolver,
    big: Vec<u8>,
}

fn fixture() -> Fixture {
    let mem = MemoryProvider::default();
    let big = noise(700_000, 5);
    mem.add_file("proj/readme.txt", b"hello zip and tar", 1_709_642_096_000);
    mem.add_file("proj/src/main.rs", b"fn main() {}\n", 1_709_642_096_000);
    mem.add_file("proj/src/lib.rs", b"", 1_709_642_096_000);
    mem.add_file("proj/data/big.bin", &big, 1_709_642_096_000);
    mem.add_file("proj/ünï cödé.txt", "unicode".as_bytes(), 0);
    mem.add_dir("proj/empty");
    mem.add_symlink("proj/link", "src/main.rs");
    mem.add_file("single.txt", b"just one", 0);
    let resolver = StaticResolver::default().with("mem", Arc::new(mem.clone()));
    Fixture { mem, resolver, big }
}

async fn run(f: &Fixture, sources: &[Uri], kind: ArchiveKind) -> (Vec<u8>, CompressStats) {
    let sink = SharedBuf::default();
    let stats = compress(&f.resolver, sources, sink.clone(), &CompressOptions::new(kind))
        .await
        .unwrap();
    (sink.bytes(), stats)
}

async fn reopen(name: &str, bytes: &[u8], cache: &Path) -> ArchiveProvider {
    let mem = MemoryProvider::default();
    mem.add_file(name, bytes, 0);
    ArchiveProvider::open(Arc::new(mem), vp(name), cache.to_path_buf())
        .await
        .unwrap()
}

fn tar_members(bytes: &[u8]) -> Vec<(String, tar::EntryType, Vec<u8>, Option<String>)> {
    let gz = flate2::read::GzDecoder::new(bytes);
    let mut archive = tar::Archive::new(gz);
    let mut out = Vec::new();
    for entry in archive.entries().unwrap() {
        let mut e = entry.unwrap();
        let path = e.path().unwrap().to_string_lossy().into_owned();
        let kind = e.header().entry_type();
        let link = e.link_name().unwrap().map(|l| l.to_string_lossy().into_owned());
        let mut data = Vec::new();
        e.read_to_end(&mut data).unwrap();
        out.push((path, kind, data, link));
    }
    out
}

const EXPECTED_ORDER: [&str; 10] = [
    "proj",
    "proj/data",
    "proj/data/big.bin",
    "proj/empty",
    "proj/link",
    "proj/readme.txt",
    "proj/src",
    "proj/src/lib.rs",
    "proj/src/main.rs",
    "proj/ünï cödé.txt",
];

#[tokio::test]
async fn zip_is_readable_by_the_zip_crate() {
    let f = fixture();
    let (bytes, stats) = run(&f, &[uri("mem", "proj")], ArchiveKind::Zip).await;
    assert_eq!(
        (stats.files, stats.dirs, stats.symlinks, stats.skipped),
        (5, 4, 1, 0),
    );
    assert_eq!(stats.bytes, 17 + 13 + 700_000 + 7);
    let mut z = zip::ZipArchive::new(io::Cursor::new(bytes)).unwrap();
    let names: Vec<String> = (0..z.len())
        .map(|i| z.by_index(i).unwrap().name().to_owned())
        .collect();
    let mut want: Vec<String> = EXPECTED_ORDER.iter().map(|n| (*n).to_owned()).collect();
    for n in ["proj", "proj/data", "proj/empty", "proj/src"] {
        let at = want.iter().position(|w| w == n).unwrap();
        want[at].push('/');
    }
    assert_eq!(names, want, "deterministic, parents first, folders end with /");
    let mut read = |name: &str| {
        let mut out = Vec::new();
        z.by_name(name).unwrap().read_to_end(&mut out).unwrap();
        out
    };
    assert_eq!(read("proj/readme.txt"), b"hello zip and tar");
    assert_eq!(
        read("proj/data/big.bin"),
        f.big,
        "multi-chunk file survives, CRC verified by the reader"
    );
    assert_eq!(read("proj/src/lib.rs"), b"");
    assert_eq!(read("proj/ünï cödé.txt"), b"unicode");
    assert_eq!(read("proj/link"), b"src/main.rs", "a link's data is its target");
    assert_eq!(
        z.by_name("proj/readme.txt")
            .unwrap()
            .unix_mode()
            .map(|m| m & 0o170_000),
        Some(0o100_000)
    );
    assert_eq!(
        z.by_name("proj/link").unwrap().unix_mode().map(|m| m & 0o170_000),
        Some(0o120_000)
    );
    assert!(z.by_name("proj/empty/").unwrap().is_dir());
}

#[tokio::test]
async fn zip_round_trips_through_the_archive_provider() {
    let f = fixture();
    let tmp = tempfile::tempdir().unwrap();
    let (bytes, _) = run(
        &f,
        &[uri("mem", "proj"), uri("mem", "single.txt")],
        ArchiveKind::Zip,
    )
    .await;
    let p = reopen("out.zip", &bytes, tmp.path()).await;
    assert_eq!(p.format(), ArchiveFormat::Zip);
    let root = list_all(&p, &VPath::root(), Lane::Interactive).await.unwrap();
    let mut names: Vec<String> = root.iter().map(Entry::display_name).collect();
    names.sort();
    assert_eq!(names, ["proj", "single.txt"]);
    let h = p.open_read(&vp("proj/data/big.bin"), Lane::Bulk).await.unwrap();
    assert_eq!(read_all(h.as_ref(), 1 << 30).await.unwrap(), f.big);
    let st = p.stat(&vp("proj/readme.txt"), false, Lane::Bulk).await.unwrap();
    assert_eq!(
        st.modified.map(crate::entry::system_time_to_ms),
        Some(1_709_642_096_000)
    );
    assert_eq!(st.mode, Some(0o644));
    let link = p.stat(&vp("proj/link"), false, Lane::Bulk).await.unwrap();
    assert_eq!(link.kind, Kind::Symlink);
    assert_eq!(p.read_link(&vp("proj/link")).await.unwrap(), b"src/main.rs");
    assert_eq!(
        p.stat(&vp("proj/empty"), false, Lane::Bulk).await.unwrap().kind,
        Kind::Dir
    );
}

#[tokio::test]
async fn tar_gz_is_readable_by_the_tar_crate_and_stores_links() {
    let f = fixture();
    let (bytes, stats) = run(&f, &[uri("mem", "proj")], ArchiveKind::TarGz).await;
    assert_eq!((stats.files, stats.dirs, stats.symlinks), (5, 4, 1));
    let members = tar_members(&bytes);
    let paths: Vec<&str> = members.iter().map(|m| m.0.trim_end_matches('/')).collect();
    assert_eq!(paths, EXPECTED_ORDER);
    let find = |p: &str| members.iter().find(|m| m.0.trim_end_matches('/') == p).unwrap();
    assert_eq!(find("proj/data/big.bin").2, f.big);
    assert_eq!(find("proj/readme.txt").2, b"hello zip and tar");
    assert_eq!(find("proj/ünï cödé.txt").2, b"unicode");
    assert_eq!(find("proj/empty").1, tar::EntryType::Directory);
    let link = find("proj/link");
    assert_eq!(link.1, tar::EntryType::Symlink);
    assert_eq!(link.3.as_deref(), Some("src/main.rs"));
    assert!(link.2.is_empty());
}

#[tokio::test]
async fn tar_gz_round_trips_through_the_archive_provider() {
    let f = fixture();
    let tmp = tempfile::tempdir().unwrap();
    let (bytes, _) = run(&f, &[uri("mem", "proj")], ArchiveKind::TarGz).await;
    let p = reopen("out.tar.gz", &bytes, tmp.path()).await;
    assert_eq!(
        p.format(),
        ArchiveFormat::Tar(crate::provider::archive::Compression::Gzip)
    );
    let h = p.open_read(&vp("proj/src/main.rs"), Lane::Bulk).await.unwrap();
    assert_eq!(read_all(h.as_ref(), 1 << 20).await.unwrap(), b"fn main() {}\n");
    assert_eq!(p.read_link(&vp("proj/link")).await.unwrap(), b"src/main.rs");
    let st = p.stat(&vp("proj/readme.txt"), false, Lane::Bulk).await.unwrap();
    assert_eq!(
        st.modified.map(crate::entry::system_time_to_ms),
        Some(1_709_642_096_000)
    );
}

#[tokio::test]
async fn long_names_survive_in_tar_and_zip() {
    let f = fixture();
    let long = format!("{}/{}.txt", "d".repeat(80), "f".repeat(120));
    f.mem.add_file(&format!("deep/{long}"), b"deep data", 0);
    let tmp = tempfile::tempdir().unwrap();
    for (kind, name) in [(ArchiveKind::TarGz, "l.tar.gz"), (ArchiveKind::Zip, "l.zip")] {
        let (bytes, _) = run(&f, &[uri("mem", "deep")], kind).await;
        let p = reopen(name, &bytes, tmp.path()).await;
        let h = p
            .open_read(&vp(&format!("deep/{long}")), Lane::Bulk)
            .await
            .unwrap();
        assert_eq!(
            read_all(h.as_ref(), 1 << 20).await.unwrap(),
            b"deep data",
            "{name}"
        );
    }
}

#[tokio::test]
async fn single_files_and_multiple_locations() {
    let f = fixture();
    let other = MemoryProvider::default();
    other.add_file("docs/note.md", b"# n", 0);
    let resolver = f.resolver.clone().with("other", Arc::new(other));
    let sink = SharedBuf::default();
    let sources = [uri("mem", "single.txt"), uri("other", "docs")];
    let stats = compress(
        &resolver,
        &sources,
        sink.clone(),
        &CompressOptions::new(ArchiveKind::Zip),
    )
    .await
    .unwrap();
    assert_eq!((stats.files, stats.dirs), (2, 1));
    let mut z = zip::ZipArchive::new(io::Cursor::new(sink.bytes())).unwrap();
    // Archive order (by index); `file_names()` iterates a hash map.
    let names: Vec<String> = (0..z.len())
        .map(|i| z.by_index(i).unwrap().name().to_owned())
        .collect();
    assert_eq!(names, ["single.txt", "docs/", "docs/note.md"]);
    let mut out = String::new();
    z.by_name("docs/note.md")
        .unwrap()
        .read_to_string(&mut out)
        .unwrap();
    assert_eq!(out, "# n");
}

#[tokio::test]
async fn progress_counts_bytes_and_uses_the_hint() {
    let f = fixture();
    let seen = Arc::new(Mutex::new(Vec::new()));
    let mut opts = CompressOptions::new(ArchiveKind::Zip);
    opts.total_hint = Some(700_037);
    opts.progress = {
        let seen = Arc::clone(&seen);
        Arc::new(move |done, total| seen.lock().unwrap().push((done, total)))
    };
    compress(
        &f.resolver,
        &[uri("mem", "proj/data"), uri("mem", "proj/readme.txt")],
        SharedBuf::default(),
        &opts,
    )
    .await
    .unwrap();
    let seen = seen.lock().unwrap();
    assert!(seen.len() >= 4, "several chunks: {seen:?}");
    assert!(seen.windows(2).all(|w| w[0].0 < w[1].0), "strictly increasing");
    assert_eq!(seen.last().copied(), Some((700_017, Some(700_037))));
}

#[tokio::test]
async fn cancel_stops_the_run_with_canceled() {
    let f = fixture();
    let mut opts = CompressOptions::new(ArchiveKind::TarGz);
    let cancel = Arc::clone(&opts.cancel);
    opts.progress = Arc::new(move |_, _| cancel.store(true, Ordering::Relaxed));
    let err = compress(&f.resolver, &[uri("mem", "proj")], SharedBuf::default(), &opts)
        .await
        .err();
    assert_eq!(err.map(|e| e.kind), Some(ErrorKind::Canceled));
    let pre = CompressOptions::new(ArchiveKind::Zip);
    pre.cancel.store(true, Ordering::Relaxed);
    let err = compress(
        &f.resolver,
        &[uri("mem", "single.txt")],
        SharedBuf::default(),
        &pre,
    )
    .await
    .err();
    assert_eq!(err.map(|e| e.kind), Some(ErrorKind::Canceled));
}

#[tokio::test]
async fn bad_selections_are_rejected() {
    let f = fixture();
    let opts = CompressOptions::new(ArchiveKind::Zip);
    let run = |sources: Vec<Uri>| {
        let resolver = f.resolver.clone();
        let opts = opts.clone();
        async move {
            compress(&resolver, &sources, SharedBuf::default(), &opts)
                .await
                .err()
                .map(|e| e.kind)
        }
    };
    assert_eq!(run(vec![]).await, Some(ErrorKind::InvalidArgument));
    assert_eq!(
        run(vec![Uri::root("mem")]).await,
        Some(ErrorKind::InvalidArgument)
    );
    assert_eq!(
        run(vec![uri("mem", "single.txt"), uri("mem", "proj/readme.txt")]).await,
        None
    );
    f.mem.add_file("proj/single.txt", b"x", 0);
    assert_eq!(
        run(vec![uri("mem", "single.txt"), uri("mem", "proj/single.txt")]).await,
        Some(ErrorKind::AlreadyExists)
    );
    assert_eq!(run(vec![uri("mem", "nope")]).await, Some(ErrorKind::NotFound));
    assert_eq!(run(vec![uri("elsewhere", "x")]).await, Some(ErrorKind::NotFound));
}

#[tokio::test]
async fn source_errors_propagate_with_their_kind() {
    let f = fixture();
    f.mem.fail_next(
        "open_read",
        "proj/readme.txt",
        Error::new(ErrorKind::PermissionDenied, "no"),
    );
    let err = compress(
        &f.resolver,
        &[uri("mem", "proj")],
        SharedBuf::default(),
        &CompressOptions::new(ArchiveKind::Zip),
    )
    .await
    .err();
    assert_eq!(err.map(|e| e.kind), Some(ErrorKind::PermissionDenied));
}

#[tokio::test]
async fn a_failing_sink_ends_the_run_with_its_error() {
    let f = fixture();
    for kind in [ArchiveKind::Zip, ArchiveKind::TarGz] {
        let sink = BrokenPipe {
            limit: 100_000,
            written: 0,
        };
        let err = compress(
            &f.resolver,
            &[uri("mem", "proj")],
            sink,
            &CompressOptions::new(kind),
        )
        .await
        .err()
        .unwrap();
        assert_eq!(err.kind, ErrorKind::Io, "{kind:?}: {err}");
        assert_ne!(err.message, WRITER_GONE, "the sink's own error is reported");
    }
}

#[test]
fn zip_writer_handles_unknown_sizes_and_forced_zip64() {
    for force in [false, true] {
        let sink = SharedBuf::default();
        let mut zip = ZipStream::new(sink.clone(), 6);
        if force {
            zip = zip.with_forced_zip64();
        }
        let mut w: Box<dyn MemberWriter> = Box::new(zip);
        let meta = |name: &str, kind: Kind, size: Option<u64>| EntryMeta {
            path: name.as_bytes().to_vec(),
            kind,
            size,
            mtime: Some(SystemTime::UNIX_EPOCH + std::time::Duration::from_secs(1_709_642_096)),
            mode: Some(0o640),
            link: None,
        };
        w.member(
            &meta("unknown.bin", Kind::File, None),
            &mut io::Cursor::new(vec![7u8; 5000]),
        )
        .unwrap();
        w.member(
            &meta("known.txt", Kind::File, Some(3)),
            &mut io::Cursor::new(b"abc".to_vec()),
        )
        .unwrap();
        w.member(&meta("d", Kind::Dir, None), &mut io::empty()).unwrap();
        w.finish().unwrap();
        let mut z = zip::ZipArchive::new(io::Cursor::new(sink.bytes())).unwrap();
        let mut data = Vec::new();
        z.by_name("unknown.bin").unwrap().read_to_end(&mut data).unwrap();
        assert_eq!(data, vec![7u8; 5000], "force={force}");
        let mut data = Vec::new();
        z.by_name("known.txt").unwrap().read_to_end(&mut data).unwrap();
        assert_eq!(data, b"abc");
        assert_eq!(
            z.by_name("known.txt").unwrap().unix_mode().map(|m| m & 0o777),
            Some(0o640)
        );
        assert!(z.by_name("d/").unwrap().is_dir());
    }
}

#[tokio::test]
async fn forced_zip64_archives_open_with_our_reader_too() {
    let sink = SharedBuf::default();
    let mut w: Box<dyn MemberWriter> = Box::new(ZipStream::new(sink.clone(), 1).with_forced_zip64());
    let meta = EntryMeta {
        path: b"a.txt".to_vec(),
        kind: Kind::File,
        size: Some(4),
        mtime: None,
        mode: None,
        link: None,
    };
    w.member(&meta, &mut io::Cursor::new(b"data".to_vec())).unwrap();
    w.finish().unwrap();
    let tmp = tempfile::tempdir().unwrap();
    let p = reopen("z64.zip", &sink.bytes(), tmp.path()).await;
    let h = p.open_read(&vp("a.txt"), Lane::Bulk).await.unwrap();
    assert_eq!(read_all(h.as_ref(), 100).await.unwrap(), b"data");
}

#[tokio::test]
async fn many_small_files_stream_through_a_small_queue() {
    let mem = MemoryProvider::default();
    for i in 0..300 {
        mem.add_file(&format!("many/f{i:03}.txt"), format!("file {i}").as_bytes(), 0);
    }
    let resolver = StaticResolver::default().with("mem", Arc::new(mem));
    let sink = SharedBuf::default();
    let stats = compress(
        &resolver,
        &[uri("mem", "many")],
        sink.clone(),
        &CompressOptions::new(ArchiveKind::TarGz),
    )
    .await
    .unwrap();
    assert_eq!((stats.files, stats.dirs), (300, 1));
    let members = tar_members(&sink.bytes());
    assert_eq!(members.len(), 301);
    assert_eq!(members[300].2, b"file 299");
}

#[tokio::test]
async fn extract_plan_lists_parents_first_with_relative_paths() {
    let f = fixture();
    let tmp = tempfile::tempdir().unwrap();
    let (bytes, _) = run(&f, &[uri("mem", "proj")], ArchiveKind::Zip).await;
    let p = reopen("out.zip", &bytes, tmp.path()).await;
    let whole = extract_plan(&p, &VPath::root()).unwrap();
    assert_eq!(whole.len(), 10);
    assert_eq!(
        whole[0],
        ExtractItem {
            path: vp("proj"),
            kind: Kind::Dir,
            size: 0
        }
    );
    let below = extract_plan(&p, &vp("proj/src")).unwrap();
    let rel: Vec<(String, Kind, u64)> = below.iter().map(|i| (i.path.display(), i.kind, i.size)).collect();
    assert_eq!(
        rel,
        [
            ("lib.rs".to_owned(), Kind::File, 0),
            ("main.rs".to_owned(), Kind::File, 13)
        ]
    );
    let one = extract_plan(&p, &vp("proj/readme.txt")).unwrap();
    assert_eq!(
        one,
        [ExtractItem {
            path: vp("readme.txt"),
            kind: Kind::File,
            size: 17
        }]
    );
    assert_eq!(
        extract_plan(&p, &vp("missing")).err().map(|e| e.kind),
        Some(ErrorKind::NotFound)
    );
    assert!(extract_plan(&p, &vp("proj/empty")).unwrap().is_empty());
}
