// SPDX-License-Identifier: LGPL-2.1-or-later
//! Viewers area on `Core`: text, Markdown and EXIF sources, folder images, remote thumbnails and
//! images, media playback. Qt-free.

use crate::app::Core;
use crate::entry::{Entry, Kind};
use crate::error::{Error, ErrorKind, Result};
use crate::mime::{category_of, FileCategory};
use crate::org::recents::RecentKind;
use crate::preview::audio::{self, AudioRead, AudioTags, RangeReader};
use crate::preview::exif::{self, ExifInfo};
use crate::preview::highlight::{self, ColourJob, ColouredText};
use crate::preview::markdown;
use crate::preview::text::{self as ptext, TextDoc};
use crate::provider::{list_all, read_all, Lane, ProgressSink, Provider, ReadHandle, ReadOptions};
use crate::settings::Settings;
use crate::sort::sort_permutation;
use crate::thumbs::{
    remote_thumbnail, render_glow, render_thumbnail, thumb_bucket, thumb_key, thumbnail_source, ThumbJobs,
    ThumbSettings, ThumbSource, ThumbnailCache, THUMB_SIZES,
};
use crate::thumbs_pdf::PdfThumbs;
use crate::thumbs_text::{thumb_text, TEXT_THUMB_BYTES};
use crate::uri::Uri;
use crate::vpath::VPath;
use std::collections::{HashMap, VecDeque};
use std::os::fd::OwnedFd;
use std::path::PathBuf;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex, MutexGuard, OnceLock};
use tokio::runtime::Handle;
use tokio::sync::oneshot;

pub const MAX_FULL_IMAGE_BYTES: u64 = 100 * 1000 * 1000;

fn too_large_to_show(bytes: u64) -> bool {
    bytes > MAX_FULL_IMAGE_BYTES
}
pub const EXIF_HEAD_BYTES: u64 = 512 * 1024;
pub const MIN_THUMB: u32 = 16;
pub const MAX_THUMB: u32 = 1024;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LoadedText {
    pub doc: TextDoc,
    pub size: Option<u64>,
}

#[derive(Debug)]
pub struct ViewedText {
    pub size: Option<u64>,
    pub truncated: bool,
    pub valid_utf8: bool,
    /// Coloured when quick, else plain until `colouring` finishes.
    pub lines: ColouredText,
    pub colouring: Option<ColourJob>,
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
    pub info: Option<ExifInfo>,
}

/// Media player's track info. `cover`: where the player's picture comes from.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AudioInfo {
    pub tags: AudioTags,
    pub cover: CoverFrom,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CoverFrom {
    File,
    Folder,
    None,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ImageItem {
    pub uri: Uri,
    pub name: String,
    pub size: Option<u64>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FullImage {
    pub bytes: Arc<Vec<u8>>,
    /// 1-8; 1 = none.
    pub orientation: u8,
}

/// Server images kept after reading: the viewer's zoom and its neighbours ask again.
pub const SHOWN_IMAGES: usize = 3;
pub const SHOWN_IMAGES_BYTES: usize = 64 * 1000 * 1000;

#[derive(Default)]
pub struct ShownImages(Mutex<VecDeque<(Uri, Option<u64>, FullImage)>>);

impl ShownImages {
    fn get(&self, uri: &Uri, size: Option<u64>) -> Option<FullImage> {
        let list = lock(&self.0);
        list.iter()
            .find(|(u, s, _)| u == uri && *s == size)
            .map(|(_, _, img)| img.clone())
    }

    /// Newest first; oldest go past [`SHOWN_IMAGES`] or [`SHOWN_IMAGES_BYTES`].
    fn put(&self, uri: &Uri, size: Option<u64>, image: FullImage) {
        if image.bytes.len() > SHOWN_IMAGES_BYTES {
            return;
        }
        let mut list = lock(&self.0);
        list.retain(|(u, _, _)| u != uri);
        list.push_front((uri.clone(), size, image));
        list.truncate(SHOWN_IMAGES);
        while list.iter().map(|(_, _, i)| i.bytes.len()).sum::<usize>() > SHOWN_IMAGES_BYTES {
            list.pop_back();
        }
    }

    pub fn len(&self) -> usize {
        lock(&self.0).len()
    }

    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ThumbId {
    pub uri: Uri,
    pub size: u32,
    /// Only what the cache holds, any size: a stand-in while the real one loads.
    pub cached: bool,
}

/// `<uri>?size=N[&cached=1]`. Size is clamped. URI may arrive percent-encoded once more (QML
/// helpers).
pub fn parse_thumb_id(id: &str) -> Option<ThumbId> {
    let (uri_part, query) = id.rsplit_once('?')?;
    let (size, cached) = match query.split_once('&') {
        Some((size, "cached=1")) => (size, true),
        Some(_) => return None,
        None => (query, false),
    };
    let size: u32 = size.strip_prefix("size=")?.parse().ok()?;
    Some(ThumbId {
        uri: parse_image_uri(uri_part)?,
        size: size.clamp(MIN_THUMB, MAX_THUMB),
        cached,
    })
}

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

/// How far the background reader runs ahead of the player.
pub const READ_AHEAD_BYTES: usize = 4 * 1024 * 1024;
/// One background read; also the most a seek waits behind one already sent.
const PIECE_BYTES: usize = 512 * 1024;
/// Read on a miss (start, seek), before the player gets its bytes.
const FIRST_BYTES: usize = 256 * 1024;
/// Pieces kept, oldest dropped first: read-ahead plus some behind for small seeks back.
const KEEP_BYTES: usize = READ_AHEAD_BYTES + 2 * 1024 * 1024;

/// Player reads small pieces on Qt's GUI thread (Qt 5.6 `QGstAppSrc`); a background task keeps
/// [`READ_AHEAD_BYTES`] past the last read cached, so steady playback never waits on the provider.
pub struct ReadAhead {
    handle: Arc<dyn ReadHandle>,
    size: u64,
    shared: Arc<Shared>,
}

struct Shared {
    state: Mutex<Cache>,
    /// Sent on every change a reader or the fetch task may wait for.
    changed: tokio::sync::watch::Sender<()>,
}

#[derive(Default)]
struct Cache {
    pieces: VecDeque<(u64, Arc<Vec<u8>>)>,
    bytes: usize,
    /// End of the player's last read.
    pos: u64,
    /// Fetch task's generation: a task whose generation is gone stops.
    generation: u64,
    fetch: Option<Fetch>,
}

struct Fetch {
    /// Next offset the task reads; bytes before it from the task's start are cached.
    next: u64,
    failed: bool,
    task: tokio::task::JoinHandle<()>,
}

impl Cache {
    fn find(&self, offset: u64, max: usize) -> Option<Vec<u8>> {
        self.pieces.iter().find_map(|(start, data)| {
            let at = usize::try_from(offset.checked_sub(*start)?).ok()?;
            let rest = data.get(at..).filter(|r| !r.is_empty())?;
            Some(rest[..rest.len().min(max)].to_vec())
        })
    }

    fn insert(&mut self, offset: u64, data: Vec<u8>) {
        self.bytes += data.len();
        self.pieces.push_back((offset, Arc::new(data)));
        while self.bytes > KEEP_BYTES {
            match self.pieces.pop_front() {
                Some((_, old)) => self.bytes -= old.len(),
                None => break,
            }
        }
    }

    /// Fetch about to answer `offset` with the piece it reads now.
    fn fetching(&self, offset: u64) -> bool {
        self.fetch
            .as_ref()
            .is_some_and(|f| !f.failed && offset >= f.next && offset < f.next + PIECE_BYTES as u64)
    }

    fn stop_fetch(&mut self) {
        self.generation += 1;
        if let Some(f) = self.fetch.take() {
            f.task.abort();
        }
    }
}

impl ReadAhead {
    /// Size must be known (media streams have a length).
    pub fn new(handle: Arc<dyn ReadHandle>) -> Result<ReadAhead> {
        let size = handle
            .size()
            .ok_or_else(|| Error::new(ErrorKind::Unsupported, "size unknown"))?;
        Ok(ReadAhead {
            handle,
            size,
            shared: Arc::new(Shared {
                state: Mutex::new(Cache::default()),
                changed: tokio::sync::watch::channel(()).0,
            }),
        })
    }

    pub fn size(&self) -> u64 {
        self.size
    }

    /// Short only at the end of the file: Qt 5.6's `QGstAppSrc` pushes a short read as a full-size
    /// buffer, and the bytes after it land at the wrong offsets. Needs a tokio runtime.
    pub async fn read(&self, offset: u64, max: usize) -> Result<Vec<u8>> {
        let want = usize::try_from(self.size.saturating_sub(offset))
            .unwrap_or(usize::MAX)
            .min(max);
        let mut out = Vec::new();
        while out.len() < want {
            let part = self
                .read_piece(offset + out.len() as u64, want - out.len())
                .await?;
            if part.is_empty() {
                break;
            }
            out.extend_from_slice(&part);
        }
        Ok(out)
    }

    async fn read_piece(&self, offset: u64, max: usize) -> Result<Vec<u8>> {
        loop {
            // Subscribed before the check: a piece landing after it still wakes this read.
            let mut changed = self.shared.changed.subscribe();
            {
                let mut cache = lock(&self.shared.state);
                if let Some(hit) = cache.find(offset, max) {
                    cache.pos = offset + hit.len() as u64;
                    drop(cache);
                    self.shared.changed.send_replace(());
                    return Ok(hit);
                }
                if !cache.fetching(offset) {
                    cache.stop_fetch();
                    break;
                }
            }
            if changed.changed().await.is_err() {
                break;
            }
        }
        let want = usize::try_from(self.size - offset)
            .unwrap_or(usize::MAX)
            .min(FIRST_BYTES.max(max));
        let data = self.handle.read_at(offset, want).await?;
        if data.is_empty() {
            return Ok(data);
        }
        let end = offset + data.len() as u64;
        let first = data[..data.len().min(max)].to_vec();
        let mut cache = lock(&self.shared.state);
        cache.insert(offset, data);
        cache.pos = offset + first.len() as u64;
        if end < self.size {
            cache.stop_fetch();
            let generation = cache.generation;
            let task = tokio::spawn(fetch(
                self.handle.clone(),
                self.shared.clone(),
                generation,
                end,
                self.size,
            ));
            cache.fetch = Some(Fetch {
                next: end,
                failed: false,
                task,
            });
        }
        Ok(first)
    }
}

impl Drop for ReadAhead {
    fn drop(&mut self) {
        lock(&self.shared.state).stop_fetch();
    }
}

/// Reads [`PIECE_BYTES`] at a time while under [`READ_AHEAD_BYTES`] ahead of the player.
async fn fetch(handle: Arc<dyn ReadHandle>, shared: Arc<Shared>, generation: u64, mut next: u64, size: u64) {
    let mut changed = shared.changed.subscribe();
    let mut hinted = next;
    while next < size {
        changed.borrow_and_update();
        let ahead = {
            let cache = lock(&shared.state);
            if cache.generation != generation {
                return;
            }
            next >= cache.pos + READ_AHEAD_BYTES as u64
        };
        if ahead {
            if changed.changed().await.is_err() {
                return;
            }
            continue;
        }
        if next >= hinted {
            hinted = next + READ_AHEAD_BYTES as u64;
            // A failed hint only costs the provider's own prefetch.
            let _ = handle.read_ahead(next, READ_AHEAD_BYTES as u64).await;
        }
        let want = usize::try_from(size - next)
            .unwrap_or(usize::MAX)
            .min(PIECE_BYTES);
        let got = handle.read_at(next, want).await;
        let mut cache = lock(&shared.state);
        if cache.generation != generation {
            return;
        }
        let failed = match got {
            Ok(data) if !data.is_empty() => {
                let at = next;
                next += data.len() as u64;
                cache.insert(at, data);
                false
            }
            // Reader repeats the read itself and sees the error.
            _ => true,
        };
        if let Some(f) = cache.fetch.as_mut() {
            f.next = next;
            f.failed = failed;
        }
        drop(cache);
        shared.changed.send_replace(());
        if failed {
            return;
        }
    }
}

pub struct Previews {
    cache: ThumbnailCache,
    jobs: ThumbJobs,
    pdf: PdfThumbs,
    waiters: Mutex<HashMap<String, Waiting>>,
    next_waiter: AtomicU64,
    /// Cover of the audio file last read for the player, by [`version_key`]: its cover request
    /// then reads no file again.
    last_cover: Mutex<Option<(String, Arc<Vec<u8>>)>>,
}

/// What an image request makes. Covers serve the media player: any audio file, whatever the
/// thumbnail settings; glow is the blurred backdrop drawn from the cover.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PreviewKind {
    Thumb,
    Cover,
    Glow,
}

/// `job`: [`ThumbJobs`] key, new per start: a cancelled job keeps its slot until its task wakes,
/// maybe after the same thumbnail is asked for again.
struct Waiting {
    job: String,
    waiters: Vec<(u64, ThumbSender)>,
}

type ThumbResult = Result<Arc<Vec<u8>>>;
type ThumbSender = oneshot::Sender<ThumbResult>;

pub struct TextTicket {
    pub rx: oneshot::Receiver<Option<String>>,
    pub key: String,
}

/// Image sizes are 16 or more.
const TEXT_KEY_SIZE: u32 = 1;
/// Empty entry: the audio file holds no cover (until it changes).
const NO_COVER_KEY_SIZE: u32 = 2;
const GLOW_KEY_SIZE: u32 = 3;
/// Cover the glow is drawn from; glow itself is [`crate::thumbs::GLOW_SIDE`] square.
const GLOW_FROM: u32 = 128;
/// Largest folder picture (`cover.jpg`, …) read for a cover.
pub const MAX_FOLDER_COVER_BYTES: u64 = 16 * 1024 * 1024;

/// `cancel` (or the part kept after taking `rx`) drops the request when the delegate is gone.
pub struct ThumbTicket {
    pub rx: oneshot::Receiver<ThumbResult>,
    pub cancel: ThumbCancel,
}

impl ThumbTicket {
    pub fn cancel(&self) {
        self.cancel.cancel();
    }
}

/// Job is cancelled when nobody else waits for the same thumbnail.
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

    /// Requests for the same file and size share one job.
    pub fn request(self: &Arc<Self>, core: Arc<Core>, uri: Uri, size: u32) -> ThumbTicket {
        self.request_as(core, uri, size, false, PreviewKind::Thumb)
    }

    /// Cached thumbnail of any size, never the file: fails at once when none was made.
    pub fn request_cached(self: &Arc<Self>, core: Arc<Core>, uri: Uri, size: u32) -> ThumbTicket {
        self.request_as(core, uri, size, true, PreviewKind::Thumb)
    }

    /// Media player's cover or its glow; `size` ignored for glow.
    pub fn request_cover(
        self: &Arc<Self>,
        core: Arc<Core>,
        uri: Uri,
        size: u32,
        kind: PreviewKind,
    ) -> ThumbTicket {
        self.request_as(core, uri, size, false, kind)
    }

    fn request_as(
        self: &Arc<Self>,
        core: Arc<Core>,
        uri: Uri,
        size: u32,
        cached: bool,
        kind: PreviewKind,
    ) -> ThumbTicket {
        let size = thumb_bucket(size);
        let mut job_key = thumb_key(&uri.to_string(), size, None, None);
        if cached {
            job_key.push_str(":cached");
        }
        match kind {
            PreviewKind::Thumb => {}
            PreviewKind::Cover => job_key.push_str(":cover"),
            PreviewKind::Glow => job_key.push_str(":glow"),
        }
        let waiter = self.next_waiter.fetch_add(1, Ordering::Relaxed);
        let (tx, rx) = oneshot::channel();
        let mut waiting = lock(&self.waiters);
        if let Some(w) = waiting.get_mut(&job_key) {
            w.waiters.push((waiter, tx));
        } else {
            let job = format!("{job_key}#{waiter}");
            waiting.insert(
                job_key.clone(),
                Waiting {
                    job: job.clone(),
                    waiters: vec![(waiter, tx)],
                },
            );
            let me = Arc::clone(self);
            let key = job_key.clone();
            self.jobs.request(&job, &uri.location.clone(), async move {
                let result = me.make(&core, &uri, size, cached, kind).await;
                me.finish(&key, result);
            });
        }
        drop(waiting);
        ThumbTicket {
            rx,
            cancel: ThumbCancel {
                previews: Arc::clone(self),
                job_key,
                waiter,
            },
        }
    }

    /// Each request its own job, so cancelling one leaves the others.
    pub fn request_text(self: &Arc<Self>, core: Arc<Core>, uri: Uri) -> TextTicket {
        let n = self.next_waiter.fetch_add(1, Ordering::Relaxed);
        let key = format!("text:{n}:{uri}");
        let (tx, rx) = oneshot::channel();
        let me = Arc::clone(self);
        let location = uri.location.clone();
        self.jobs.request(&key, &location, async move {
            let _ = tx.send(me.make_text(&core, &uri).await);
        });
        TextTicket { rx, key }
    }

    pub fn cancel_text(&self, key: &str) {
        self.jobs.cancel(key);
    }

    async fn make_text(&self, core: &Core, uri: &Uri) -> Option<String> {
        let file = ThumbFile::open(core, uri, false).await.ok()?;
        let entry = &file.entry;
        let settings = ThumbSettings::new(&core.settings());
        if thumbnail_source(entry, file.local, true, &settings) != ThumbSource::Text {
            return None;
        }
        let key = thumb_key(
            &file.key_uri,
            TEXT_KEY_SIZE,
            entry.modified_ms(),
            entry.etag.as_deref(),
        );
        if let Some(hit) = self.cache.get(&key).filter(|_| !file.local) {
            return thumb_text(&hit);
        }
        let handle = file
            .provider
            .open_read(&file.path, Lane::Interactive)
            .await
            .ok()?;
        let head = read_all(handle.as_ref(), TEXT_THUMB_BYTES).await.ok()?;
        if !file.local {
            if let Err(e) = self.cache.put(&key, &head) {
                log::warn!("text thumbnail not cached: {e}");
            }
        }
        thumb_text(&head)
    }

    /// A cancelled job that still ended serves those waiting on its successor: same thumbnail.
    fn finish(&self, key: &str, result: ThumbResult) {
        let senders = lock(&self.waiters)
            .remove(key)
            .map(|w| w.waiters)
            .unwrap_or_default();
        for (_, tx) in senders {
            let _ = tx.send(result.clone());
        }
    }

    fn cancel(&self, key: &str, waiter: u64) {
        let job = {
            let mut waiting = lock(&self.waiters);
            let Some(w) = waiting.get_mut(key) else {
                return;
            };
            w.waiters.retain(|(id, _)| *id != waiter);
            if !w.waiters.is_empty() {
                return;
            }
            waiting.remove(key).map(|w| w.job)
        };
        if let Some(job) = job {
            self.jobs.cancel(&job);
        }
    }

    /// App makes only server files' thumbnails; local ones are the system thumbnailer's. `size` is
    /// one of [`THUMB_SIZES`].
    async fn make(&self, core: &Core, uri: &Uri, size: u32, cached: bool, kind: PreviewKind) -> ThumbResult {
        let file = ThumbFile::open(core, uri, cached).await?;
        let entry = &file.entry;
        if entry.is_dir() {
            return Err(Error::kind(ErrorKind::IsADirectory));
        }
        let audio = category_of(entry) == FileCategory::Audio;
        // Read when asked: a View options change holds from the next thumbnail on.
        let settings = ThumbSettings::new(&core.settings());
        if kind != PreviewKind::Thumb {
            if !audio {
                return Err(Error::new(ErrorKind::Unsupported, "not an audio file"));
            }
        } else if thumbnail_source(entry, file.local, true, &settings) != ThumbSource::App {
            return Err(Error::new(
                ErrorKind::Unsupported,
                "no app thumbnail for this file",
            ));
        }
        if kind == PreviewKind::Glow {
            return self.glow(core, &file).await;
        }
        let pdf = category_of(entry) == FileCategory::Pdf;
        let version = thumb_key(&file.key_uri, 0, entry.modified_ms(), entry.etag.as_deref());
        if pdf && self.pdf.is_refused(&version) {
            return Err(Error::new(ErrorKind::Unsupported, "PDF cannot be drawn"));
        }
        let key_of = |s: u32| thumb_key(&file.key_uri, s, entry.modified_ms(), entry.etag.as_deref());
        let key = key_of(size);
        if let Some(hit) = self.cache.get(&key) {
            return Ok(Arc::new(hit));
        }
        let larger = THUMB_SIZES.iter().filter(|s| **s > size);
        let bytes = if let Some(hit) = larger.filter_map(|s| self.cache.get(&key_of(*s))).next() {
            // Cached pixels are upright already.
            tokio::task::spawn_blocking(move || render_thumbnail(&hit, size, 1))
                .await
                .map_err(|e| Error::new(ErrorKind::Internal, format!("thumbnail worker failed: {e}")))??
        } else if cached {
            let mut smaller = THUMB_SIZES.iter().rev().filter(|s| **s < size);
            return smaller
                .find_map(|s| self.cache.get(&key_of(*s)))
                .map(Arc::new)
                .ok_or_else(|| Error::new(ErrorKind::NotFound, "no thumbnail made yet"));
        } else if pdf && core.power_save() {
            return Err(Error::new(
                ErrorKind::Unsupported,
                "battery saving: server PDFs not read",
            ));
        } else if audio && kind == PreviewKind::Thumb && !file.local && core.power_save() {
            return Err(Error::new(
                ErrorKind::Unsupported,
                "battery saving: server audio not read",
            ));
        } else if audio {
            let (bytes, own) = self.audio_cover(core, &file, size).await?;
            // A folder picture is kept under its own key: one change to it shows in every track.
            if own {
                self.put(&key, &bytes);
            }
            return Ok(Arc::new(bytes));
        } else {
            let handle = file.provider.open_read(&file.path, Lane::Interactive).await?;
            if pdf {
                let (dir, limit) = (self.cache.dir(), settings.pdf_limit());
                self.pdf.make(dir, &version, handle.as_ref(), size, limit).await?
            } else {
                // Battery saving: embedded EXIF thumbnail only, no whole file.
                let max = if core.power_save() {
                    0
                } else {
                    settings.max_remote_bytes
                };
                remote_thumbnail(handle.as_ref(), size, max).await?
            }
        };
        self.put(&key, &bytes);
        Ok(Arc::new(bytes))
    }

    fn put(&self, key: &str, bytes: &[u8]) {
        if let Err(e) = self.cache.put(key, bytes) {
            log::warn!("thumbnail not cached: {e}");
        }
    }

    fn key_of(file: &ThumbFile, size: u32) -> String {
        thumb_key(
            &file.key_uri,
            size,
            file.entry.modified_ms(),
            file.entry.etag.as_deref(),
        )
    }

    /// Rendered cover at `size`: the file's own (`true`), else its folder's picture.
    async fn audio_cover(&self, core: &Core, file: &ThumbFile, size: u32) -> Result<(Vec<u8>, bool)> {
        let none_key = Self::key_of(file, NO_COVER_KEY_SIZE);
        if !self.cache.contains(&none_key) {
            let version = Self::key_of(file, 0);
            let memo = lock(&self.last_cover)
                .as_ref()
                .filter(|(k, _)| *k == version)
                .map(|(_, b)| Arc::clone(b));
            let embedded = match memo {
                Some(bytes) => Some(bytes),
                None => read_cover(file).await?.map(Arc::new),
            };
            if let Some(bytes) = embedded {
                let rendered = spawn_render(move || render_thumbnail(&bytes, size, 1)).await;
                if let Ok(done) = rendered {
                    return Ok((done, true));
                }
            }
            self.put(&none_key, &[]);
        }
        match self.folder_cover(core, file, size).await {
            Some(bytes) => Ok((bytes, false)),
            None => Err(Error::new(ErrorKind::NotFound, "no cover")),
        }
    }

    /// `cover.jpg`, `folder.jpg` or `front.jpg` beside the file, rendered at `size` and kept by its
    /// own name and change time. None when missing, too large or not an image.
    async fn folder_cover(&self, core: &Core, file: &ThumbFile, size: u32) -> Option<Vec<u8>> {
        let folder = Uri::parse(&file.key_uri).ok()?.parent()?;
        let entries = match core.dircache.get(&folder) {
            Ok(Some(hit)) => hit.entries,
            _ => list_all(file.provider.as_ref(), &folder.path, Lane::Interactive)
                .await
                .ok()?,
        };
        let picture = audio::folder_cover(&entries)?.clone();
        let uri = folder.join(&picture.name).ok()?;
        let key = thumb_key(
            &uri.to_string(),
            size,
            picture.modified_ms(),
            picture.etag.as_deref(),
        );
        if let Some(hit) = self.cache.get(&key) {
            return Some(hit);
        }
        if picture.size.is_some_and(|s| s > MAX_FOLDER_COVER_BYTES) {
            return None;
        }
        let handle = file.provider.open_read(&uri.path, Lane::Interactive).await.ok()?;
        let bytes = read_all(handle.as_ref(), MAX_FOLDER_COVER_BYTES + 1).await.ok()?;
        if bytes.len() as u64 > MAX_FOLDER_COVER_BYTES {
            return None;
        }
        let rendered = spawn_render(move || render_thumbnail(&bytes, size, exif::orientation(&bytes)))
            .await
            .ok()?;
        self.put(&key, &rendered);
        Some(rendered)
    }

    async fn glow(&self, core: &Core, file: &ThumbFile) -> ThumbResult {
        let key = Self::key_of(file, GLOW_KEY_SIZE);
        if let Some(hit) = self.cache.get(&key) {
            return Ok(Arc::new(hit));
        }
        let base_key = Self::key_of(file, GLOW_FROM);
        let base = match self.cache.get(&base_key) {
            Some(hit) => hit,
            None => self.audio_cover(core, file, GLOW_FROM).await?.0,
        };
        let glow = spawn_render(move || render_glow(&base)).await?;
        self.put(&key, &glow);
        Ok(Arc::new(glow))
    }

    /// Player read the file: its cover serves the next cover request.
    fn remember_cover(&self, version: String, cover: Option<Vec<u8>>) {
        *lock(&self.last_cover) = cover.map(|c| (version, Arc::new(c)));
    }
}

async fn spawn_render<T: Send + 'static>(work: impl FnOnce() -> Result<T> + Send + 'static) -> Result<T> {
    tokio::task::spawn_blocking(work)
        .await
        .map_err(|e| Error::new(ErrorKind::Internal, format!("thumbnail worker failed: {e}")))?
}

/// Tags (and cover when `want_cover`) of an audio file; only the parts the parser reads cross the
/// network ([`audio::RangeReader`]). `None`: not readable as audio.
async fn read_audio_of(handle: Arc<dyn ReadHandle>, want_cover: bool) -> Result<Option<AudioRead>> {
    let Some(size) = handle.size() else {
        return Ok(None);
    };
    let rt = Handle::current();
    tokio::task::spawn_blocking(move || {
        let fetch = |offset: u64, max: usize| {
            rt.block_on(handle.read_at(offset, max))
                .map_err(|e| std::io::Error::other(e.message))
        };
        audio::read_audio(RangeReader::new(size, audio::READ_BUDGET, fetch), want_cover)
    })
    .await
    .map_err(|e| Error::new(ErrorKind::Internal, format!("audio reader failed: {e}")))
}

async fn read_cover(file: &ThumbFile) -> Result<Option<Vec<u8>>> {
    let handle = file.provider.open_read(&file.path, Lane::Interactive).await?;
    Ok(read_audio_of(Arc::from(handle), true)
        .await?
        .and_then(|r| r.cover))
}

struct ThumbFile {
    provider: Arc<dyn Provider>,
    path: VPath,
    entry: Entry,
    local: bool,
    key_uri: String,
}

impl ThumbFile {
    /// `listed`: entry from the cached folder listing when there is one, so a viewer's placeholder
    /// waits on no server round trip (those queue behind the full image's reads).
    async fn open(core: &Core, uri: &Uri, listed: bool) -> Result<ThumbFile> {
        let provider = core.provider(&uri.location)?;
        let entry = match listed.then(|| listed_file(core, uri)).flatten() {
            Some(entry) => entry,
            None => provider.stat(&uri.path, true, Lane::Interactive).await?,
        };
        Ok(ThumbFile {
            provider,
            path: uri.path.clone(),
            entry,
            local: core.locations.to_local_path(uri).is_some(),
            key_uri: uri.to_string(),
        })
    }
}

fn listed_file(core: &Core, uri: &Uri) -> Option<Entry> {
    let listing = core.dircache.get(&uri.parent()?).ok()??;
    let name = uri.name()?;
    listing
        .entries
        .into_iter()
        .find(|e| e.name == name && e.kind == Kind::File)
}

type PreviewsByDir = Mutex<Vec<(PathBuf, Arc<Previews>)>>;

static PREVIEWS: OnceLock<PreviewsByDir> = OnceLock::new();

impl Core {
    pub async fn open_reader(&self, uri: &Uri) -> Result<Arc<dyn ReadHandle>> {
        let provider = self.provider(&uri.location)?;
        let handle = provider.open_read(&uri.path, Lane::Interactive).await?;
        Ok(Arc::from(handle))
    }

    /// Failures do not matter to the viewer.
    pub fn note_viewed(&self, uri: &Uri, kind: RecentKind) {
        if let Err(e) = self.recents.record(uri, &display(uri), kind) {
            log::debug!("recents not updated: {e}");
        }
    }

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
        })
    }

    /// Colours come with the text if ready within [`highlight::FIRST_WAIT`], else text is plain and
    /// colouring goes on.
    pub async fn view_text(&self, uri: &Uri) -> Result<ViewedText> {
        let loaded = self.load_text(uri).await?;
        let TextDoc {
            text,
            truncated,
            valid_utf8,
        } = loaded.doc;
        let name = uri.path.name().unwrap_or_default().to_vec();
        let job = highlight::start(name, text.clone());
        let (lines, colouring) = match job.wait(highlight::FIRST_WAIT).await {
            Ok(Some(lines)) => (lines, None),
            Ok(None) => (ColouredText::plain(text), None),
            Err(job) => (ColouredText::plain(text), Some(job)),
        };
        Ok(ViewedText {
            size: loaded.size,
            truncated,
            valid_utf8,
            lines,
            colouring,
        })
    }

    /// Only the first MiB is rendered.
    pub async fn render_markdown(&self, uri: &Uri) -> Result<RenderedMarkdown> {
        let loaded = self.load_text(uri).await?;
        Ok(RenderedMarkdown {
            html: markdown::render(&loaded.doc.text),
            truncated: loaded.doc.truncated,
        })
    }

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

    /// Tags of an audio file and where its cover comes from. A file lofty can't read has empty
    /// tags, not an error: the player then shows its name.
    pub async fn audio_info(&self, uri: &Uri) -> Result<AudioInfo> {
        let provider = self.provider(&uri.location)?;
        let entry = provider.stat(&uri.path, true, Lane::Interactive).await?;
        let handle = provider.open_read(&uri.path, Lane::Interactive).await?;
        let read = read_audio_of(Arc::from(handle), true).await?.unwrap_or_default();
        let has_cover = read.cover.is_some();
        let previews = self.previews(&Handle::current())?;
        let version = thumb_key(&uri.to_string(), 0, entry.modified_ms(), entry.etag.as_deref());
        previews.remember_cover(version, read.cover);
        let cover = if has_cover {
            CoverFrom::File
        } else if self.folder_has_cover(uri, provider.as_ref()).await {
            CoverFrom::Folder
        } else {
            CoverFrom::None
        };
        Ok(AudioInfo {
            tags: read.tags,
            cover,
        })
    }

    async fn folder_has_cover(&self, uri: &Uri, provider: &dyn Provider) -> bool {
        let Some(folder) = uri.parent() else {
            return false;
        };
        let entries = match self.dircache.get(&folder) {
            Ok(Some(hit)) => hit.entries,
            _ => match list_all(provider, &folder.path, Lane::Interactive).await {
                Ok(all) => all,
                Err(_) => return false,
            },
        };
        audio::folder_cover(&entries).is_some()
    }

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

    /// Kept in [`ShownImages`] while its file keeps its size.
    pub async fn full_image(&self, uri: &Uri) -> Result<FullImage> {
        let provider = self.provider(&uri.location)?;
        let handle = provider.open_read(&uri.path, Lane::Interactive).await?;
        let size = handle.size();
        if size.is_some_and(too_large_to_show) {
            return Err(Error::new(ErrorKind::Unsupported, "image too large to show"));
        }
        if let Some(hit) = self.shown_images.get(uri, size) {
            return Ok(hit);
        }
        let bytes = crate::provider::read_all(handle.as_ref(), MAX_FULL_IMAGE_BYTES + 1).await?;
        if too_large_to_show(bytes.len() as u64) {
            return Err(Error::new(ErrorKind::Unsupported, "image too large to show"));
        }
        let orientation = exif::orientation(&bytes);
        let image = FullImage {
            bytes: Arc::new(bytes),
            orientation,
        };
        if size.is_some() {
            self.shown_images.put(uri, size, image.clone());
        }
        Ok(image)
    }

    pub fn shown_images(&self) -> &ShownImages {
        &self.shown_images
    }

    /// Viewer closed: kept images go.
    pub fn forget_shown_images(&self) {
        lock(&self.shown_images.0).clear();
    }

    /// Open cache takes the new budget, trimmed off the caller's thread; one opened later reads the
    /// setting itself.
    pub(crate) fn follow_thumbnail_budget(&self, settings: &Settings) {
        let dir = self.paths.thumbs_dir();
        let list = lock(PREVIEWS.get_or_init(Mutex::default));
        if let Some((_, previews)) = list.iter().find(|(d, _)| *d == dir) {
            let max = settings.thumbnail_cache_bytes();
            let me = Arc::clone(previews);
            previews
                .jobs
                .runtime()
                .spawn_blocking(move || me.cache.set_max_bytes(max));
        }
    }

    pub fn previews(&self, rt: &Handle) -> Result<Arc<Previews>> {
        let dir = self.paths.thumbs_dir();
        let mut list = lock(PREVIEWS.get_or_init(Mutex::default));
        if let Some((_, p)) = list.iter().find(|(d, _)| *d == dir) {
            return Ok(Arc::clone(p));
        }
        let max = self.settings().thumbnail_cache_bytes();
        let previews = Arc::new(Previews {
            cache: ThumbnailCache::new(&dir, max)?,
            jobs: ThumbJobs::new(rt.clone()),
            pdf: PdfThumbs::default(),
            waiters: Mutex::default(),
            next_waiter: AtomicU64::new(1),
            last_cover: Mutex::default(),
        });
        list.push((dir, Arc::clone(&previews)));
        Ok(previews)
    }

    /// Fallback: copies a media file into the private cache so it plays from disk.
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

    pub fn clear_playback_cache(&self) {
        let _ = std::fs::remove_dir_all(self.paths.cache_dir().join("media"));
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Logs each `read_at` when called; each read then waits for one permit of `gate`.
    struct Gated {
        data: Vec<u8>,
        calls: std::sync::Mutex<Vec<(u64, usize)>>,
        called: tokio::sync::watch::Sender<usize>,
        gate: tokio::sync::Semaphore,
        hints: std::sync::Mutex<Vec<(u64, u64)>>,
        fail_at: std::sync::Mutex<Option<u64>>,
        /// Claimed size beyond the data: a file that shrank since it was opened.
        missing: std::sync::atomic::AtomicU64,
    }

    impl Gated {
        fn new(total: usize, permits: usize) -> Arc<Gated> {
            Arc::new(Gated {
                data: (0..total).map(|i| (i % 251) as u8).collect(),
                calls: Default::default(),
                called: tokio::sync::watch::channel(0).0,
                gate: tokio::sync::Semaphore::new(permits),
                hints: Default::default(),
                fail_at: Default::default(),
                missing: Default::default(),
            })
        }

        fn calls(&self) -> Vec<(u64, usize)> {
            self.calls.lock().unwrap().clone()
        }

        /// Hang guard only: work of milliseconds.
        async fn until(&self, what: impl Fn(&[(u64, usize)]) -> bool) {
            let mut rx = self.called.subscribe();
            let wait = async {
                while !what(&self.calls()) {
                    rx.changed().await.unwrap();
                }
            };
            tokio::time::timeout(std::time::Duration::from_secs(10), wait)
                .await
                .expect("reads never got there");
        }
    }

    #[async_trait::async_trait]
    impl ReadHandle for Gated {
        fn size(&self) -> Option<u64> {
            Some(self.data.len() as u64 + self.missing.load(Ordering::Relaxed))
        }
        async fn read_at(&self, offset: u64, max: usize) -> Result<Vec<u8>> {
            self.calls.lock().unwrap().push((offset, max));
            self.called.send_modify(|n| *n += 1);
            self.gate.acquire().await.unwrap().forget();
            if self.fail_at.lock().unwrap().take_if(|at| *at == offset).is_some() {
                return Err(Error::new(ErrorKind::ConnectionLost, "gone"));
            }
            let start = usize::try_from(offset).unwrap_or(usize::MAX).min(self.data.len());
            Ok(self.data[start..(start + max).min(self.data.len())].to_vec())
        }
        async fn read_ahead(&self, offset: u64, bytes: u64) -> Result<()> {
            self.hints.lock().unwrap().push((offset, bytes));
            Ok(())
        }
    }

    const OPEN: usize = tokio::sync::Semaphore::MAX_PERMITS;

    fn reach(calls: &[(u64, usize)]) -> u64 {
        calls.iter().map(|(at, len)| at + *len as u64).max().unwrap_or(0)
    }

    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    async fn playing_on_reads_only_what_the_background_fetch_already_has() {
        let h = Gated::new(12 << 20, OPEN);
        let r = ReadAhead::new(h.clone()).unwrap();
        assert_eq!(r.read(0, 4096).await.unwrap(), h.data[..4096]);
        let mut pos = 4096u64;
        while pos < 3 << 20 {
            h.until(|c| reach(c) >= (pos + (4 << 20)).min(12 << 20)).await;
            let got = r.read(pos, 23_000).await.unwrap();
            assert_eq!(got, h.data[pos as usize..pos as usize + 23_000]);
            pos += 23_000;
        }
        let calls = h.calls();
        assert_eq!(
            calls[0],
            (0, 256 << 10),
            "player waits only for a short first read"
        );
        let mut next = 256 << 10;
        for &(at, len) in &calls[1..] {
            assert_eq!(
                (at, len),
                (next, 512 << 10),
                "rest in pieces, in order, in the background"
            );
            next += len as u64;
        }
        // Nothing to wait for: the fetch must stay idle once far enough ahead.
        tokio::time::sleep(std::time::Duration::from_millis(200)).await;
        assert!(
            reach(&h.calls()) <= pos + (4 << 20) + (512 << 10),
            "fetch stops 4 MiB ahead of the player"
        );
        let hints = h.hints.lock().unwrap().clone();
        assert_eq!(
            hints,
            [(256 << 10, 4 << 20), ((256 << 10) + (4 << 20), 4 << 20)],
            "one hint per 4 MiB"
        );
    }

    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    async fn a_read_the_fetch_is_about_to_answer_waits_for_it() {
        let h = Gated::new(4 << 20, 1);
        let r = Arc::new(ReadAhead::new(h.clone()).unwrap());
        r.read(0, 4096).await.unwrap();
        h.until(|c| c.len() == 2).await;
        let at = FIRST_BYTES as u64 + 10;
        let reader = tokio::spawn({
            let r = r.clone();
            async move { r.read(at, 100).await.unwrap() }
        });
        h.gate.add_permits(1);
        let got = tokio::time::timeout(std::time::Duration::from_secs(10), reader)
            .await
            .expect("read waits for the fetch's piece")
            .unwrap();
        assert_eq!(got, h.data[at as usize..at as usize + 100]);
        assert_eq!(
            h.calls()[1],
            (FIRST_BYTES as u64, PIECE_BYTES),
            "no second read of it"
        );
    }

    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    async fn a_seek_reads_there_first_and_fetches_on_from_there() {
        let h = Gated::new(20 << 20, OPEN);
        let r = ReadAhead::new(h.clone()).unwrap();
        r.read(0, 4096).await.unwrap();
        h.until(|c| c.len() >= 2).await;
        let at = 6u64 << 20;
        assert_eq!(
            r.read(at, 4096).await.unwrap(),
            h.data[at as usize..at as usize + 4096]
        );
        assert!(h.calls().contains(&(at, FIRST_BYTES)));
        let ahead = at + 4096 + READ_AHEAD_BYTES as u64;
        h.until(|c| reach(c) >= ahead).await;
        // Nothing to wait for: the fetch must stay idle once far enough ahead of the seek.
        tokio::time::sleep(std::time::Duration::from_millis(200)).await;
        assert!(reach(&h.calls()) <= ahead + PIECE_BYTES as u64);
    }

    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    async fn a_piece_a_stopped_fetch_gets_late_is_not_kept() {
        let h = Gated::new(4 << 20, 1);
        let r = ReadAhead::new(h.clone()).unwrap();
        r.read(0, 4096).await.unwrap();
        h.until(|c| c.len() == 2).await;
        // Fetch past its read and waiting for the lock when stopped: abort cannot reach it.
        let f = lock(&r.shared.state).fetch.take().unwrap();
        lock(&r.shared.state).stop_fetch();
        h.gate.add_permits(1);
        f.task.await.unwrap();
        assert!(lock(&r.shared.state).find(FIRST_BYTES as u64, 1).is_none());
    }

    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    async fn a_fetch_stops_at_an_early_end_of_file() {
        let h = Gated::new(1 << 20, OPEN);
        h.missing.store(2 << 20, Ordering::Relaxed);
        let r = ReadAhead::new(h.clone()).unwrap();
        r.read(0, 10).await.unwrap();
        h.until(|c| c.iter().any(|c| c.0 == 1 << 20)).await;
        // Nothing to wait for: an empty read ends the fetch.
        tokio::time::sleep(std::time::Duration::from_millis(200)).await;
        assert_eq!(h.calls().iter().filter(|c| c.0 == 1 << 20).count(), 1);
        assert!(r.read(1 << 20, 10).await.unwrap().is_empty(), "file ends early");
    }

    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    async fn a_skip_past_the_piece_on_its_way_reads_at_once() {
        let h = Gated::new(4 << 20, 1);
        let r = Arc::new(ReadAhead::new(h.clone()).unwrap());
        r.read(0, 4096).await.unwrap();
        h.until(|c| c.len() == 2).await;
        let at = (FIRST_BYTES + PIECE_BYTES) as u64 + 10;
        let reader = tokio::spawn({
            let r = r.clone();
            async move { r.read(at, 100).await.unwrap() }
        });
        h.until(|c| c.contains(&(at, FIRST_BYTES))).await;
        h.gate.add_permits(OPEN - 1);
        assert_eq!(reader.await.unwrap(), h.data[at as usize..at as usize + 100]);
    }

    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    async fn played_bytes_are_dropped_once_6_mib_are_cached() {
        let h = Gated::new(12 << 20, OPEN);
        let r = ReadAhead::new(h.clone()).unwrap();
        r.read(0, 10).await.unwrap();
        // Player 4 MiB behind `upto`: fetch stops at the first piece end past it.
        let fill = |upto: u64| {
            let (h, r) = (h.clone(), &r);
            async move {
                h.until(|c| reach(c) >= 4 << 20).await;
                r.read(upto - (4 << 20), 10).await.unwrap();
                h.until(|c| reach(c) >= upto).await;
            }
        };
        fill(5 << 20).await;
        r.read(0, 10).await.unwrap();
        assert_eq!(
            h.calls().iter().filter(|c| c.0 == 0).count(),
            1,
            "5.25 MiB cached: kept"
        );
        fill(7 << 20).await;
        let second = FIRST_BYTES as u64 + 10;
        r.read(second, 10).await.unwrap();
        assert!(
            h.calls().contains(&(second, FIRST_BYTES)),
            "7.25 MiB cached: oldest 1.25 MiB dropped"
        );
    }

    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    async fn a_failed_fetch_leaves_the_read_to_the_player() {
        let h = Gated::new(4 << 20, OPEN);
        *h.fail_at.lock().unwrap() = Some(FIRST_BYTES as u64);
        let r = ReadAhead::new(h.clone()).unwrap();
        r.read(0, 4096).await.unwrap();
        let at = FIRST_BYTES as u64;
        let got = tokio::time::timeout(std::time::Duration::from_secs(10), r.read(at, 10))
            .await
            .expect("no wait on a fetch that failed")
            .unwrap();
        assert_eq!(got, h.data[at as usize..at as usize + 10]);
        assert_eq!(
            h.calls().iter().filter(|c| c.0 == at).count(),
            2,
            "fetch's read, then the player's"
        );
    }

    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    async fn dropping_the_reader_stops_its_fetch() {
        let h = Gated::new(12 << 20, 1);
        let r = ReadAhead::new(h.clone()).unwrap();
        r.read(0, 4096).await.unwrap();
        h.until(|c| c.len() == 2).await;
        drop(r);
        h.gate.add_permits(OPEN - 1);
        // Nothing to wait for: a stopped fetch reads nothing more.
        tokio::time::sleep(std::time::Duration::from_millis(200)).await;
        assert_eq!(h.calls().len(), 2);
    }

    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    async fn a_media_read_comes_back_whole_and_short_only_at_the_end() {
        let total = 3 << 20;
        let h = Gated::new(total, OPEN);
        let r = ReadAhead::new(h.clone()).unwrap();
        assert_eq!(r.size(), total as u64);
        let big = r.read(5, (2 << 20) + 3).await.unwrap();
        assert_eq!(big, h.data[5..(2 << 20) + 8], "longer than any piece");
        h.until(|c| reach(c) == total as u64).await;
        let tail = r.read(total as u64 - 7, 100).await.unwrap();
        assert_eq!(tail, h.data[total - 7..], "short at the end only");
        assert!(r.read(total as u64, 10).await.unwrap().is_empty());
        assert!(r.read(0, 0).await.unwrap().is_empty());
        assert_eq!(r.read(0, 3).await.unwrap(), h.data[..3]);
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
    fn thumb_ids_parse_and_clamp() {
        let t = parse_thumb_id("lautta://nv-a/p/a%20b.jpg?size=128").unwrap();
        assert_eq!(t.uri.to_string(), "lautta://nv-a/p/a%20b.jpg");
        assert_eq!((t.size, t.cached), (128, false));
        assert_eq!(
            parse_thumb_id("lautta://nv-a/x.jpg?size=1").unwrap().size,
            MIN_THUMB
        );
        assert_eq!(
            parse_thumb_id("lautta://nv-a/x.jpg?size=99999").unwrap().size,
            MAX_THUMB
        );
        assert!(parse_thumb_id("lautta://nv-a/x.jpg").is_none());
        assert!(parse_thumb_id("lautta://nv-a/x.jpg?size=big").is_none());
        assert!(parse_thumb_id("http://x/y?size=3").is_none());
    }

    #[test]
    fn images_up_to_the_limit_are_shown() {
        assert!(!too_large_to_show(MAX_FULL_IMAGE_BYTES));
        assert!(too_large_to_show(MAX_FULL_IMAGE_BYTES + 1));
    }

    #[test]
    fn shown_images_stay_within_their_byte_budget() {
        let shown = ShownImages::default();
        let third = SHOWN_IMAGES_BYTES / 3 + 1;
        let img = |n: usize| FullImage {
            bytes: Arc::new(vec![0; n]),
            orientation: 1,
        };
        let u = |s: &str| Uri::parse(&format!("lautta://nv-a/{s}")).unwrap();
        assert!(shown.is_empty());
        shown.put(&u("photo"), Some(1), img(20_000_000));
        assert!(!shown.is_empty());
        assert_eq!(shown.len(), 1, "a 20 MB photo is kept");
        shown.put(&u("big"), Some(1), img(SHOWN_IMAGES_BYTES + 1));
        assert!(
            shown.get(&u("big"), Some(1)).is_none(),
            "larger than the budget: not kept"
        );
        shown.0.lock().unwrap().clear();
        shown.put(&u("exact"), Some(1), img(SHOWN_IMAGES_BYTES));
        assert_eq!(shown.len(), 1, "exactly the budget fits");
        for name in ["a", "b", "c"] {
            shown.put(&u(name), Some(1), img(third));
        }
        assert_eq!(shown.len(), 2, "third one would pass the budget");
        assert!(shown.get(&u("a"), Some(1)).is_none(), "oldest went");
        assert!(shown.get(&u("c"), Some(1)).is_some());
        assert!(
            shown.get(&u("c"), Some(2)).is_none(),
            "other size is another file"
        );
        shown.put(&u("b"), Some(1), img(1));
        assert_eq!(shown.len(), 2, "same image replaced, not doubled");
    }

    #[test]
    fn thumb_ids_may_ask_for_the_cache_only() {
        let t = parse_thumb_id("lautta://nv-a/x.jpg?size=1024&cached=1").unwrap();
        assert_eq!((t.size, t.cached), (1024, true));
        assert!(parse_thumb_id("lautta://nv-a/x.jpg?size=64&cached=0").is_none());
        assert!(parse_thumb_id("lautta://nv-a/x.jpg?size=64&other=1").is_none());
        assert!(parse_thumb_id("lautta://nv-a/x.jpg?cached=1&size=64").is_none());
    }

    #[test]
    fn image_ids_accept_an_extra_percent_encoding() {
        let plain = parse_image_uri("lautta://user-pictures/a%20b.jpg").unwrap();
        let wrapped = parse_image_uri("lautta%3A%2F%2Fuser-pictures%2Fa%2520b.jpg").unwrap();
        assert_eq!(plain, wrapped);
        assert!(parse_image_uri("nonsense").is_none());
    }
}
