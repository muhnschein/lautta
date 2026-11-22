// SPDX-License-Identifier: LGPL-2.1-or-later
//! Compressed files: zip, 7z, tar.xz, tar.gz written into a [`Sink`]. For remote destinations the
//! caller passes a pipe's write end and uploads the read end meanwhile.
//!
//! * 7z needs to seek: written straight into a seekable sink, else built in a spool file first.
//! * Folders are added recursively, children sorted by name: deterministic output.
//! * Symlinks are stored as links in both formats, never followed: no cycles, nothing outside the
//!   selection is read.
//! * Special files (devices, sockets) are skipped and counted.
//! * The zip writer is our own: the `zip` crate cannot write to a pipe.
//! * A file that changes size while read aborts the run rather than writing a corrupt tar entry.

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

const CHUNK: usize = 256 * 1024;
const QUEUE: usize = 8;
/// tar needs sizes up front; unknown-size files are spooled in memory up to this.
const SPOOL_LIMIT: u64 = 64 * 1024 * 1024;
const WRITER_GONE: &str = "archive writer stopped";

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ArchiveKind {
    Zip,
    SevenZ,
    TarXz,
    TarGz,
}

impl ArchiveKind {
    /// tar headers carry the size before the data.
    fn needs_size(self) -> bool {
        matches!(self, ArchiveKind::TarXz | ArchiveKind::TarGz)
    }
}

/// Where the compressed bytes go. [`Sink::into_file`] hands over a seekable file for 7z.
pub trait Sink: Write + Send + 'static {
    fn into_file(self) -> std::result::Result<std::fs::File, Self>
    where
        Self: Sized,
    {
        Err(self)
    }
}

impl Sink for std::fs::File {
    /// A pipe cannot seek.
    fn into_file(mut self) -> std::result::Result<std::fs::File, Self> {
        match io::Seek::stream_position(&mut self) {
            Ok(_) => Ok(self),
            Err(_) => Err(self),
        }
    }
}

#[derive(Clone)]
pub struct CompressOptions {
    pub kind: ArchiveKind,
    /// Level 0-9 (default 6).
    pub level: u32,
    pub total_hint: Option<u64>,
    /// 7z into an unseekable sink is built here first; `None`: system temp dir.
    pub spool_dir: Option<std::path::PathBuf>,
    pub progress: ProgressSink,
    pub cancel: Arc<AtomicBool>,
}

impl CompressOptions {
    pub fn new(kind: ArchiveKind) -> CompressOptions {
        CompressOptions {
            kind,
            level: 6,
            total_hint: None,
            spool_dir: None,
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
    pub bytes: u64,
}

#[derive(Debug, Clone)]
pub(crate) struct EntryMeta {
    pub path: Vec<u8>,
    pub kind: Kind,
    /// File size when known; the data stream must then match it exactly.
    pub size: Option<u64>,
    pub mtime: Option<SystemTime>,
    pub mode: Option<u32>,
    pub link: Option<Vec<u8>>,
}

enum Msg {
    Entry(EntryMeta),
    Data(Vec<u8>),
    EndData,
    Finish,
}

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
                _ => return Err(io::Error::other("compression aborted")),
            }
        }
    }
}

pub(crate) trait MemberWriter: Send {
    fn member(&mut self, meta: &EntryMeta, data: &mut dyn Read) -> io::Result<()>;
    fn finish(self: Box<Self>) -> io::Result<()>;
}

/// tar inside a compressor `E`; `close` ends the compressed stream.
struct Tar<E: Write> {
    builder: tar::Builder<E>,
    close: fn(E) -> io::Result<()>,
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

impl<E: Write + Send> MemberWriter for Tar<E> {
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
        let inner = self.builder.into_inner()?;
        (self.close)(inner)
    }
}

/// 7z stores unix mode and type in the high half of the attributes when bit 15 is set.
const SEVENZ_UNIX_EXTENSION: u32 = 0x8000;
const SEVENZ_DIRECTORY: u32 = 0x10;
/// Largest LZMA2 dictionary used (preset 6's); small files get a smaller one, so a folder of many
/// small files does not set up 8 MiB match tables per file.
const SEVENZ_DICT_MAX: u32 = 8 * 1024 * 1024;
const SEVENZ_DICT_MIN: u32 = 64 * 1024;

fn sevenz_dict(size: Option<u64>) -> u32 {
    match size {
        Some(n) => u32::try_from(n)
            .unwrap_or(u32::MAX)
            .checked_next_power_of_two()
            .unwrap_or(SEVENZ_DICT_MAX)
            .clamp(SEVENZ_DICT_MIN, SEVENZ_DICT_MAX),
        None => SEVENZ_DICT_MAX,
    }
}

fn sevenz_err(e: sevenz_rust2::Error) -> io::Error {
    io::Error::other(format!("7z: {e}"))
}

/// Entries are compressed one by one (not solid): any file can be read without the ones before it.
struct SevenZ<W: Sink> {
    archive: sevenz_rust2::ArchiveWriter<std::fs::File>,
    level: u32,
    /// Unseekable sink: archive goes to a spool file, copied here at the end.
    forward: Option<W>,
}

impl<W: Sink> SevenZ<W> {
    fn new(sink: W, level: u32, spool_dir: Option<&Path>) -> io::Result<Self> {
        let (file, forward) = match sink.into_file() {
            Ok(file) => (file, None),
            Err(sink) => {
                let dir = spool_dir.map_or_else(std::env::temp_dir, Path::to_path_buf);
                std::fs::create_dir_all(&dir)?;
                (tempfile::tempfile_in(dir)?, Some(sink))
            }
        };
        let archive = sevenz_rust2::ArchiveWriter::new(file).map_err(sevenz_err)?;
        Ok(SevenZ {
            archive,
            level,
            forward,
        })
    }

    fn entry(meta: &EntryMeta) -> io::Result<sevenz_rust2::ArchiveEntry> {
        let name = std::str::from_utf8(&meta.path)
            .map_err(|_| io::Error::new(io::ErrorKind::InvalidInput, "7z names must be UTF-8"))?;
        let (mut entry, unix_type, default_mode) = match meta.kind {
            Kind::Dir => (sevenz_rust2::ArchiveEntry::new_directory(name), 0o040_000, 0o755),
            Kind::Symlink => (sevenz_rust2::ArchiveEntry::new_file(name), 0o120_000, 0o777),
            _ => (sevenz_rust2::ArchiveEntry::new_file(name), 0o100_000, 0o644),
        };
        let mode = unix_type | (meta.mode.unwrap_or(default_mode) & 0o7777);
        entry.has_windows_attributes = true;
        entry.windows_attributes = (mode << 16)
            | SEVENZ_UNIX_EXTENSION
            | if meta.kind == Kind::Dir {
                SEVENZ_DIRECTORY
            } else {
                0
            };
        if let Some(t) = meta.mtime.and_then(|t| sevenz_rust2::NtTime::try_from(t).ok()) {
            entry.has_last_modified_date = true;
            entry.last_modified_date = t;
        }
        Ok(entry)
    }
}

impl<W: Sink> MemberWriter for SevenZ<W> {
    fn member(&mut self, meta: &EntryMeta, data: &mut dyn Read) -> io::Result<()> {
        let entry = Self::entry(meta)?;
        let mut options = sevenz_rust2::encoder_options::Lzma2Options::from_level(self.level);
        options.set_dictionary_size(sevenz_dict(meta.size));
        self.archive.set_content_methods(vec![options.into()]);
        let pushed = match meta.kind {
            Kind::Dir => self.archive.push_archive_entry::<&[u8]>(entry, None),
            Kind::Symlink => {
                let target = meta.link.as_deref().unwrap_or_default();
                self.archive.push_archive_entry(entry, Some(target))
            }
            _ => self.archive.push_archive_entry(entry, Some(data)),
        };
        pushed.map(|_| ()).map_err(sevenz_err)
    }

    fn finish(self: Box<Self>) -> io::Result<()> {
        let mut file = self.archive.finish()?;
        match self.forward {
            Some(mut sink) => {
                io::Seek::seek(&mut file, io::SeekFrom::Start(0))?;
                io::copy(&mut file, &mut sink)?;
                sink.flush()
            }
            None => file.flush(),
        }
    }
}

fn new_writer<W: Sink>(opts: &CompressOptions, sink: W) -> io::Result<Box<dyn MemberWriter>> {
    let level = opts.level.min(9);
    Ok(match opts.kind {
        ArchiveKind::Zip => Box::new(zipstream::ZipStream::new(sink, level)),
        ArchiveKind::SevenZ => Box::new(SevenZ::new(sink, level, opts.spool_dir.as_deref())?),
        ArchiveKind::TarXz => Box::new(Tar {
            builder: tar::Builder::new(xz2::write::XzEncoder::new(sink, level)),
            close: |e| e.finish()?.flush(),
        }),
        ArchiveKind::TarGz => Box::new(Tar {
            builder: tar::Builder::new(flate2::write::GzEncoder::new(
                sink,
                flate2::Compression::new(level),
            )),
            close: |e| e.finish()?.flush(),
        }),
    })
}

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
        if meta.size.is_none() && self.opts.kind.needs_size() {
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

/// Blocks on `sink` only in a dedicated thread; sources are read async, backpressured by the sink's
/// speed. Top-level names must be distinct.
pub async fn compress<W: Sink>(
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
    let writer = {
        let opts = opts.clone();
        tokio::task::spawn_blocking(move || new_writer(&opts, sink))
            .await
            .map_err(|e| Error::new(ErrorKind::Internal, format!("archive writer failed: {e}")))??
    };
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

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ExtractItem {
    pub path: VPath,
    pub kind: Kind,
    pub size: u64,
}

/// Parents before children.
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
