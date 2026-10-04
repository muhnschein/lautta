// SPDX-License-Identifier: LGPL-2.1-or-later
//! Read-only archive provider (SPEC PRV-10, LOC-5): zip, tar, tar.gz/bz2/xz/zst
//! and 7z presented as a location.
//!
//! * zip is read through a `Read + Seek` adapter with a 64 KiB block cache
//!   ([`blocks`]) so a remote zip needs ranged reads only: the central
//!   directory first, then the blocks of the entries that are opened.
//! * plain tar is indexed the same way (headers only).
//! * compressed tar and 7z cannot be indexed without reading everything, so
//!   they are downloaded once to a cache file and indexed from there.
//!
//! Entry names that could leave the archive (`..`, absolute, drive letters,
//! NUL) are never indexed; their number is reported by
//! [`ArchiveProvider::skipped_entries`].

mod blocks;
mod sevenz;
mod tarfmt;
#[cfg(test)]
mod tests;
mod zipdir;

use crate::entry::{cap, Capabilities, Entry, EntryFlags, Kind};
use crate::error::{Error, ErrorKind, Result};
use crate::provider::{
    AttributeChanges, CopyOptions, Lane, ProgressSink, Provider, ReadHandle, ReadOptions, RenameMode,
    SpaceInfo, WriteOptions,
};
use crate::uri::Uri;
use crate::vpath::VPath;
use async_trait::async_trait;
use blocks::{from_io, BlockSource, BLOCK};
use sha2::{Digest, Sha256};
use std::collections::{BTreeMap, HashMap};
use std::fs::File;
use std::io::{self, Read, Seek, SeekFrom, Write};
use std::os::fd::OwnedFd;
use std::os::unix::fs::FileExt;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;
use std::time::SystemTime;
use tokio::sync::mpsc;

/// Entries up to this size are decompressed into memory by `open_read`; larger
/// ones go to an unlinked temporary file in the cache directory.
pub const MEMORY_LIMIT: u64 = 16 * 1024 * 1024;
/// Entries per listing batch.
const LIST_BATCH: usize = 256;
/// Symlink hops followed by `stat`/`open_read`.
const MAX_HOPS: usize = 8;

/// Outer compression of a tar archive.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Compression {
    None,
    Gzip,
    Bzip2,
    Xz,
    Zstd,
}

/// What kind of archive a file is.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ArchiveFormat {
    Zip,
    Tar(Compression),
    SevenZ,
}

/// Where an entry's bytes live; filled by the format readers.
#[derive(Debug, Clone)]
enum Loc {
    Implicit,
    Zip(zipdir::ZipLoc),
    TarAt(u64),
    TarSeq(usize),
    SevenZ(String),
}

/// One entry as a format reader reports it, before sanitising.
struct RawEntry {
    name: Vec<u8>,
    kind: Kind,
    size: u64,
    mtime: Option<SystemTime>,
    mode: Option<u32>,
    link: Option<Vec<u8>>,
    loc: Loc,
}

#[derive(Debug, Clone)]
struct Node {
    kind: Kind,
    size: u64,
    mtime: Option<SystemTime>,
    mode: Option<u32>,
    link: Option<Vec<u8>>,
    loc: Loc,
}

impl Node {
    fn implicit_dir() -> Node {
        Node {
            kind: Kind::Dir,
            size: 0,
            mtime: None,
            mode: Some(0o755),
            link: None,
            loc: Loc::Implicit,
        }
    }
}

/// A flattened archive entry for planners (`compress::extract_plan`).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ArchiveEntry {
    pub path: VPath,
    pub kind: Kind,
    pub size: u64,
}

/// `arc-<16 hex>`: the location id of the archive opened from `source_uri`
/// (the display form of its [`Uri`]).
pub fn archive_location_id_str(source_uri: &str) -> String {
    let digest = Sha256::digest(source_uri.as_bytes());
    format!("arc-{}", hex::encode(&digest[..8]))
}

/// [`archive_location_id_str`] for a typed URI.
pub fn archive_location_id(source_uri: &Uri) -> String {
    archive_location_id_str(&source_uri.to_string())
}

fn has_tar_extension(name: &[u8]) -> bool {
    let lower = String::from_utf8_lossy(name).to_lowercase();
    [
        ".tar",
        ".tar.gz",
        ".tgz",
        ".tar.bz2",
        ".tbz2",
        ".tbz",
        ".tar.xz",
        ".txz",
        ".tar.zst",
        ".tzst",
        ".tar.zstd",
    ]
    .iter()
    .any(|ext| lower.ends_with(ext))
}

fn compression_magic(head: &[u8]) -> Option<Compression> {
    if head.starts_with(&[0x1f, 0x8b]) {
        Some(Compression::Gzip)
    } else if head.starts_with(b"BZh") {
        Some(Compression::Bzip2)
    } else if head.starts_with(&[0xfd, b'7', b'z', b'X', b'Z', 0]) {
        Some(Compression::Xz)
    } else if head.starts_with(&[0x28, 0xb5, 0x2f, 0xfd]) {
        Some(Compression::Zstd)
    } else {
        None
    }
}

fn has_ustar(block: &[u8]) -> bool {
    block.get(257..262) == Some(b"ustar")
}

/// Recognises an archive from its first bytes (at least 512 are useful) and
/// its file name: magic numbers first, extension only where the magic cannot
/// tell (old-style tar, compressed tar).
pub fn detect_format(head: &[u8], name: &[u8]) -> Option<ArchiveFormat> {
    if let Some(comp) = compression_magic(head) {
        let is_tar = has_tar_extension(name) || has_ustar(&tarfmt::decompressed_head(comp, head, 512));
        return is_tar.then_some(ArchiveFormat::Tar(comp));
    }
    if head.starts_with(b"PK\x03\x04") || head.starts_with(b"PK\x05\x06") {
        return Some(ArchiveFormat::Zip);
    }
    if head.starts_with(&[b'7', b'z', 0xbc, 0xaf, 0x27, 0x1c]) {
        return Some(ArchiveFormat::SevenZ);
    }
    if has_ustar(head) || (head.len() >= 512 && has_tar_extension(name)) {
        return Some(ArchiveFormat::Tar(Compression::None));
    }
    None
}

/// True when `head`/`name` look like a supported archive (the "open as
/// archive" menu item).
pub fn is_archive(head: &[u8], name: &[u8]) -> bool {
    detect_format(head, name).is_some()
}

/// An entry name that stays inside the archive, or `None`. Rejected: absolute
/// paths, `..` components (also behind a backslash, for names written on
/// Windows), NUL and drive prefixes.
fn sanitize(raw: &[u8]) -> Option<VPath> {
    if raw.first() == Some(&b'/') || raw.contains(&0) {
        return None;
    }
    let mut comps = raw.split(|b| *b == b'/' || *b == b'\\');
    if comps.clone().any(|c| c == b"..") {
        return None;
    }
    let drive = comps.next().map_or(false, |c| {
        c.len() == 2 && c[0].is_ascii_alphabetic() && c[1] == b':'
    });
    if drive {
        return None;
    }
    VPath::parse(raw).ok()
}

/// Resolves a symlink target relative to the link's folder, staying inside
/// the archive (absolute targets are taken from the archive root).
fn resolve_link(parent: &VPath, target: &[u8]) -> Option<VPath> {
    let mut parts: Vec<&[u8]> = if target.first() == Some(&b'/') {
        Vec::new()
    } else {
        parent.components().collect()
    };
    for c in target.split(|b| *b == b'/') {
        match c {
            b"" | b"." => {}
            b".." => {
                parts.pop()?;
            }
            other => parts.push(other),
        }
    }
    VPath::parse(&parts.join(&b'/')).ok()
}

/// The sanitised, tree-shaped view of an archive.
struct Index {
    nodes: BTreeMap<VPath, Node>,
    children: HashMap<VPath, Vec<Vec<u8>>>,
    skipped: usize,
}

impl Index {
    fn build(entries: Vec<RawEntry>) -> Index {
        let mut nodes = BTreeMap::new();
        nodes.insert(VPath::root(), Node::implicit_dir());
        let mut index = Index {
            nodes,
            children: HashMap::new(),
            skipped: 0,
        };
        for raw in entries {
            index.add_raw(raw);
        }
        for names in index.children.values_mut() {
            names.sort();
        }
        index
    }

    fn add_raw(&mut self, raw: RawEntry) {
        let Some(path) = sanitize(&raw.name) else {
            self.skipped += 1;
            return;
        };
        if path.is_root() {
            return;
        }
        let node = Node {
            kind: raw.kind,
            size: raw.size,
            mtime: raw.mtime,
            mode: raw.mode,
            link: raw.link,
            loc: raw.loc,
        };
        if !self.insert(path, node) {
            self.skipped += 1;
        }
    }

    /// Inserts a node, creating implicit parents. False when an ancestor is
    /// not a folder or a folder would be replaced by a file.
    fn insert(&mut self, path: VPath, node: Node) -> bool {
        for ancestor in path.ancestors().into_iter().skip(1) {
            match self.nodes.get(&ancestor) {
                Some(n) if n.kind != Kind::Dir => return false,
                Some(_) => {}
                None => self.link_child(&ancestor, Node::implicit_dir()),
            }
        }
        if let Some(existing) = self.nodes.get(&path) {
            if existing.kind == Kind::Dir && node.kind != Kind::Dir {
                return false;
            }
            self.nodes.insert(path, node);
            return true;
        }
        self.link_child(&path, node);
        true
    }

    fn link_child(&mut self, path: &VPath, node: Node) {
        let parent = path.parent().unwrap_or_default();
        let name = path.name().unwrap_or_default().to_vec();
        self.children.entry(parent).or_default().push(name);
        self.nodes.insert(path.clone(), node);
    }
}

/// Removes a cache file when the archive is closed.
struct CacheFile(PathBuf);

impl Drop for CacheFile {
    fn drop(&mut self) {
        let _ = std::fs::remove_file(&self.0);
    }
}

struct Inner {
    format: ArchiveFormat,
    index: Index,
    blocks: Option<Arc<BlockSource>>,
    cache: Option<CacheFile>,
    cache_dir: PathBuf,
}

fn join_err(err: tokio::task::JoinError) -> Error {
    Error::new(ErrorKind::Internal, format!("archive worker failed: {err}"))
}

fn read_only<T>() -> Result<T> {
    Err(Error::new(
        ErrorKind::ReadOnlyFilesystem,
        "archives are read-only",
    ))
}

static UNIQUE: AtomicU64 = AtomicU64::new(0);

fn unique_name(prefix: &str) -> String {
    format!(
        "{prefix}-{}-{}",
        std::process::id(),
        UNIQUE.fetch_add(1, Ordering::Relaxed)
    )
}

impl Inner {
    fn cache_file(&self) -> Result<File> {
        match &self.cache {
            Some(c) => File::open(&c.0).map_err(Error::from),
            None => Err(Error::new(ErrorKind::Internal, "archive has no cache file")),
        }
    }

    fn block_source(&self) -> Result<&Arc<BlockSource>> {
        self.blocks
            .as_ref()
            .ok_or_else(|| Error::new(ErrorKind::Internal, "archive has no block source"))
    }

    /// Streams the bytes of `node` into `out` (blocking).
    fn copy_node(&self, node: &Node, out: &mut dyn Write) -> Result<()> {
        match &node.loc {
            Loc::Implicit => Err(Error::kind(ErrorKind::IsADirectory)),
            Loc::Zip(z) => {
                let mut r = zipdir::open_entry(self.block_source()?, z)?;
                io::copy(&mut r, out).map_err(from_io)?;
                Ok(())
            }
            Loc::TarAt(offset) => {
                let mut r = self.block_source()?.reader();
                r.seek(SeekFrom::Start(*offset)).map_err(from_io)?;
                let copied = io::copy(&mut r.take(node.size), out).map_err(from_io)?;
                if copied == node.size {
                    Ok(())
                } else {
                    Err(Error::new(ErrorKind::ProtocolError, "tar entry is truncated"))
                }
            }
            Loc::TarSeq(seq) => {
                let comp = match self.format {
                    ArchiveFormat::Tar(c) => c,
                    _ => Compression::None,
                };
                tarfmt::copy_seq(self.cache_file()?, comp, *seq, out)
            }
            Loc::SevenZ(name) => sevenz::copy_entry(self.cache_file()?, name, out),
        }
    }

    fn node(&self, path: &VPath) -> Result<&Node> {
        self.index
            .nodes
            .get(path)
            .ok_or_else(|| Error::kind(ErrorKind::NotFound))
    }

    /// Follows symlinks that stay inside the archive.
    fn resolve(&self, path: &VPath, follow: bool) -> Result<(VPath, &Node)> {
        let mut cur = path.clone();
        for _ in 0..=MAX_HOPS {
            let node = self.node(&cur)?;
            if !follow || node.kind != Kind::Symlink {
                return Ok((cur, node));
            }
            let target = self.link_bytes(&cur, node)?;
            cur = resolve_link(&cur.parent().unwrap_or_default(), &target)
                .ok_or_else(|| Error::kind(ErrorKind::NotFound))?;
        }
        Err(Error::new(
            ErrorKind::InvalidArgument,
            "too many levels of symbolic links",
        ))
    }

    /// The link target: from the header (tar) or from the entry data.
    fn link_bytes(&self, _path: &VPath, node: &Node) -> Result<Vec<u8>> {
        if let Some(l) = &node.link {
            return Ok(l.clone());
        }
        if node.size > 4096 {
            return Err(Error::new(ErrorKind::ProtocolError, "symlink target too long"));
        }
        let mut buf = Vec::new();
        self.copy_node(node, &mut buf)?;
        Ok(buf)
    }

    fn make_entry(&self, path: &VPath, node: &Node) -> Entry {
        let mut e = Entry::new(path.name().unwrap_or_default(), node.kind);
        e.size = (node.kind != Kind::Dir).then_some(node.size);
        e.modified = node.mtime;
        e.mode = node.mode;
        if node.kind == Kind::Symlink {
            let target = node
                .link
                .as_deref()
                .and_then(|l| resolve_link(&path.parent().unwrap_or_default(), l))
                .and_then(|t| self.index.nodes.get(&t));
            match target {
                Some(t) => e.target_kind = t.kind,
                None => {
                    e.target_kind = Kind::Unknown;
                    e.flags |= EntryFlags::TARGET_UNKNOWN;
                }
            }
        }
        e
    }
}

/// Read-only provider over one archive file.
pub struct ArchiveProvider {
    inner: Arc<Inner>,
}

impl ArchiveProvider {
    /// Opens `path` of `source` as an archive. `cache_dir` receives the
    /// downloaded copy of non-zip archives and large extracted entries.
    pub async fn open(source: Arc<dyn Provider>, path: VPath, cache_dir: PathBuf) -> Result<ArchiveProvider> {
        let handle: Arc<dyn ReadHandle> = Arc::from(source.open_read(&path, Lane::Interactive).await?);
        let head = read_block(handle.as_ref(), 0, BLOCK as usize).await?;
        let format = detect_format(&head, path.name().unwrap_or_default())
            .ok_or_else(|| Error::new(ErrorKind::Unsupported, "not a supported archive"))?;
        let inner = match format {
            ArchiveFormat::Zip | ArchiveFormat::Tar(Compression::None) => {
                let size = match handle.size() {
                    Some(s) => s,
                    None => source
                        .stat(&path, true, Lane::Interactive)
                        .await?
                        .size
                        .unwrap_or(0),
                };
                let src = BlockSource::new(handle, tokio::runtime::Handle::current(), size, Some(head));
                index_ranged(format, src, cache_dir).await?
            }
            _ => {
                let cache = download_to_cache(source.as_ref(), &path, &cache_dir).await?;
                index_cached(format, cache, cache_dir).await?
            }
        };
        Ok(ArchiveProvider {
            inner: Arc::new(inner),
        })
    }

    pub fn format(&self) -> ArchiveFormat {
        self.inner.format
    }

    /// Entries refused because their names could escape the archive (or
    /// collide with a folder); never listed, never extractable.
    pub fn skipped_entries(&self) -> usize {
        self.inner.index.skipped
    }

    /// Ranged reads issued to the source so far (zip and plain tar; 0 for
    /// archives that were downloaded).
    pub fn source_reads(&self) -> u64 {
        self.inner.blocks.as_ref().map_or(0, |b| b.read_count())
    }

    /// Every entry below the root, parents before children.
    pub fn entries(&self) -> Vec<ArchiveEntry> {
        self.inner
            .index
            .nodes
            .iter()
            .filter(|(p, _)| !p.is_root())
            .map(|(p, n)| ArchiveEntry {
                path: p.clone(),
                kind: n.kind,
                size: n.size,
            })
            .collect()
    }

    async fn run<T: Send + 'static>(
        &self,
        f: impl FnOnce(&Inner) -> Result<T> + Send + 'static,
    ) -> Result<T> {
        let inner = Arc::clone(&self.inner);
        tokio::task::spawn_blocking(move || f(&inner))
            .await
            .map_err(join_err)?
    }
}

async fn read_block(handle: &dyn ReadHandle, offset: u64, want: usize) -> Result<Vec<u8>> {
    let mut data = Vec::new();
    while data.len() < want {
        let chunk = handle
            .read_at(offset + data.len() as u64, want - data.len())
            .await?;
        if chunk.is_empty() {
            break;
        }
        data.extend_from_slice(&chunk);
    }
    Ok(data)
}

async fn index_ranged(format: ArchiveFormat, src: Arc<BlockSource>, cache_dir: PathBuf) -> Result<Inner> {
    let reader_src = Arc::clone(&src);
    let raw = tokio::task::spawn_blocking(move || match format {
        ArchiveFormat::Zip => zipdir::read_entries(&mut reader_src.reader()),
        _ => tarfmt::index_seek(reader_src.reader()),
    })
    .await
    .map_err(join_err)??;
    Ok(Inner {
        format,
        index: Index::build(raw),
        blocks: Some(src),
        cache: None,
        cache_dir,
    })
}

async fn index_cached(format: ArchiveFormat, cache: CacheFile, cache_dir: PathBuf) -> Result<Inner> {
    let path = cache.0.clone();
    let raw = tokio::task::spawn_blocking(move || {
        let file = File::open(&path)?;
        match format {
            ArchiveFormat::Tar(comp) => {
                tarfmt::index_stream(tarfmt::decoder(comp, io::BufReader::new(file))?)
            }
            _ => sevenz::index(file),
        }
    })
    .await
    .map_err(join_err)??;
    Ok(Inner {
        format,
        index: Index::build(raw),
        blocks: None,
        cache: Some(cache),
        cache_dir,
    })
}

/// Sequential download of the whole archive into `dir` (PRV-10).
async fn download_to_cache(source: &dyn Provider, path: &VPath, dir: &Path) -> Result<CacheFile> {
    tokio::fs::create_dir_all(dir).await?;
    let handle = source.open_read(path, Lane::Bulk).await?;
    let target = dir.join(unique_name("archive"));
    let guard = CacheFile(target.clone());
    let file = Arc::new(tokio::fs::File::create(&target).await?.into_std().await);
    let mut offset = 0u64;
    loop {
        let chunk = handle.read_at(offset, 1 << 20).await?;
        if chunk.is_empty() {
            return Ok(guard);
        }
        offset += chunk.len() as u64;
        let f = Arc::clone(&file);
        let at = offset - chunk.len() as u64;
        tokio::task::spawn_blocking(move || f.write_all_at(&chunk, at))
            .await
            .map_err(join_err)??;
    }
}

/// Forwards bytes to `inner`, dropping the first `skip` of them and reporting
/// progress every 64 KiB.
struct SinkWriter<W: Write> {
    inner: W,
    skip: u64,
    written: u64,
    reported: u64,
    total: u64,
    progress: ProgressSink,
}

impl<W: Write> SinkWriter<W> {
    fn new(inner: W, skip: u64, total: u64, progress: ProgressSink) -> SinkWriter<W> {
        SinkWriter {
            inner,
            skip,
            written: 0,
            reported: 0,
            total,
            progress,
        }
    }

    fn finish(mut self) -> io::Result<()> {
        self.inner.flush()?;
        (self.progress)(self.written, Some(self.total));
        Ok(())
    }
}

impl<W: Write> Write for SinkWriter<W> {
    fn write(&mut self, buf: &[u8]) -> io::Result<usize> {
        let drop_now = usize::try_from(self.skip).unwrap_or(usize::MAX).min(buf.len());
        self.skip -= drop_now as u64;
        let keep = &buf[drop_now..];
        if !keep.is_empty() {
            self.inner.write_all(keep)?;
            self.written += keep.len() as u64;
            if self.written - self.reported >= 64 * 1024 {
                self.reported = self.written;
                (self.progress)(self.written, Some(self.total));
            }
        }
        Ok(buf.len())
    }

    fn flush(&mut self) -> io::Result<()> {
        self.inner.flush()
    }
}

/// Collects up to `cap` bytes; more is an error (a lying size field).
struct CapWriter {
    buf: Vec<u8>,
    cap: u64,
}

impl Write for CapWriter {
    fn write(&mut self, data: &[u8]) -> io::Result<usize> {
        if self.buf.len() as u64 + data.len() as u64 > self.cap {
            return Err(io::Error::new(
                io::ErrorKind::InvalidData,
                "entry larger than its declared size",
            ));
        }
        self.buf.extend_from_slice(data);
        Ok(data.len())
    }

    fn flush(&mut self) -> io::Result<()> {
        Ok(())
    }
}

struct DigestWriter(Sha256);

impl Write for DigestWriter {
    fn write(&mut self, data: &[u8]) -> io::Result<usize> {
        self.0.update(data);
        Ok(data.len())
    }

    fn flush(&mut self) -> io::Result<()> {
        Ok(())
    }
}

struct MemoryHandle(Vec<u8>);

#[async_trait]
impl ReadHandle for MemoryHandle {
    fn size(&self) -> Option<u64> {
        Some(self.0.len() as u64)
    }

    async fn read_at(&self, offset: u64, max: usize) -> Result<Vec<u8>> {
        let start = usize::try_from(offset).unwrap_or(usize::MAX).min(self.0.len());
        let end = start.saturating_add(max).min(self.0.len());
        Ok(self.0[start..end].to_vec())
    }
}

/// A large entry extracted to an unlinked temporary file.
struct TempFileHandle {
    file: Arc<File>,
    size: u64,
}

#[async_trait]
impl ReadHandle for TempFileHandle {
    fn size(&self) -> Option<u64> {
        Some(self.size)
    }

    async fn read_at(&self, offset: u64, max: usize) -> Result<Vec<u8>> {
        let file = Arc::clone(&self.file);
        let want = max.min(1 << 20);
        tokio::task::spawn_blocking(move || {
            let mut buf = vec![0u8; want];
            let n = file.read_at(&mut buf, offset)?;
            buf.truncate(n);
            Ok(buf)
        })
        .await
        .map_err(join_err)?
    }
}

fn extract_to_temp(inner: &Inner, node: &Node) -> Result<File> {
    std::fs::create_dir_all(&inner.cache_dir)?;
    let path = inner.cache_dir.join(unique_name("entry"));
    let file = std::fs::OpenOptions::new()
        .read(true)
        .write(true)
        .create_new(true)
        .open(&path)?;
    // Unlinked at once: the data lives as long as the handle and cannot leak.
    std::fs::remove_file(&path)?;
    let mut writer = io::BufWriter::new(&file);
    inner.copy_node(node, &mut writer)?;
    writer.flush()?;
    drop(writer);
    Ok(file)
}

#[async_trait]
impl Provider for ArchiveProvider {
    fn capabilities(&self) -> Capabilities {
        Capabilities::with(&[cap::READ_ONLY, cap::RANDOM_READ])
    }

    async fn list(&self, dir: &VPath, _lane: Lane, out: mpsc::Sender<Vec<Entry>>) -> Result<()> {
        let inner = &self.inner;
        let (resolved, node) = inner.resolve(dir, true)?;
        if node.kind != Kind::Dir {
            return Err(Error::kind(ErrorKind::NotADirectory));
        }
        let names = inner.index.children.get(&resolved).cloned().unwrap_or_default();
        for chunk in names.chunks(LIST_BATCH) {
            let mut batch = Vec::with_capacity(chunk.len());
            for name in chunk {
                let path = resolved.join(name)?;
                batch.push(inner.make_entry(&path, inner.node(&path)?));
            }
            out.send(batch)
                .await
                .map_err(|_| Error::kind(ErrorKind::Canceled))?;
        }
        Ok(())
    }

    async fn stat(&self, path: &VPath, follow: bool, _lane: Lane) -> Result<Entry> {
        let (resolved, node) = self.inner.resolve(path, follow)?;
        let mut entry = self.inner.make_entry(&resolved, node);
        if follow {
            entry.name = path.name().unwrap_or_default().to_vec();
        }
        Ok(entry)
    }

    async fn read_link(&self, path: &VPath) -> Result<Vec<u8>> {
        let node = self.inner.node(path)?;
        if node.kind != Kind::Symlink {
            return Err(Error::new(ErrorKind::InvalidArgument, "not a symbolic link"));
        }
        let path = path.clone();
        let node = node.clone();
        self.run(move |inner| inner.link_bytes(&path, &node)).await
    }

    async fn make_dir(&self, _path: &VPath, _exclusive: bool) -> Result<()> {
        read_only()
    }

    async fn make_file(&self, _path: &VPath) -> Result<()> {
        read_only()
    }

    async fn remove_file(&self, _path: &VPath) -> Result<()> {
        read_only()
    }

    async fn remove_dir(&self, _path: &VPath) -> Result<()> {
        read_only()
    }

    async fn rename(&self, _from: &VPath, _to: &VPath, _mode: RenameMode) -> Result<()> {
        read_only()
    }

    async fn set_attributes(&self, _path: &VPath, _changes: AttributeChanges) -> Result<()> {
        read_only()
    }

    async fn make_symlink(&self, _target: &[u8], _link: &VPath) -> Result<()> {
        read_only()
    }

    async fn make_hardlink(&self, _existing: &VPath, _new_path: &VPath) -> Result<()> {
        read_only()
    }

    async fn open_read(&self, path: &VPath, _lane: Lane) -> Result<Box<dyn ReadHandle>> {
        let (_, node) = self.inner.resolve(path, true)?;
        if node.kind == Kind::Dir {
            return Err(Error::kind(ErrorKind::IsADirectory));
        }
        let node = node.clone();
        self.run(move |inner| -> Result<Box<dyn ReadHandle>> {
            if node.size > MEMORY_LIMIT {
                let file = extract_to_temp(inner, &node)?;
                let size = file.metadata()?.len(); // NOSONAR: runs on the blocking pool
                return Ok(Box::new(TempFileHandle {
                    file: Arc::new(file),
                    size,
                }));
            }
            let mut sink = CapWriter {
                buf: Vec::new(),
                cap: node.size,
            };
            inner.copy_node(&node, &mut sink)?;
            Ok(Box::new(MemoryHandle(sink.buf)))
        })
        .await
    }

    async fn upload_from(
        &self,
        _src: OwnedFd,
        _dst: &VPath,
        _opts: WriteOptions,
        _progress: ProgressSink,
    ) -> Result<()> {
        read_only()
    }

    async fn download_into(
        &self,
        src: &VPath,
        dst: OwnedFd,
        opts: ReadOptions,
        progress: ProgressSink,
    ) -> Result<()> {
        let (_, node) = self.inner.resolve(src, true)?;
        if node.kind == Kind::Dir {
            return Err(Error::kind(ErrorKind::IsADirectory));
        }
        let node = node.clone();
        self.run(move |inner| {
            let total = node.size.saturating_sub(opts.offset);
            let mut sink = SinkWriter::new(File::from(dst), opts.offset, total, progress);
            inner.copy_node(&node, &mut sink)?;
            sink.finish().map_err(Error::from)
        })
        .await
    }

    async fn server_copy(&self, _from: &VPath, _to: &VPath, _opts: CopyOptions) -> Result<()> {
        read_only()
    }

    async fn checksum(&self, path: &VPath, algorithm: &str) -> Result<Vec<u8>> {
        if !algorithm.eq_ignore_ascii_case("sha256") {
            return Err(Error::new(
                ErrorKind::Unsupported,
                format!("checksum {algorithm}"),
            ));
        }
        let (_, node) = self.inner.resolve(path, true)?;
        let node = node.clone();
        self.run(move |inner| {
            let mut w = DigestWriter(Sha256::new());
            inner.copy_node(&node, &mut w)?;
            Ok(w.0.finalize().to_vec())
        })
        .await
    }

    async fn space(&self, _dir: &VPath) -> Result<SpaceInfo> {
        Err(Error::new(
            ErrorKind::Unsupported,
            "no space information for archives",
        ))
    }
}
