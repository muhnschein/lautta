// SPDX-License-Identifier: LGPL-2.1-or-later
//! Creating archives (SPEC PRV-11, §10.1 "Compress"): zip or tar.gz from any
//! selection, streamed into a `std::io::Write` sink. When the destination is
//! remote the caller passes the write end of a pipe and uploads the read end
//! while the archive is being produced (NVB-9); nothing is buffered beyond a
//! few chunks.
//!
//! Choices worth knowing:
//! * Folders are added recursively, children sorted by name, so the output is
//!   deterministic.
//! * Symbolic links are stored *as links* in both formats (tar symlink entry;
//!   zip entry with unix mode `S_IFLNK` whose data is the target). They are
//!   never followed: no cycles, nothing outside the selection is read, and
//!   extracting restores what was there.
//! * Special files (devices, sockets) are skipped and counted.
//! * The zip writer is our own (the `zip` crate cannot write to a pipe): data
//!   descriptors, deflate, zip64 whenever a size is unknown or ≥ 4 GiB, and
//!   it reads back with any unzip tool and with [`crate::provider::archive`].
//! * A file that changes size while it is read aborts the operation rather
//!   than writing a corrupt tar entry.

mod zipstream;

use crate::entry::{Entry, Kind};
use crate::error::{Error, ErrorKind, Result};
use crate::provider::archive::ArchiveProvider;
use crate::provider::{no_progress, Lane, ProgressSink, Provider, ProviderResolver};
use crate::uri::Uri;
use crate::vpath::{validate_name, VPath};
use std::io::{self, Read, Write};
use std::os::unix::ffi::OsStrExt;
use std::path::Path;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::time::SystemTime;
use tokio::sync::mpsc;

/// Bytes read from a source per request.
const CHUNK: usize = 256 * 1024;
/// Chunks in flight between the reader and the writer thread.
const QUEUE: usize = 8;
/// tar needs sizes up front; unknown-size files are spooled in memory up to this.
const SPOOL_LIMIT: u64 = 64 * 1024 * 1024;
const WRITER_GONE: &str = "archive writer stopped";

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ArchiveKind {
    Zip,
    TarGz,
}

/// Knobs and hooks of one compression run.
#[derive(Clone)]
pub struct CompressOptions {
    pub kind: ArchiveKind,
    /// Deflate level 0-9 (default 6).
    pub level: u32,
    /// Total input bytes when the planner knows them (progress denominator).
    pub total_hint: Option<u64>,
    /// Called with (bytes read so far, `total_hint`).
    pub progress: ProgressSink,
    /// Set to stop; the run ends with `ErrorKind::Canceled`.
    pub cancel: Arc<AtomicBool>,
}

impl CompressOptions {
    pub fn new(kind: ArchiveKind) -> CompressOptions {
        CompressOptions {
            kind,
            level: 6,
            total_hint: None,
            progress: no_progress(),
            cancel: Arc::new(AtomicBool::new(false)),
        }
    }
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct CompressStats {
    pub files: u64,
    pub dirs: u64,
    pub symlinks: u64,
    pub skipped: u64,
    /// Input bytes read.
    pub bytes: u64,
}

/// What the writer needs to know about one archive member.
#[derive(Debug, Clone)]
pub(crate) struct EntryMeta {
    /// Archive-relative name, `/`-separated, no leading or trailing slash.
    pub path: Vec<u8>,
    pub kind: Kind,
    /// File size when known; the data stream must then match it exactly.
    pub size: Option<u64>,
    pub mtime: Option<SystemTime>,
    pub mode: Option<u32>,
    /// Symlink target.
    pub link: Option<Vec<u8>>,
}

enum Msg {
    Entry(EntryMeta),
    Data(Vec<u8>),
    EndData,
    Finish,
}

/// Presents the `Data` messages of the current file as a `Read`.
struct ChunkReader<'a> {
    rx: &'a mut mpsc::Receiver<Msg>,
    buf: Vec<u8>,
    pos: usize,
    done: bool,
}

impl Read for ChunkReader<'_> {
    fn read(&mut self, out: &mut [u8]) -> io::Result<usize> {
        loop {
            if self.pos < self.buf.len() {
                let n = (self.buf.len() - self.pos).min(out.len());
                out[..n].copy_from_slice(&self.buf[self.pos..self.pos + n]);
                self.pos += n;
                return Ok(n);
            }
            if self.done {
                return Ok(0);
            }
            match self.rx.blocking_recv() {
                Some(Msg::Data(v)) => {
                    self.buf = v;
                    self.pos = 0;
                }
                Some(Msg::EndData) => self.done = true,
                // Not `Interrupted`: `io::copy` would retry that forever.
                _ => return Err(io::Error::new(io::ErrorKind::Other, "compression aborted")),
            }
        }
    }
}

/// A format writer fed one member at a time.
pub(crate) trait MemberWriter: Send {
    /// `data` yields the file's bytes (empty for folders and links).
    fn member(&mut self, meta: &EntryMeta, data: &mut dyn Read) -> io::Result<()>;
    fn finish(self: Box<Self>) -> io::Result<()>;
}

struct TarGz<W: Write> {
    builder: tar::Builder<flate2::write::GzEncoder<W>>,
}

fn tar_header(meta: &EntryMeta, kind: tar::EntryType, size: u64) -> tar::Header {
    let mut h = tar::Header::new_gnu();
    h.set_entry_type(kind);
    h.set_size(size);
    h.set_mode(meta.mode.unwrap_or(0o644) & 0o7777);
    let secs = meta
        .mtime
        .and_then(|t| t.duration_since(SystemTime::UNIX_EPOCH).ok())
        .map_or(0, |d| d.as_secs());
    h.set_mtime(secs);
    h
}

impl<W: Write + Send> MemberWriter for TarGz<W> {
    fn member(&mut self, meta: &EntryMeta, data: &mut dyn Read) -> io::Result<()> {
        let path = Path::new(std::ffi::OsStr::from_bytes(&meta.path));
        match meta.kind {
            Kind::Dir => {
                let mut h = tar_header(meta, tar::EntryType::Directory, 0);
                h.set_mode(meta.mode.unwrap_or(0o755) & 0o7777);
                self.builder.append_data(&mut h, path, io::empty())
            }
            Kind::Symlink => {
                let target = meta.link.as_deref().unwrap_or_default();
                let mut h = tar_header(meta, tar::EntryType::Symlink, 0);
                self.builder
                    .append_link(&mut h, path, Path::new(std::ffi::OsStr::from_bytes(target)))
            }
            _ => {
                let size = meta
                    .size
                    .ok_or_else(|| io::Error::new(io::ErrorKind::InvalidInput, "tar needs the size"))?;
                let mut h = tar_header(meta, tar::EntryType::Regular, size);
                self.builder.append_data(&mut h, path, data)
            }
        }
    }

    fn finish(self: Box<Self>) -> io::Result<()> {
        let enc = self.builder.into_inner()?;
        let mut out = enc.finish()?;
        out.flush()
    }
}

fn new_writer<W: Write + Send + 'static>(kind: ArchiveKind, level: u32, sink: W) -> Box<dyn MemberWriter> {
    let level = level.min(9);
    match kind {
        ArchiveKind::Zip => Box::new(zipstream::ZipStream::new(sink, level)),
        ArchiveKind::TarGz => Box::new(TarGz {
            builder: tar::Builder::new(flate2::write::GzEncoder::new(
                sink,
                flate2::Compression::new(level),
            )),
        }),
    }
}

/// The writer thread: builds the archive from messages until `Finish`.
fn write_archive(mut rx: mpsc::Receiver<Msg>, mut writer: Box<dyn MemberWriter>) -> Result<()> {
    loop {
        match rx.blocking_recv() {
            Some(Msg::Entry(meta)) => {
                let mut data = ChunkReader {
                    rx: &mut rx,
                    buf: Vec::new(),
                    pos: 0,
                    done: meta.kind != Kind::File,
                };
                writer.member(&meta, &mut data)?;
                if !data.done {
                    return Err(Error::new(ErrorKind::Internal, "member data was not consumed"));
                }
            }
            Some(Msg::Finish) => return Ok(writer.finish()?),
            Some(_) => return Err(Error::new(ErrorKind::Internal, "unexpected message")),
            None => return Err(Error::kind(ErrorKind::Canceled)),
        }
    }
}

struct Walker<'a> {
    resolver: &'a dyn ProviderResolver,
    opts: &'a CompressOptions,
    tx: mpsc::Sender<Msg>,
    stats: CompressStats,
}

struct Item {
    provider: Arc<dyn Provider>,
    path: VPath,
    name: Vec<u8>,
    entry: Entry,
}

fn writer_gone() -> Error {
    Error::new(ErrorKind::Io, WRITER_GONE)
}

impl Walker<'_> {
    async fn send(&self, msg: Msg) -> Result<()> {
        self.tx.send(msg).await.map_err(|_| writer_gone())
    }

    fn check_cancel(&self) -> Result<()> {
        if self.opts.cancel.load(Ordering::Relaxed) {
            Err(Error::kind(ErrorKind::Canceled))
        } else {
            Ok(())
        }
    }

    async fn add_source(&mut self, uri: &Uri, name: Vec<u8>) -> Result<()> {
        let provider = self.resolver.provider(&uri.location)?;
        let entry = provider.stat(&uri.path, false, Lane::Bulk).await?;
        let mut stack = vec![Item {
            provider,
            path: uri.path.clone(),
            name,
            entry,
        }];
        while let Some(item) = stack.pop() {
            self.check_cancel()?;
            match item.entry.kind {
                Kind::Dir => self.add_dir(item, &mut stack).await?,
                Kind::File => self.add_file(&item).await?,
                Kind::Symlink => self.add_link(&item).await?,
                _ => self.stats.skipped += 1,
            }
        }
        Ok(())
    }

    fn meta(item: &Item) -> EntryMeta {
        EntryMeta {
            path: item.name.clone(),
            kind: item.entry.kind,
            size: item.entry.size,
            mtime: item.entry.modified,
            mode: item.entry.mode,
            link: None,
        }
    }

    async fn add_dir(&mut self, item: Item, stack: &mut Vec<Item>) -> Result<()> {
        self.send(Msg::Entry(Self::meta(&item))).await?;
        self.stats.dirs += 1;
        let mut children = crate::provider::list_all(item.provider.as_ref(), &item.path, Lane::Bulk).await?;
        children.sort_by(|a, b| b.name.cmp(&a.name));
        // Reverse order on the stack: the alphabetically first child is popped first.
        for child in children {
            validate_name(&child.name)?;
            let mut name = item.name.clone();
            name.push(b'/');
            name.extend_from_slice(&child.name);
            stack.push(Item {
                provider: Arc::clone(&item.provider),
                path: item.path.join(&child.name)?,
                name,
                entry: child,
            });
        }
        Ok(())
    }

    async fn add_link(&mut self, item: &Item) -> Result<()> {
        let mut meta = Self::meta(item);
        meta.size = None;
        meta.link = Some(item.provider.read_link(&item.path).await?);
        self.send(Msg::Entry(meta)).await?;
        self.stats.symlinks += 1;
        Ok(())
    }

    async fn add_file(&mut self, item: &Item) -> Result<()> {
        let handle = item.provider.open_read(&item.path, Lane::Bulk).await?;
        let mut meta = Self::meta(item);
        meta.size = item.entry.size.or_else(|| handle.size());
        let mut spool = None;
        if meta.size.is_none() && self.opts.kind == ArchiveKind::TarGz {
            let data = crate::provider::read_all(handle.as_ref(), SPOOL_LIMIT + 1).await?;
            if data.len() as u64 > SPOOL_LIMIT {
                return Err(Error::new(
                    ErrorKind::Unsupported,
                    "file of unknown size is too large for tar",
                ));
            }
            meta.size = Some(data.len() as u64);
            spool = Some(data);
        }
        let size = meta.size;
        self.send(Msg::Entry(meta)).await?;
        match spool {
            Some(data) => self.send_data(data).await?,
            None => self.stream_file(handle.as_ref(), size).await?,
        }
        self.send(Msg::EndData).await?;
        self.stats.files += 1;
        Ok(())
    }

    async fn send_data(&mut self, data: Vec<u8>) -> Result<()> {
        self.stats.bytes += data.len() as u64;
        (self.opts.progress)(self.stats.bytes, self.opts.total_hint);
        self.send(Msg::Data(data)).await
    }

    /// Sends exactly `size` bytes when known, to EOF otherwise.
    async fn stream_file(
        &mut self,
        handle: &dyn crate::provider::ReadHandle,
        size: Option<u64>,
    ) -> Result<()> {
        let mut offset = 0u64;
        loop {
            self.check_cancel()?;
            let want = size.map_or(CHUNK as u64, |s| (s - offset).min(CHUNK as u64));
            if want == 0 {
                return Ok(());
            }
            let mut chunk = handle
                .read_at(offset, usize::try_from(want).unwrap_or(CHUNK))
                .await?;
            if chunk.is_empty() {
                return match size {
                    Some(_) => Err(Error::new(ErrorKind::Io, "file changed while compressing")),
                    None => Ok(()),
                };
            }
            chunk.truncate(usize::try_from(want).unwrap_or(CHUNK));
            offset += chunk.len() as u64;
            self.send_data(chunk).await?;
        }
    }
}

/// Top-level member name for a source.
fn source_name(uri: &Uri) -> Result<Vec<u8>> {
    match uri.path.name() {
        Some(n) => Ok(n.to_vec()),
        None => Err(Error::new(
            ErrorKind::InvalidArgument,
            "cannot compress a location root",
        )),
    }
}

fn top_level_names(sources: &[Uri]) -> Result<Vec<Vec<u8>>> {
    let mut seen = std::collections::HashSet::new();
    let mut names = Vec::with_capacity(sources.len());
    for uri in sources {
        let name = source_name(uri)?;
        if !seen.insert(name.clone()) {
            return Err(Error::new(
                ErrorKind::AlreadyExists,
                format!(
                    "two items named {} in the selection",
                    String::from_utf8_lossy(&name)
                ),
            ));
        }
        names.push(name);
    }
    Ok(names)
}

async fn walk(
    resolver: &dyn ProviderResolver,
    sources: &[Uri],
    names: Vec<Vec<u8>>,
    opts: &CompressOptions,
    tx: mpsc::Sender<Msg>,
) -> Result<CompressStats> {
    let mut walker = Walker {
        resolver,
        opts,
        tx,
        stats: CompressStats::default(),
    };
    for (uri, name) in sources.iter().zip(names) {
        walker.add_source(uri, name).await?;
    }
    walker.send(Msg::Finish).await?;
    Ok(walker.stats)
}

/// Writes an archive of `sources` into `sink` (PRV-11). Blocks on `sink` only
/// in a dedicated thread; reading sources is async and backpressured by the
/// sink's speed. Top-level names must be distinct.
pub async fn compress<W: Write + Send + 'static>(
    resolver: &dyn ProviderResolver,
    sources: &[Uri],
    sink: W,
    opts: &CompressOptions,
) -> Result<CompressStats> {
    if sources.is_empty() {
        return Err(Error::new(ErrorKind::InvalidArgument, "nothing to compress"));
    }
    let names = top_level_names(sources)?;
    let (tx, rx) = mpsc::channel(QUEUE);
    let writer = new_writer(opts.kind, opts.level, sink);
    let thread = tokio::task::spawn_blocking(move || write_archive(rx, writer));
    let walked = walk(resolver, sources, names, opts, tx).await;
    let written = thread
        .await
        .map_err(|e| Error::new(ErrorKind::Internal, format!("archive writer failed: {e}")))?;
    match walked {
        Ok(stats) => written.map(|()| stats),
        // The sink failed: its error is the real one.
        Err(e) if e.message == WRITER_GONE => Err(written.err().unwrap_or(e)),
        Err(e) => Err(e),
    }
}

/// One member of an archive as the transfer planner sees it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ExtractItem {
    /// Path relative to the extracted folder (or the file's own name).
    pub path: VPath,
    pub kind: Kind,
    pub size: u64,
}

/// What extracting `under` (a folder or a file inside `archive`) would create,
/// parents before children (§10.1, XFR-3).
pub fn extract_plan(archive: &ArchiveProvider, under: &VPath) -> Result<Vec<ExtractItem>> {
    let all = archive.entries();
    let Some(root) = all.iter().find(|e| &e.path == under) else {
        if under.is_root() {
            return Ok(plan(&all, under));
        }
        return Err(Error::kind(ErrorKind::NotFound));
    };
    if root.kind == Kind::Dir {
        return Ok(plan(&all, under));
    }
    let name = under.name().unwrap_or_default();
    Ok(vec![ExtractItem {
        path: VPath::root().join(name)?,
        kind: root.kind,
        size: root.size,
    }])
}

fn plan(all: &[crate::provider::archive::ArchiveEntry], under: &VPath) -> Vec<ExtractItem> {
    all.iter()
        .filter(|e| &e.path != under)
        .filter_map(|e| {
            e.path.strip_prefix(under).map(|rel| ExtractItem {
                path: rel,
                kind: e.kind,
                size: e.size,
            })
        })
        .collect()
}

#[cfg(test)]
mod tests;
