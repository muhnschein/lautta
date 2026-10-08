// SPDX-License-Identifier: LGPL-2.1-or-later
//! User-level actions of the viewers area on [`Core`](crate::app::Core):
//! loading and saving text (PRV-4, EDT-4, EDT-2), Markdown, hex, EXIF and
//! SQLite sources, the folder's images, remote thumbnails and full images
//! (PRV-2, PRV-3), media for playback (PRV-8, PRV-9) and recents (ORG-2).
//! Everything is Qt-free; the Qt layer forwards to these functions.

use crate::app::Core;
use crate::entry::{ms_to_system_time, Entry};
use crate::error::{Error, ErrorKind, Result};
use crate::mime::{category_of, FileCategory};
use crate::ops::names::keep_both_name;
use crate::org::recents::RecentKind;
use crate::preview::exif::{self, ExifInfo};
use crate::preview::hex::HexView;
use crate::preview::markdown;
use crate::preview::text::{self as ptext, TextDoc, TextMeta};
use crate::provider::{
    list_all, no_progress, Disposition, Lane, ProgressSink, Provider, ReadHandle, ReadOptions, RenameMode,
    WriteOptions,
};
use crate::sort::sort_permutation;
use crate::thumbs::{remote_thumbnail, thumb_key, ThumbJobs, ThumbnailCache};
use crate::uri::Uri;
use crate::vpath::VPath;
use std::collections::{HashMap, HashSet};
use std::os::fd::OwnedFd;
use std::path::PathBuf;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex, MutexGuard, OnceLock};
use tokio::runtime::Handle;
use tokio::sync::oneshot;

/// Largest remote image decoded for the viewer (memory, PRF-4).
pub const MAX_FULL_IMAGE_BYTES: u64 = 100 * 1000 * 1000;
/// How much of an image is read to find its EXIF block.
pub const EXIF_HEAD_BYTES: u64 = 512 * 1024;
/// Thumbnail edge limits for `image://lautta-thumb`.
pub const MIN_THUMB: u32 = 16;
pub const MAX_THUMB: u32 = 1024;

/// What identifies a file's content for the EDT-2 conflict check.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct Stamp {
    pub size: Option<u64>,
    pub mtime_ms: Option<i64>,
    pub etag: Option<Vec<u8>>,
}

impl Stamp {
    pub fn of(e: &Entry) -> Stamp {
        Stamp {
            size: e.size,
            mtime_ms: e.modified_ms(),
            etag: e.etag.clone(),
        }
    }

    /// ETags decide when both sides have one, otherwise size and mtime.
    pub fn differs(&self, now: &Stamp) -> bool {
        if let (Some(a), Some(b)) = (&self.etag, &now.etag) {
            return a != b;
        }
        self.size != now.size || self.mtime_ms != now.mtime_ms
    }
}

/// A text file as the viewer and the editor see it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LoadedText {
    pub doc: TextDoc,
    pub size: Option<u64>,
    pub stamp: Stamp,
    /// The location and the file accept writes.
    pub writable: bool,
}

impl LoadedText {
    /// EDT-4: complete, valid UTF-8, and writable.
    pub fn editable(&self) -> bool {
        self.doc.editable() && self.writable
    }
}

/// How `save_text` treats the file on disk.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SaveMode {
    /// Write only when the file still matches this baseline (EDT-2).
    Checked(Stamp),
    /// Write over whatever is there (*Upload mine and replace*).
    Replace,
    /// Write next to the original under a free name (*Save mine as copy*).
    AsCopy,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SaveConflict {
    /// The file is gone.
    pub deleted: bool,
    pub size: Option<u64>,
    pub mtime_ms: Option<i64>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SaveOutcome {
    Saved { uri: Uri, stamp: Stamp },
    Conflict(SaveConflict),
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RenderedMarkdown {
    pub html: String,
    pub truncated: bool,
}

#[derive(Debug, Clone, PartialEq)]
pub struct ExifReport {
    pub name: String,
    pub size: Option<u64>,
    pub modified_ms: Option<i64>,
    /// `None` when the file carries no EXIF block.
    pub info: Option<ExifInfo>,
}

/// One image of the folder the viewer swipes through.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ImageItem {
    pub uri: Uri,
    pub name: String,
    pub size: Option<u64>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FullImage {
    pub bytes: Vec<u8>,
    /// EXIF orientation 1-8 the decoder must apply (1 = none).
    pub orientation: u8,
}

/// Parses the *Go to offset* input: hexadecimal, with or without `0x`.
pub fn parse_offset(text: &str) -> Option<u64> {
    let t = text.trim().replace([' ', '_'], "");
    let digits = t
        .strip_prefix("0x")
        .or_else(|| t.strip_prefix("0X"))
        .unwrap_or(&t);
    if digits.is_empty() {
        return None;
    }
    u64::from_str_radix(digits, 16).ok()
}

/// Splits `<uri>?size=N` of the thumbnail provider; the size is clamped.
/// The URI may arrive percent-encoded once more (QML helpers).
pub fn parse_thumb_id(id: &str) -> Option<(Uri, u32)> {
    let (uri_part, query) = id.rsplit_once('?')?;
    let size: u32 = query.strip_prefix("size=")?.parse().ok()?;
    Some((parse_image_uri(uri_part)?, size.clamp(MIN_THUMB, MAX_THUMB)))
}

/// The id of `image://lautta-file/<uri>`.
pub fn parse_image_uri(id: &str) -> Option<Uri> {
    if let Ok(u) = Uri::parse(id) {
        return Some(u);
    }
    let decoded = percent_encoding::percent_decode_str(id).decode_utf8().ok()?;
    Uri::parse(&decoded).ok()
}

fn lock<T>(m: &Mutex<T>) -> MutexGuard<'_, T> {
    match m.lock() {
        Ok(g) => g,
        Err(p) => p.into_inner(),
    }
}

fn display(uri: &Uri) -> String {
    uri.name().map(crate::vpath::display_name).unwrap_or_default()
}

/// The unlinked file holding `bytes`, as an fd positioned at its start.
fn temp_fd(dir: &std::path::Path, bytes: &[u8]) -> Result<OwnedFd> {
    use std::io::{Seek, Write};
    static COUNTER: AtomicU64 = AtomicU64::new(0);
    std::fs::create_dir_all(dir)?;
    let name = format!(
        "save-{}-{}",
        std::process::id(),
        COUNTER.fetch_add(1, Ordering::Relaxed)
    );
    let path = dir.join(name);
    let mut file = std::fs::OpenOptions::new()
        .read(true)
        .write(true)
        .create_new(true)
        .open(&path)?;
    // The data only has to live as long as the descriptor.
    std::fs::remove_file(&path)?;
    file.write_all(bytes)?;
    file.rewind()?;
    Ok(OwnedFd::from(file))
}

/// Window the media reader keeps ahead of the player (PRV-8).
pub const READ_AHEAD_BYTES: usize = 2 * 1024 * 1024;

/// A read handle with a 2 MiB window for the media device (PRV-8): the
/// player asks for small pieces, the provider is asked for large ones, and
/// the next window is announced with `read_ahead` so a bridge can prefetch.
pub struct ReadAhead {
    handle: Arc<dyn ReadHandle>,
    size: u64,
    window: Mutex<(u64, Arc<Vec<u8>>)>,
}

impl ReadAhead {
    /// The size must be known (media streams have a length).
    pub fn new(handle: Arc<dyn ReadHandle>) -> Result<ReadAhead> {
        let size = handle
            .size()
            .ok_or_else(|| Error::new(ErrorKind::Unsupported, "size unknown"))?;
        Ok(ReadAhead {
            handle,
            size,
            window: Mutex::new((0, Arc::default())),
        })
    }

    pub fn size(&self) -> u64 {
        self.size
    }

    /// Up to `max` bytes at `offset`; empty at the end of the file.
    pub async fn read(&self, offset: u64, max: usize) -> Result<Vec<u8>> {
        if offset >= self.size || max == 0 {
            return Ok(Vec::new());
        }
        if let Some(hit) = self.cached(offset, max) {
            return Ok(hit);
        }
        let want = usize::try_from(self.size - offset)
            .unwrap_or(usize::MAX)
            .min(READ_AHEAD_BYTES);
        let data = self.handle.read_at(offset, want).await?;
        if data.is_empty() {
            return Ok(data);
        }
        let end = offset + data.len() as u64;
        let first = data[..data.len().min(max)].to_vec();
        *lock(&self.window) = (offset, Arc::new(data));
        if end < self.size {
            // A failed hint only costs the prefetch.
            let _ = self.handle.read_ahead(end, READ_AHEAD_BYTES as u64).await;
        }
        Ok(first)
    }

    fn cached(&self, offset: u64, max: usize) -> Option<Vec<u8>> {
        let w = lock(&self.window);
        let (start, data) = &*w;
        let at = usize::try_from(offset.checked_sub(*start)?).ok()?;
        let rest = data.get(at..).filter(|r| !r.is_empty())?;
        Some(rest[..rest.len().min(max)].to_vec())
    }
}

/// Process-wide thumbnail state: the disk cache (PRV-2) and the job
/// scheduler (PRV-3). Created once per cache folder.
pub struct Previews {
    cache: ThumbnailCache,
    jobs: ThumbJobs,
    waiters: Mutex<HashMap<String, Vec<(u64, ThumbSender)>>>,
    next_waiter: AtomicU64,
}

type ThumbResult = Result<Arc<Vec<u8>>>;
type ThumbSender = oneshot::Sender<ThumbResult>;

/// A pending thumbnail: await `rx`; `cancel` (or the part kept after taking
/// `rx`) drops the request when the delegate is gone.
pub struct ThumbTicket {
    pub rx: oneshot::Receiver<ThumbResult>,
    pub cancel: ThumbCancel,
}

impl ThumbTicket {
    pub fn cancel(&self) {
        self.cancel.cancel();
    }
}

/// Cancels one waiter of a thumbnail job; the job itself is cancelled when
/// nobody else waits for the same thumbnail (PRV-3).
#[derive(Clone)]
pub struct ThumbCancel {
    previews: Arc<Previews>,
    job_key: String,
    waiter: u64,
}

impl ThumbCancel {
    pub fn cancel(&self) {
        self.previews.cancel(&self.job_key, self.waiter);
    }
}

impl Previews {
    pub fn cache(&self) -> &ThumbnailCache {
        &self.cache
    }

    pub fn jobs(&self) -> &ThumbJobs {
        &self.jobs
    }

    /// Queues the thumbnail of a remote image. Several requests for the same
    /// image and size share one job.
    pub fn request(self: &Arc<Self>, core: Arc<Core>, uri: Uri, size: u32) -> ThumbTicket {
        let job_key = thumb_key(&uri.to_string(), size, None, None);
        let waiter = self.next_waiter.fetch_add(1, Ordering::Relaxed);
        let (tx, rx) = oneshot::channel();
        lock(&self.waiters)
            .entry(job_key.clone())
            .or_default()
            .push((waiter, tx));
        let me = Arc::clone(self);
        let key = job_key.clone();
        let location = uri.location.clone();
        let started = self.jobs.request(&job_key, &location, async move {
            let result = me.make(&core, &uri, size).await;
            me.finish(&key, result);
        });
        if !started && lock(&self.waiters).get(&job_key).map_or(0, Vec::len) == 1 {
            // The earlier job ended between our check and now.
            self.finish(
                &job_key,
                Err(Error::new(ErrorKind::Canceled, "thumbnail job vanished")),
            );
        }
        ThumbTicket {
            rx,
            cancel: ThumbCancel {
                previews: Arc::clone(self),
                job_key,
                waiter,
            },
        }
    }

    fn finish(&self, key: &str, result: ThumbResult) {
        let senders = lock(&self.waiters).remove(key).unwrap_or_default();
        for (_, tx) in senders {
            let _ = tx.send(result.clone());
        }
    }

    fn cancel(&self, key: &str, waiter: u64) {
        let empty = {
            let mut w = lock(&self.waiters);
            let Some(list) = w.get_mut(key) else {
                return;
            };
            list.retain(|(id, _)| *id != waiter);
            let empty = list.is_empty();
            if empty {
                w.remove(key);
            }
            empty
        };
        if empty {
            self.jobs.cancel(key);
        }
    }

    /// The thumbnail itself: cache first, then the first 64 KiB or the whole
    /// file (PRV-2).
    async fn make(&self, core: &Core, uri: &Uri, size: u32) -> ThumbResult {
        if !core.settings().thumbnails_remote {
            return Err(Error::new(ErrorKind::Unsupported, "remote thumbnails are off"));
        }
        if core.locations.to_local_path(uri).is_some() {
            return Err(Error::new(
                ErrorKind::Unsupported,
                "local files use the system thumbnailer",
            ));
        }
        let provider = core.provider(&uri.location)?;
        let entry = provider.stat(&uri.path, true, Lane::Interactive).await?;
        if entry.is_dir() {
            return Err(Error::kind(ErrorKind::IsADirectory));
        }
        let key = thumb_key(&uri.to_string(), size, entry.modified_ms(), entry.etag.as_deref());
        if let Some(hit) = self.cache.get(&key) {
            return Ok(Arc::new(hit));
        }
        let handle = provider.open_read(&uri.path, Lane::Interactive).await?;
        let bytes = remote_thumbnail(handle.as_ref(), size, false).await?;
        if let Err(e) = self.cache.put(&key, &bytes) {
            log::warn!("thumbnail not cached: {e}");
        }
        Ok(Arc::new(bytes))
    }
}

/// One `Previews` per thumbnail folder.
type PreviewsByDir = Mutex<Vec<(PathBuf, Arc<Previews>)>>;

static PREVIEWS: OnceLock<PreviewsByDir> = OnceLock::new();

impl Core {
    /// A random-access reader of the file (viewers and media).
    pub async fn open_reader(&self, uri: &Uri) -> Result<Arc<dyn ReadHandle>> {
        let provider = self.provider(&uri.location)?;
        let handle = provider.open_read(&uri.path, Lane::Interactive).await?;
        Ok(Arc::from(handle))
    }

    /// Notes a viewed, opened or edited file in Recents (ORG-2). Failures do
    /// not matter to the viewer.
    pub fn note_viewed(&self, uri: &Uri, kind: RecentKind) {
        if let Err(e) = self.recents.record(uri, &display(uri), kind) {
            log::debug!("recents not updated: {e}");
        }
    }

    /// Reads a text file for the viewer or the editor (PRV-4, EDT-4).
    pub async fn load_text(&self, uri: &Uri) -> Result<LoadedText> {
        let provider = self.provider(&uri.location)?;
        let entry = provider.stat(&uri.path, true, Lane::Interactive).await?;
        if entry.is_dir() {
            return Err(Error::kind(ErrorKind::IsADirectory));
        }
        let handle = provider.open_read(&uri.path, Lane::Interactive).await?;
        let doc = ptext::load_text(handle.as_ref()).await?;
        Ok(LoadedText {
            doc,
            size: entry.size,
            stamp: Stamp::of(&entry),
            writable: provider.capabilities().writable()
                && !entry.flags.contains(crate::entry::EntryFlags::READONLY),
        })
    }

    /// Saves edited text with the file's own conventions (EDT-4) through the
    /// provider, replacing the file only after the new content is complete.
    /// A file that changed since `SaveMode::Checked`'s baseline is reported
    /// as a conflict instead of being overwritten (EDT-2).
    pub async fn save_text(
        &self,
        uri: &Uri,
        text: &str,
        meta: &TextMeta,
        mode: SaveMode,
    ) -> Result<SaveOutcome> {
        let bytes = ptext::save_text(text, meta);
        let provider = self.provider(&uri.location)?;
        let outcome = match mode {
            SaveMode::AsCopy => {
                let copy = free_sibling(provider.as_ref(), uri).await?;
                write_new(self, provider.as_ref(), &copy, &bytes).await?;
                saved(provider.as_ref(), copy).await?
            }
            SaveMode::Replace => {
                let mode = current_mode(provider.as_ref(), uri).await;
                replace_file(self, provider.as_ref(), uri, &bytes, mode).await?;
                saved(provider.as_ref(), uri.clone()).await?
            }
            SaveMode::Checked(base) => {
                let now = match provider.stat(&uri.path, true, Lane::Interactive).await {
                    Ok(e) => Some(e),
                    Err(e) if e.kind == ErrorKind::NotFound => None,
                    Err(e) => return Err(e),
                };
                match now {
                    None => return Ok(conflict(true, None)),
                    Some(e) if base.differs(&Stamp::of(&e)) => return Ok(conflict(false, Some(&e))),
                    Some(e) => {
                        replace_file(self, provider.as_ref(), uri, &bytes, e.mode).await?;
                        saved(provider.as_ref(), uri.clone()).await?
                    }
                }
            }
        };
        self.note_viewed(uri, RecentKind::Edited);
        Ok(outcome)
    }

    /// Markdown as Qt rich text; only the first MiB is rendered (PRV-4).
    pub async fn render_markdown(&self, uri: &Uri) -> Result<RenderedMarkdown> {
        let loaded = self.load_text(uri).await?;
        Ok(RenderedMarkdown {
            html: markdown::render(&loaded.doc.text),
            truncated: loaded.doc.truncated,
        })
    }

    /// The hex view of a file (ranged reads, PRV-4).
    pub async fn open_hex(&self, uri: &Uri) -> Result<HexView> {
        HexView::new(self.open_reader(uri).await?)
    }

    /// EXIF fields of an image for the details panel (PRV-4).
    pub async fn exif_report(&self, uri: &Uri) -> Result<ExifReport> {
        let provider = self.provider(&uri.location)?;
        let entry = provider.stat(&uri.path, true, Lane::Interactive).await?;
        let handle = provider.open_read(&uri.path, Lane::Interactive).await?;
        let head = crate::provider::read_all(handle.as_ref(), EXIF_HEAD_BYTES).await?;
        let info = tokio::task::spawn_blocking(move || exif::read_exif(&head))
            .await
            .map_err(|e| Error::new(ErrorKind::Internal, e.to_string()))?;
        Ok(ExifReport {
            name: entry.display_name(),
            size: entry.size,
            modified_ms: entry.modified_ms(),
            info,
        })
    }

    /// The path of a local SQLite file; remote files must be copied first
    /// (PRV-4, *Open remote*).
    pub fn sqlite_file(&self, uri: &Uri) -> Result<PathBuf> {
        self.locations.to_local_path(uri).ok_or_else(|| {
            Error::new(
                ErrorKind::Unsupported,
                "databases are only shown from local files",
            )
        })
    }

    /// The folder's images in the folders' sort order (PRV-4: swipe through
    /// the folder).
    pub async fn list_images(&self, folder: &Uri) -> Result<Vec<ImageItem>> {
        let provider = self.provider(&folder.location)?;
        let all = list_all(provider.as_ref(), &folder.path, Lane::Interactive).await?;
        let prefs = self.view_prefs();
        let hidden = prefs.show_hidden;
        let images: Vec<Entry> = all
            .into_iter()
            .filter(|e| category_of(e) == FileCategory::Image && (hidden || !e.is_hidden()))
            .collect();
        let order = sort_permutation(&images, &prefs.sort_options());
        order
            .into_iter()
            .map(|i| {
                let e = &images[i as usize];
                Ok(ImageItem {
                    uri: folder.join(&e.name)?,
                    name: e.display_name(),
                    size: e.size,
                })
            })
            .collect()
    }

    /// A file's bytes for the full image provider (remote files, PRV-4).
    pub async fn full_image(&self, uri: &Uri) -> Result<FullImage> {
        let provider = self.provider(&uri.location)?;
        let handle = provider.open_read(&uri.path, Lane::Interactive).await?;
        if handle.size().is_some_and(|s| s > MAX_FULL_IMAGE_BYTES) {
            return Err(Error::new(ErrorKind::Unsupported, "image too large to show"));
        }
        let bytes = crate::provider::read_all(handle.as_ref(), MAX_FULL_IMAGE_BYTES + 1).await?;
        if bytes.len() as u64 > MAX_FULL_IMAGE_BYTES {
            return Err(Error::new(ErrorKind::Unsupported, "image too large to show"));
        }
        let orientation = exif::orientation(&bytes);
        Ok(FullImage { bytes, orientation })
    }

    /// The thumbnail state for this core's cache folder.
    pub fn previews(&self, rt: &Handle) -> Result<Arc<Previews>> {
        let dir = self.paths.thumbs_dir();
        let mut list = lock(PREVIEWS.get_or_init(Mutex::default));
        if let Some((_, p)) = list.iter().find(|(d, _)| *d == dir) {
            return Ok(Arc::clone(p));
        }
        let max = self.settings().thumbnail_cache_mb.saturating_mul(1_000_000);
        let previews = Arc::new(Previews {
            cache: ThumbnailCache::new(&dir, max)?,
            jobs: ThumbJobs::new(rt.clone()),
            waiters: Mutex::default(),
            next_waiter: AtomicU64::new(1),
        });
        list.push((dir, Arc::clone(&previews)));
        Ok(previews)
    }

    /// PRV-9 fallback: copies a media file into the private cache so it can
    /// be played from disk. Returns the local file.
    pub async fn download_for_playback(&self, uri: &Uri, progress: ProgressSink) -> Result<PathBuf> {
        let provider = self.provider(&uri.location)?;
        let dir = self.paths.cache_dir().join("media");
        let name = display(uri);
        let key = thumb_key(&uri.to_string(), 0, None, None);
        let target = dir.join(&key[..16]).join(&name);
        let parent = target.parent().map(PathBuf::from).unwrap_or_default();
        let file = tokio::task::spawn_blocking({
            let (parent, target) = (parent.clone(), target.clone());
            move || {
                std::fs::create_dir_all(&parent)?;
                std::fs::File::create(&target).map_err(Error::from)
            }
        })
        .await
        .map_err(|e| Error::new(ErrorKind::Internal, e.to_string()))??;
        let done = provider
            .download_into(&uri.path, OwnedFd::from(file), ReadOptions::default(), progress)
            .await;
        if done.is_err() {
            let _ = tokio::fs::remove_file(&target).await;
        }
        done.map(|()| target)
    }

    /// Removes the playback copies (cache clean-up at start).
    pub fn clear_playback_cache(&self) {
        let _ = std::fs::remove_dir_all(self.paths.cache_dir().join("media"));
    }
}

fn conflict(deleted: bool, now: Option<&Entry>) -> SaveOutcome {
    SaveOutcome::Conflict(SaveConflict {
        deleted,
        size: now.and_then(|e| e.size),
        mtime_ms: now.and_then(Entry::modified_ms),
    })
}

async fn saved(provider: &dyn Provider, uri: Uri) -> Result<SaveOutcome> {
    let entry = provider.stat(&uri.path, true, Lane::Interactive).await?;
    Ok(SaveOutcome::Saved {
        stamp: Stamp::of(&entry),
        uri,
    })
}

async fn current_mode(provider: &dyn Provider, uri: &Uri) -> Option<u32> {
    provider
        .stat(&uri.path, true, Lane::Interactive)
        .await
        .ok()
        .and_then(|e| e.mode)
}

/// `name 2.ext`, `name 3.ext`, … next to `uri` (the first free one).
async fn free_sibling(provider: &dyn Provider, uri: &Uri) -> Result<Uri> {
    let parent = uri
        .parent()
        .ok_or_else(|| Error::new(ErrorKind::InvalidArgument, "no parent folder"))?;
    let name = uri
        .name()
        .ok_or_else(|| Error::new(ErrorKind::InvalidArgument, "the location root is not a file"))?;
    let taken: HashSet<Vec<u8>> = list_all(provider, &parent.path, Lane::Interactive)
        .await?
        .into_iter()
        .map(|e| e.name)
        .collect();
    parent.join(&keep_both_name(name, &|n| taken.contains(n)))
}

fn write_options(disposition: Disposition, bytes: &[u8], mode: Option<u32>) -> WriteOptions {
    WriteOptions {
        disposition,
        size: Some(bytes.len() as u64),
        modified: Some(ms_to_system_time(crate::entry::system_time_to_ms(
            std::time::SystemTime::now(),
        ))),
        mode,
        ..WriteOptions::default()
    }
}

/// A new file; fails when the name is taken.
async fn write_new(core: &Core, provider: &dyn Provider, uri: &Uri, bytes: &[u8]) -> Result<()> {
    let fd = temp_fd(&core.paths.cache_dir().join("save"), bytes)?;
    let opts = write_options(Disposition::Create, bytes, None);
    provider.upload_from(fd, &uri.path, opts, no_progress()).await
}

/// Replaces `uri`: the content goes to a temporary sibling first and is
/// renamed over the file, so a failed upload never leaves half a file.
async fn replace_file(
    core: &Core,
    provider: &dyn Provider,
    uri: &Uri,
    bytes: &[u8],
    mode: Option<u32>,
) -> Result<()> {
    let parent = uri
        .parent()
        .ok_or_else(|| Error::new(ErrorKind::InvalidArgument, "no parent folder"))?;
    let name = uri
        .name()
        .ok_or_else(|| Error::new(ErrorKind::InvalidArgument, "the location root is not a file"))?;
    let mut temp_name = b".".to_vec();
    temp_name.extend_from_slice(name);
    temp_name.extend_from_slice(b".lautta-save");
    let temp: VPath = parent.path.join(&temp_name)?;
    // A leftover from an earlier failed save.
    match provider.remove_file(&temp).await {
        Err(e) if e.kind != ErrorKind::NotFound => return Err(e),
        _ => {}
    }
    let fd = temp_fd(&core.paths.cache_dir().join("save"), bytes)?;
    let opts = write_options(Disposition::Create, bytes, mode);
    let done = match provider.upload_from(fd, &temp, opts, no_progress()).await {
        Ok(()) => provider.rename(&temp, &uri.path, RenameMode::Replace).await,
        Err(e) => Err(e),
    };
    if done.is_err() {
        let _ = provider.remove_file(&temp).await;
    }
    done
}

#[cfg(test)]
mod tests {
    use super::*;

    struct Counting {
        data: Vec<u8>,
        reads: std::sync::atomic::AtomicUsize,
        hints: std::sync::atomic::AtomicUsize,
    }

    #[async_trait::async_trait]
    impl ReadHandle for Counting {
        fn size(&self) -> Option<u64> {
            Some(self.data.len() as u64)
        }
        async fn read_at(&self, offset: u64, max: usize) -> Result<Vec<u8>> {
            self.reads.fetch_add(1, Ordering::Relaxed);
            let start = usize::try_from(offset).unwrap_or(usize::MAX).min(self.data.len());
            Ok(self.data[start..(start + max).min(self.data.len())].to_vec())
        }
        async fn read_ahead(&self, _offset: u64, _bytes: u64) -> Result<()> {
            self.hints.fetch_add(1, Ordering::Relaxed);
            Ok(())
        }
    }

    #[tokio::test]
    async fn the_media_reader_serves_small_reads_from_one_big_one() {
        let total = READ_AHEAD_BYTES * 2 + 1000;
        let data: Vec<u8> = (0..total).map(|i| (i % 251) as u8).collect();
        let h = Arc::new(Counting {
            data: data.clone(),
            reads: Default::default(),
            hints: Default::default(),
        });
        let r = ReadAhead::new(h.clone()).unwrap();
        assert_eq!(r.size(), total as u64);
        for i in 0..100u64 {
            let got = r.read(i * 4096, 4096).await.unwrap();
            assert_eq!(got, data[(i * 4096) as usize..(i * 4096 + 4096) as usize]);
        }
        assert_eq!(
            h.reads.load(Ordering::Relaxed),
            1,
            "one 2 MiB window serves 400 KiB"
        );
        assert_eq!(h.hints.load(Ordering::Relaxed), 1, "the next window is announced");
        // A seek far away fetches a new window.
        let at = (READ_AHEAD_BYTES + 5) as u64;
        assert_eq!(r.read(at, 10).await.unwrap(), data[at as usize..at as usize + 10]);
        assert_eq!(h.reads.load(Ordering::Relaxed), 2);
        // A read crossing the window end is cut at it; the next call refills.
        let edge = (READ_AHEAD_BYTES * 2 + 5) as u64;
        let tail = r.read(edge, 5000).await.unwrap();
        assert_eq!(tail.len(), 995, "cut at the end of the file");
        assert_eq!(
            h.hints.load(Ordering::Relaxed),
            2,
            "no hint at the end of the file"
        );
        assert!(r.read(total as u64, 10).await.unwrap().is_empty());
        assert!(r.read(0, 0).await.unwrap().is_empty());
        // Going back into an earlier window refetches it.
        assert_eq!(r.read(0, 3).await.unwrap(), data[..3]);
    }

    #[tokio::test]
    async fn the_media_reader_needs_a_known_size() {
        struct Unsized;
        #[async_trait::async_trait]
        impl ReadHandle for Unsized {
            fn size(&self) -> Option<u64> {
                None
            }
            async fn read_at(&self, _: u64, _: usize) -> Result<Vec<u8>> {
                Ok(Vec::new())
            }
        }
        let err = ReadAhead::new(Arc::new(Unsized)).err().unwrap();
        assert_eq!(err.kind, ErrorKind::Unsupported);
    }

    #[test]
    fn offsets_are_hexadecimal() {
        assert_eq!(parse_offset("1F40"), Some(0x1f40));
        assert_eq!(parse_offset(" 0x10 "), Some(16));
        assert_eq!(parse_offset("ff ff"), Some(0xffff));
        assert_eq!(parse_offset(""), None);
        assert_eq!(parse_offset("0x"), None);
        assert_eq!(parse_offset("xyz"), None);
        assert_eq!(parse_offset("-1"), None);
    }

    #[test]
    fn thumb_ids_parse_and_clamp() {
        let (u, s) = parse_thumb_id("lautta://nv-a/p/a%20b.jpg?size=128").unwrap();
        assert_eq!(u.to_string(), "lautta://nv-a/p/a%20b.jpg");
        assert_eq!(s, 128);
        assert_eq!(parse_thumb_id("lautta://nv-a/x.jpg?size=1").unwrap().1, MIN_THUMB);
        assert_eq!(
            parse_thumb_id("lautta://nv-a/x.jpg?size=99999").unwrap().1,
            MAX_THUMB
        );
        assert!(parse_thumb_id("lautta://nv-a/x.jpg").is_none());
        assert!(parse_thumb_id("lautta://nv-a/x.jpg?size=big").is_none());
        assert!(parse_thumb_id("http://x/y?size=3").is_none());
    }

    #[test]
    fn image_ids_accept_an_extra_percent_encoding() {
        let plain = parse_image_uri("lautta://user-pictures/a%20b.jpg").unwrap();
        let wrapped = parse_image_uri("lautta%3A%2F%2Fuser-pictures%2Fa%2520b.jpg").unwrap();
        assert_eq!(plain, wrapped);
        assert!(parse_image_uri("nonsense").is_none());
    }

    #[test]
    fn stamps_prefer_etags() {
        let a = Stamp {
            size: Some(1),
            mtime_ms: Some(1),
            etag: Some(b"x".to_vec()),
        };
        let mut b = a.clone();
        b.size = Some(2);
        assert!(!a.differs(&b), "equal etags win over size");
        b.etag = Some(b"y".to_vec());
        assert!(a.differs(&b));
        let c = Stamp {
            size: Some(1),
            mtime_ms: Some(1),
            etag: None,
        };
        assert!(!c.differs(&c.clone()));
        assert!(c.differs(&Stamp {
            mtime_ms: Some(2),
            ..c.clone()
        }));
    }
}
