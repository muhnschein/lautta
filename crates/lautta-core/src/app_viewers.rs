// SPDX-License-Identifier: LGPL-2.1-or-later
//! Viewers area on `Core`: text, Markdown and EXIF sources, folder images, remote thumbnails and
//! images, media playback. Qt-free.

use crate::app::Core;
use crate::entry::Entry;
use crate::error::{Error, ErrorKind, Result};
use crate::mime::{category_of, FileCategory};
use crate::org::recents::RecentKind;
use crate::preview::exif::{self, ExifInfo};
use crate::preview::highlight::{self, ColourJob, ColouredText};
use crate::preview::markdown;
use crate::preview::text::{self as ptext, TextDoc};
use crate::provider::{list_all, read_all, Lane, ProgressSink, Provider, ReadHandle, ReadOptions};
use crate::settings::Settings;
use crate::sort::sort_permutation;
use crate::thumbs::{
    remote_thumbnail, thumb_key, thumbnail_source, ThumbJobs, ThumbSettings, ThumbSource, ThumbnailCache,
};
use crate::thumbs_pdf::PdfThumbs;
use crate::thumbs_text::{thumb_text, TEXT_THUMB_BYTES};
use crate::uri::Uri;
use crate::vpath::VPath;
use std::collections::HashMap;
use std::os::fd::OwnedFd;
use std::path::PathBuf;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex, MutexGuard, OnceLock};
use tokio::runtime::Handle;
use tokio::sync::oneshot;

pub const MAX_FULL_IMAGE_BYTES: u64 = 100 * 1000 * 1000;
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

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ImageItem {
    pub uri: Uri,
    pub name: String,
    pub size: Option<u64>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FullImage {
    pub bytes: Vec<u8>,
    /// 1-8; 1 = none.
    pub orientation: u8,
}

/// Size is clamped. URI may arrive percent-encoded once more (QML helpers).
pub fn parse_thumb_id(id: &str) -> Option<(Uri, u32)> {
    let (uri_part, query) = id.rsplit_once('?')?;
    let size: u32 = query.strip_prefix("size=")?.parse().ok()?;
    Some((parse_image_uri(uri_part)?, size.clamp(MIN_THUMB, MAX_THUMB)))
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

pub const READ_AHEAD_BYTES: usize = 2 * 1024 * 1024;

/// Player asks small pieces, provider large ones; next window is announced with `read_ahead` so a
/// bridge can prefetch.
pub struct ReadAhead {
    handle: Arc<dyn ReadHandle>,
    size: u64,
    window: Mutex<(u64, Arc<Vec<u8>>)>,
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
            window: Mutex::new((0, Arc::default())),
        })
    }

    pub fn size(&self) -> u64 {
        self.size
    }

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

pub struct Previews {
    cache: ThumbnailCache,
    jobs: ThumbJobs,
    pdf: PdfThumbs,
    waiters: Mutex<HashMap<String, Vec<(u64, ThumbSender)>>>,
    next_waiter: AtomicU64,
}

type ThumbResult = Result<Arc<Vec<u8>>>;
type ThumbSender = oneshot::Sender<ThumbResult>;

pub struct TextTicket {
    pub rx: oneshot::Receiver<Option<String>>,
    pub key: String,
}

/// Image sizes are 16 or more.
const TEXT_KEY_SIZE: u32 = 1;

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
        let file = ThumbFile::open(core, uri).await.ok()?;
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

    /// App makes only server files' thumbnails; local ones are the system thumbnailer's.
    async fn make(&self, core: &Core, uri: &Uri, size: u32) -> ThumbResult {
        let file = ThumbFile::open(core, uri).await?;
        let entry = &file.entry;
        if entry.is_dir() {
            return Err(Error::kind(ErrorKind::IsADirectory));
        }
        // Read when asked: a View options change holds from the next thumbnail on.
        let settings = ThumbSettings::new(&core.settings());
        if thumbnail_source(entry, file.local, true, &settings) != ThumbSource::App {
            return Err(Error::new(
                ErrorKind::Unsupported,
                "no app thumbnail for this file",
            ));
        }
        let pdf = category_of(entry) == FileCategory::Pdf;
        let version = thumb_key(&file.key_uri, 0, entry.modified_ms(), entry.etag.as_deref());
        if pdf && self.pdf.is_refused(&version) {
            return Err(Error::new(ErrorKind::Unsupported, "PDF cannot be drawn"));
        }
        let key = thumb_key(&file.key_uri, size, entry.modified_ms(), entry.etag.as_deref());
        if let Some(hit) = self.cache.get(&key) {
            return Ok(Arc::new(hit));
        }
        let handle = file.provider.open_read(&file.path, Lane::Interactive).await?;
        let bytes = if pdf {
            let (dir, limit) = (self.cache.dir(), settings.pdf_limit());
            self.pdf.make(dir, &version, handle.as_ref(), size, limit).await?
        } else {
            remote_thumbnail(handle.as_ref(), size, settings.max_remote_bytes).await?
        };
        if let Err(e) = self.cache.put(&key, &bytes) {
            log::warn!("thumbnail not cached: {e}");
        }
        Ok(Arc::new(bytes))
    }
}

struct ThumbFile {
    provider: Arc<dyn Provider>,
    path: VPath,
    entry: Entry,
    local: bool,
    key_uri: String,
}

impl ThumbFile {
    async fn open(core: &Core, uri: &Uri) -> Result<ThumbFile> {
        let provider = core.provider(&uri.location)?;
        let entry = provider.stat(&uri.path, true, Lane::Interactive).await?;
        Ok(ThumbFile {
            provider,
            path: uri.path.clone(),
            entry,
            local: core.locations.to_local_path(uri).is_some(),
            key_uri: uri.to_string(),
        })
    }
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
        let at = (READ_AHEAD_BYTES + 5) as u64;
        assert_eq!(r.read(at, 10).await.unwrap(), data[at as usize..at as usize + 10]);
        assert_eq!(h.reads.load(Ordering::Relaxed), 2);
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
}
