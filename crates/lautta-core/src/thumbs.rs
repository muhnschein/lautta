// SPDX-License-Identifier: LGPL-2.1-or-later
//! Remote thumbnails and the thumbnail cache (SPEC PRV-2, PRV-3).
//!
//! * [`remote_thumbnail`]: EXIF thumbnail from the first 64 KiB when there is
//!   one, otherwise the whole file (≤ 20 MB, ≤ 5 MB when metered) decoded
//!   and scaled, EXIF orientation applied.
//! * [`ThumbnailCache`]: on-disk cache, 200 MB LRU, keyed by
//!   sha256(URI, size, mtime, etag).
//! * [`ThumbJobs`]: at most two jobs in flight per location, newest request
//!   first (the delegates that are visible now), cancelable by key.
//!
//! Local thumbnails come from `Nemo.Thumbnailer` in QML (PRV-1), not from here.

use crate::error::{Error, ErrorKind, Result};
use crate::preview::exif;
use crate::provider::{read_all, ReadHandle};
use image::imageops::FilterType;
use image::{DynamicImage, ImageOutputFormat};
use sha2::{Digest, Sha256};
use std::collections::{BTreeMap, HashMap, VecDeque};
use std::future::Future;
use std::io::Cursor;
use std::path::{Path, PathBuf};
use std::pin::Pin;
use std::sync::{Arc, Mutex, MutexGuard};

/// Cache budget (PRV-2).
pub const DEFAULT_CACHE_BYTES: u64 = 200 * 1000 * 1000;
/// Head read looking for an EXIF thumbnail.
pub const HEAD_BYTES: u64 = 64 * 1024;
/// Largest file decoded for a thumbnail.
pub const MAX_DECODE_BYTES: u64 = 20 * 1000 * 1000;
/// The same on a metered network.
pub const MAX_DECODE_BYTES_METERED: u64 = 5 * 1000 * 1000;
/// Images with more pixels than this are not decoded (memory bound).
const MAX_PIXELS: u64 = 64_000_000;
const JPEG_QUALITY: u8 = 85;
/// Simultaneous jobs per location (PRV-3).
pub const MAX_IN_FLIGHT_PER_LOCATION: usize = 2;

/// Cache key: hex sha256 over URI, requested size, mtime and etag.
pub fn thumb_key(uri: &str, size: u32, mtime_ms: Option<i64>, etag: Option<&[u8]>) -> String {
    let mut h = Sha256::new();
    // Length prefixes keep the fields unambiguous.
    h.update((uri.len() as u64).to_le_bytes());
    h.update(uri.as_bytes());
    h.update(size.to_le_bytes());
    h.update(mtime_ms.map_or([0u8; 9], |m| {
        let mut b = [1u8; 9];
        b[1..].copy_from_slice(&m.to_le_bytes());
        b
    }));
    h.update(etag.map_or(0u64, |e| e.len() as u64 + 1).to_le_bytes());
    h.update(etag.unwrap_or_default());
    hex::encode(h.finalize())
}

fn thumb_error(msg: &str) -> Error {
    Error::new(ErrorKind::Unsupported, msg)
}

/// Applies an EXIF orientation value (1-8) to an image.
pub fn apply_orientation(img: DynamicImage, orientation: u8) -> DynamicImage {
    match orientation {
        2 => img.fliph(),
        3 => img.rotate180(),
        4 => img.flipv(),
        5 => img.rotate90().fliph(),
        6 => img.rotate90(),
        7 => img.rotate270().fliph(),
        8 => img.rotate270(),
        _ => img,
    }
}

fn encode(img: &DynamicImage) -> Result<Vec<u8>> {
    let mut out = Cursor::new(Vec::new());
    let result = if img.color().has_alpha() {
        img.write_to(&mut out, ImageOutputFormat::Png)
    } else {
        DynamicImage::ImageRgb8(img.to_rgb8()).write_to(&mut out, ImageOutputFormat::Jpeg(JPEG_QUALITY))
    };
    result.map_err(|e| thumb_error(&format!("cannot encode thumbnail: {e}")))?;
    Ok(out.into_inner())
}

/// Decodes `bytes`, scales it down to fit `target`×`target` (never up) and
/// applies `orientation`. Pure CPU work: call from a blocking context.
pub fn render_thumbnail(bytes: &[u8], target: u32, orientation: u8) -> Result<Vec<u8>> {
    let target = target.max(1);
    let reader = image::io::Reader::new(Cursor::new(bytes))
        .with_guessed_format()
        .map_err(|_| thumb_error("unknown image format"))?;
    let (w, h) = reader
        .into_dimensions()
        .map_err(|e| thumb_error(&format!("not a decodable image: {e}")))?;
    if u64::from(w) * u64::from(h) > MAX_PIXELS {
        return Err(thumb_error("image too large to thumbnail"));
    }
    let img =
        image::load_from_memory(bytes).map_err(|e| thumb_error(&format!("not a decodable image: {e}")))?;
    let img = if img.width() > target || img.height() > target {
        img.resize(target, target, FilterType::Triangle)
    } else {
        img
    };
    encode(&apply_orientation(img, orientation))
}

/// A thumbnail of a remote image (PRV-2). `metered` lowers the size limit
/// for the whole-file fallback. Returns JPEG, or PNG for images with alpha.
pub async fn remote_thumbnail(handle: &dyn ReadHandle, target_size: u32, metered: bool) -> Result<Vec<u8>> {
    let head = read_all(handle, HEAD_BYTES).await?;
    let orientation = exif::orientation(&head);
    if let Some(thumb) = exif::exif_thumbnail(&head) {
        // The embedded thumbnail is used as is when it needs no work.
        let rendered =
            tokio::task::spawn_blocking(move || render_thumbnail(&thumb, target_size, orientation))
                .await
                .map_err(|e| Error::new(ErrorKind::Internal, format!("thumbnail worker failed: {e}")))?;
        if let Ok(done) = rendered {
            return Ok(done);
        }
    }
    let limit = if metered {
        MAX_DECODE_BYTES_METERED
    } else {
        MAX_DECODE_BYTES
    };
    if handle.size().is_some_and(|s| s > limit) {
        return Err(thumb_error("file too large for a remote thumbnail"));
    }
    let bytes = read_all(handle, limit + 1).await?;
    if bytes.len() as u64 > limit {
        return Err(thumb_error("file too large for a remote thumbnail"));
    }
    let orientation = exif::orientation(&bytes);
    tokio::task::spawn_blocking(move || render_thumbnail(&bytes, target_size, orientation))
        .await
        .map_err(|e| Error::new(ErrorKind::Internal, format!("thumbnail worker failed: {e}")))?
}

#[derive(Default)]
struct CacheIndex {
    /// key to (bytes, recency sequence number)
    entries: HashMap<String, (u64, u64)>,
    /// recency sequence number to key, oldest first
    order: BTreeMap<u64, String>,
    total: u64,
    next_seq: u64,
}

impl CacheIndex {
    fn touch(&mut self, key: &str) -> bool {
        let Some(entry) = self.entries.get_mut(key) else {
            return false;
        };
        self.order.remove(&entry.1);
        entry.1 = self.next_seq;
        self.order.insert(self.next_seq, key.to_owned());
        self.next_seq += 1;
        true
    }

    fn insert(&mut self, key: &str, size: u64) {
        self.remove(key);
        self.entries.insert(key.to_owned(), (size, self.next_seq));
        self.order.insert(self.next_seq, key.to_owned());
        self.next_seq += 1;
        self.total += size;
    }

    fn remove(&mut self, key: &str) {
        if let Some((size, seq)) = self.entries.remove(key) {
            self.order.remove(&seq);
            self.total -= size;
        }
    }

    fn oldest(&self) -> Option<String> {
        self.order.values().next().cloned()
    }
}

/// Disk cache of generated thumbnails with least-recently-used eviction.
pub struct ThumbnailCache {
    dir: PathBuf,
    max_bytes: u64,
    index: Mutex<CacheIndex>,
}

impl ThumbnailCache {
    /// Opens (creating) `dir`; existing files count against the budget, the
    /// oldest by modification time are evicted first.
    pub fn new(dir: impl Into<PathBuf>, max_bytes: u64) -> Result<ThumbnailCache> {
        let dir = dir.into();
        std::fs::create_dir_all(&dir)?;
        let mut found: Vec<(std::time::SystemTime, String, u64)> = Vec::new();
        for item in std::fs::read_dir(&dir)? {
            let item = item?;
            let name = item.file_name().to_string_lossy().into_owned();
            let Some(key) = name.strip_suffix(".thumb") else {
                continue;
            };
            let meta = item.metadata()?;
            let mtime = meta.modified().unwrap_or(std::time::SystemTime::UNIX_EPOCH);
            found.push((mtime, key.to_owned(), meta.len()));
        }
        found.sort();
        let mut index = CacheIndex::default();
        for (_, key, size) in found {
            index.insert(&key, size);
        }
        let cache = ThumbnailCache {
            dir,
            max_bytes,
            index: Mutex::new(index),
        };
        cache.evict_to_budget(0);
        Ok(cache)
    }

    fn lock(&self) -> MutexGuard<'_, CacheIndex> {
        match self.index.lock() {
            Ok(g) => g,
            Err(p) => p.into_inner(),
        }
    }

    fn file(&self, key: &str) -> PathBuf {
        self.dir.join(format!("{key}.thumb"))
    }

    pub fn dir(&self) -> &Path {
        &self.dir
    }

    pub fn max_bytes(&self) -> u64 {
        self.max_bytes
    }

    pub fn total_bytes(&self) -> u64 {
        self.lock().total
    }

    pub fn len(&self) -> usize {
        self.lock().entries.len()
    }

    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }

    pub fn contains(&self, key: &str) -> bool {
        self.lock().entries.contains_key(key)
    }

    /// The cached image; marks it as most recently used.
    pub fn get(&self, key: &str) -> Option<Vec<u8>> {
        if !valid_key(key) || !self.lock().touch(key) {
            return None;
        }
        match std::fs::read(self.file(key)) {
            Ok(data) => Some(data),
            Err(_) => {
                self.lock().remove(key);
                None
            }
        }
    }

    /// Stores an image, then evicts the least recently used ones until the
    /// budget holds. An image larger than the whole budget is not stored.
    pub fn put(&self, key: &str, data: &[u8]) -> Result<()> {
        if !valid_key(key) {
            return Err(Error::new(ErrorKind::InvalidArgument, "bad thumbnail key"));
        }
        if data.len() as u64 > self.max_bytes {
            return Ok(());
        }
        let tmp = self.dir.join(format!("{key}.tmp"));
        std::fs::write(&tmp, data)?;
        std::fs::rename(&tmp, self.file(key))?;
        self.lock().insert(key, data.len() as u64);
        self.evict_to_budget(0);
        Ok(())
    }

    /// Drops one entry.
    pub fn remove(&self, key: &str) {
        if valid_key(key) {
            self.lock().remove(key);
            let _ = std::fs::remove_file(self.file(key));
        }
    }

    fn evict_to_budget(&self, reserve: u64) {
        loop {
            let victim = {
                let mut index = self.lock();
                if index.total + reserve <= self.max_bytes {
                    return;
                }
                let Some(key) = index.oldest() else {
                    return;
                };
                index.remove(&key);
                key
            };
            let _ = std::fs::remove_file(self.file(&victim));
        }
    }
}

/// Keys are hex digests; anything else could escape the cache directory.
fn valid_key(key: &str) -> bool {
    !key.is_empty() && key.len() <= 128 && key.bytes().all(|b| b.is_ascii_alphanumeric())
}

type Job = Pin<Box<dyn Future<Output = ()> + Send>>;

struct Pending {
    key: String,
    job: Job,
}

#[derive(Default)]
struct LocationJobs {
    running: HashMap<String, tokio::task::AbortHandle>,
    /// Newest first: the most recent request is the visible one (PRV-3).
    pending: VecDeque<Pending>,
}

#[derive(Default)]
struct JobState {
    locations: HashMap<String, LocationJobs>,
    /// key to location, for every pending or running job
    keys: HashMap<String, String>,
}

type SharedJobs = Arc<Mutex<JobState>>;

fn lock_jobs(state: &SharedJobs) -> MutexGuard<'_, JobState> {
    match state.lock() {
        Ok(g) => g,
        Err(p) => p.into_inner(),
    }
}

/// Frees the slot and starts the next job when a job ends or is aborted.
struct SlotGuard {
    state: SharedJobs,
    rt: tokio::runtime::Handle,
    location: String,
    key: String,
}

impl Drop for SlotGuard {
    fn drop(&mut self) {
        let mut st = lock_jobs(&self.state);
        st.keys.remove(&self.key);
        if let Some(loc) = st.locations.get_mut(&self.location) {
            loc.running.remove(&self.key);
        }
        pump(&mut st, &self.state, &self.rt, &self.location);
    }
}

fn pump(st: &mut JobState, shared: &SharedJobs, rt: &tokio::runtime::Handle, location: &str) {
    let Some(loc) = st.locations.get_mut(location) else {
        return;
    };
    while loc.running.len() < MAX_IN_FLIGHT_PER_LOCATION {
        let Some(next) = loc.pending.pop_front() else {
            break;
        };
        let guard = SlotGuard {
            state: Arc::clone(shared),
            rt: rt.clone(),
            location: location.to_owned(),
            key: next.key.clone(),
        };
        let job = next.job;
        let handle = rt.spawn(async move {
            let _slot = guard;
            job.await;
        });
        loc.running.insert(next.key, handle.abort_handle());
    }
    if loc.running.is_empty() && loc.pending.is_empty() {
        st.locations.remove(location);
    }
}

/// Thumbnail job scheduler (PRV-3).
#[derive(Clone)]
pub struct ThumbJobs {
    state: SharedJobs,
    rt: tokio::runtime::Handle,
}

impl ThumbJobs {
    /// Jobs run on `rt`, which also lets non-async threads (the UI) submit.
    pub fn new(rt: tokio::runtime::Handle) -> ThumbJobs {
        ThumbJobs {
            state: Arc::default(),
            rt,
        }
    }

    /// Queues `job` for `key` at `location`. Returns false (and drops `job`)
    /// when `key` is already queued or running. Newer requests start first.
    pub fn request(&self, key: &str, location: &str, job: impl Future<Output = ()> + Send + 'static) -> bool {
        let mut st = lock_jobs(&self.state);
        if st.keys.contains_key(key) {
            return false;
        }
        st.keys.insert(key.to_owned(), location.to_owned());
        st.locations
            .entry(location.to_owned())
            .or_default()
            .pending
            .push_front(Pending {
                key: key.to_owned(),
                job: Box::pin(job),
            });
        pump(&mut st, &self.state, &self.rt, location);
        true
    }

    /// Cancels a queued job (it never runs) or aborts a running one.
    /// Returns whether `key` was known.
    pub fn cancel(&self, key: &str) -> bool {
        let aborted = {
            let mut st = lock_jobs(&self.state);
            let Some(location) = st.keys.get(key).cloned() else {
                return false;
            };
            let loc = st.locations.entry(location.clone()).or_default();
            if let Some(at) = loc.pending.iter().position(|p| p.key == key) {
                loc.pending.remove(at);
                st.keys.remove(key);
                None
            } else {
                loc.running.get(key).cloned()
            }
        };
        // Outside the lock: the aborted task's guard takes it when dropped.
        if let Some(handle) = aborted {
            handle.abort();
        }
        true
    }

    /// Jobs queued for `location` that have not started.
    pub fn queued(&self, location: &str) -> usize {
        lock_jobs(&self.state)
            .locations
            .get(location)
            .map_or(0, |l| l.pending.len())
    }

    /// Jobs running for `location`.
    pub fn running(&self, location: &str) -> usize {
        lock_jobs(&self.state)
            .locations
            .get(location)
            .map_or(0, |l| l.running.len())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::preview::exif::testutil;
    use crate::provider::memory::MemoryProvider;
    use crate::provider::{Lane, Provider};
    use crate::vpath::VPath;
    use image::{Rgb, RgbImage, Rgba, RgbaImage};
    use std::sync::atomic::{AtomicUsize, Ordering};
    use std::time::Duration;
    use tokio::sync::{oneshot, Notify};

    fn jpeg_of(w: u32, h: u32, color: [u8; 3]) -> Vec<u8> {
        let img = DynamicImage::ImageRgb8(RgbImage::from_pixel(w, h, Rgb(color)));
        let mut out = Cursor::new(Vec::new());
        img.write_to(&mut out, ImageOutputFormat::Jpeg(90)).unwrap();
        out.into_inner()
    }

    fn png_of(w: u32, h: u32) -> Vec<u8> {
        let img = DynamicImage::ImageRgba8(RgbaImage::from_pixel(w, h, Rgba([10, 20, 30, 128])));
        let mut out = Cursor::new(Vec::new());
        img.write_to(&mut out, ImageOutputFormat::Png).unwrap();
        out.into_inner()
    }

    fn dims(bytes: &[u8]) -> (u32, u32) {
        let img = image::load_from_memory(bytes).unwrap();
        (img.width(), img.height())
    }

    async fn handle_for(data: &[u8]) -> Box<dyn ReadHandle> {
        let mem = MemoryProvider::default();
        mem.add_file("img", data, 0);
        mem.open_read(&VPath::parse(b"img").unwrap_or_default(), Lane::Interactive)
            .await
            .unwrap()
    }

    #[tokio::test]
    async fn decode_resize_keeps_aspect_ratio() {
        let src = jpeg_of(400, 200, [200, 30, 30]);
        let out = remote_thumbnail(handle_for(&src).await.as_ref(), 100, false)
            .await
            .unwrap();
        assert_eq!(dims(&out), (100, 50));
        assert_eq!(&out[..2], [0xFF, 0xD8], "photo without alpha comes back as JPEG");
        let tall = jpeg_of(100, 300, [0, 0, 200]);
        let out = remote_thumbnail(handle_for(&tall).await.as_ref(), 150, false)
            .await
            .unwrap();
        assert_eq!(dims(&out), (50, 150));
    }

    #[tokio::test]
    async fn small_images_are_not_upscaled_and_alpha_stays_png() {
        let out = remote_thumbnail(handle_for(&png_of(40, 20)).await.as_ref(), 256, false)
            .await
            .unwrap();
        assert_eq!(dims(&out), (40, 20));
        assert_eq!(&out[..4], [0x89, b'P', b'N', b'G']);
        let rgba = image::load_from_memory(&out).unwrap().to_rgba8();
        assert_eq!(rgba.get_pixel(0, 0).0[3], 128, "alpha is preserved");
    }

    #[tokio::test]
    async fn exif_orientation_is_applied_to_decoded_images() {
        let spec = testutil::Spec {
            orientation: Some(6),
            thumbnail: None,
            gps: false,
        };
        let src = testutil::with_exif(&jpeg_of(400, 200, [10, 200, 10]), &spec);
        let out = remote_thumbnail(handle_for(&src).await.as_ref(), 100, false)
            .await
            .unwrap();
        assert_eq!(dims(&out), (50, 100), "rotated by 90 degrees");
    }

    #[test]
    fn every_orientation_maps_pixels_correctly() {
        // 2x1 image: left pixel red, right pixel blue.
        let mut img = RgbImage::new(2, 1);
        img.put_pixel(0, 0, Rgb([255, 0, 0]));
        img.put_pixel(1, 0, Rgb([0, 0, 255]));
        let img = DynamicImage::ImageRgb8(img);
        let px = |o: u8| {
            let out = apply_orientation(img.clone(), o).to_rgb8();
            let first = out.get_pixel(0, 0).0;
            let last = out.get_pixel(out.width() - 1, out.height() - 1).0;
            (out.width(), out.height(), first, last)
        };
        let (red, blue) = ([255, 0, 0], [0, 0, 255]);
        assert_eq!(px(1), (2, 1, red, blue));
        assert_eq!(px(2), (2, 1, blue, red), "mirror");
        assert_eq!(px(3), (2, 1, blue, red), "rotate 180");
        assert_eq!(px(4), (2, 1, red, blue), "flip vertically");
        assert_eq!(px(5), (1, 2, red, blue), "transpose");
        assert_eq!(px(6), (1, 2, red, blue), "90 clockwise: left goes to top");
        assert_eq!(px(7), (1, 2, blue, red), "transverse");
        assert_eq!(
            px(8),
            (1, 2, blue, red),
            "90 counter-clockwise: left goes to bottom"
        );
        assert_eq!(px(0), (2, 1, red, blue));
    }

    #[tokio::test]
    async fn embedded_exif_thumbnail_is_used_without_reading_the_file() {
        let embedded = jpeg_of(160, 120, [250, 250, 0]);
        let spec = testutil::Spec {
            orientation: None,
            thumbnail: Some(embedded),
            gps: false,
        };
        // The "image" is garbage after the EXIF block: only the embedded
        // thumbnail can have produced a result.
        let mut file = testutil::bare_jpeg(&spec);
        file.extend(vec![0u8; 3_000_000]);
        let out = remote_thumbnail(handle_for(&file).await.as_ref(), 80, false)
            .await
            .unwrap();
        assert_eq!(dims(&out), (80, 60));
        let px = image::load_from_memory(&out)
            .unwrap()
            .to_rgb8()
            .get_pixel(10, 10)
            .0;
        assert!(px[0] > 200 && px[1] > 200 && px[2] < 80, "{px:?}");
    }

    #[tokio::test]
    async fn exif_thumbnail_respects_orientation() {
        let spec = testutil::Spec {
            orientation: Some(8),
            thumbnail: Some(jpeg_of(160, 120, [1, 2, 3])),
            gps: false,
        };
        let file = testutil::bare_jpeg(&spec);
        let out = remote_thumbnail(handle_for(&file).await.as_ref(), 1000, false)
            .await
            .unwrap();
        assert_eq!(dims(&out), (120, 160));
    }

    #[tokio::test]
    async fn size_limits_depend_on_the_network() {
        let mut big = jpeg_of(64, 64, [9, 9, 9]);
        big.resize(6_000_000, 0);
        let h = handle_for(&big).await;
        let metered = remote_thumbnail(h.as_ref(), 32, true).await;
        assert_eq!(metered.err().map(|e| e.kind), Some(ErrorKind::Unsupported));
        assert!(
            remote_thumbnail(h.as_ref(), 32, false).await.is_ok(),
            "6 MB is fine when not metered"
        );
        let mut huge = jpeg_of(64, 64, [9, 9, 9]);
        huge.resize(20_000_001, 0);
        let h = handle_for(&huge).await;
        assert!(remote_thumbnail(h.as_ref(), 32, false).await.is_err());
    }

    #[tokio::test]
    async fn non_images_are_an_error() {
        let h = handle_for(b"this is definitely not a picture").await;
        assert_eq!(
            remote_thumbnail(h.as_ref(), 32, false)
                .await
                .err()
                .map(|e| e.kind),
            Some(ErrorKind::Unsupported)
        );
        let h = handle_for(b"").await;
        assert!(remote_thumbnail(h.as_ref(), 32, false).await.is_err());
    }

    #[test]
    fn decompression_bomb_dimensions_are_refused() {
        // A PNG header claiming 20000x20000 pixels.
        let mut img = Cursor::new(Vec::new());
        DynamicImage::ImageLuma8(image::GrayImage::new(1, 1))
            .write_to(&mut img, ImageOutputFormat::Png)
            .unwrap();
        let mut png = img.into_inner();
        png[16..20].copy_from_slice(&20_000u32.to_be_bytes());
        png[20..24].copy_from_slice(&20_000u32.to_be_bytes());
        assert!(render_thumbnail(&png, 64, 1).is_err());
    }

    #[test]
    fn keys_differ_per_input_and_are_hex() {
        let base = thumb_key("srv:/a.jpg", 128, Some(5), Some(b"e1"));
        assert_eq!(base.len(), 64);
        assert!(base.bytes().all(|b| b.is_ascii_hexdigit()));
        assert_eq!(base, thumb_key("srv:/a.jpg", 128, Some(5), Some(b"e1")));
        assert_ne!(base, thumb_key("srv:/b.jpg", 128, Some(5), Some(b"e1")));
        assert_ne!(base, thumb_key("srv:/a.jpg", 256, Some(5), Some(b"e1")));
        assert_ne!(base, thumb_key("srv:/a.jpg", 128, Some(6), Some(b"e1")));
        assert_ne!(base, thumb_key("srv:/a.jpg", 128, Some(5), Some(b"e2")));
        assert_ne!(base, thumb_key("srv:/a.jpg", 128, None, Some(b"e1")));
        assert_ne!(base, thumb_key("srv:/a.jpg", 128, Some(5), None));
        assert_ne!(thumb_key("a", 1, None, Some(b"")), thumb_key("a", 1, None, None));
    }

    fn cache(max: u64) -> (tempfile::TempDir, ThumbnailCache) {
        let tmp = tempfile::tempdir().unwrap();
        let c = ThumbnailCache::new(tmp.path().join("thumbs"), max).unwrap();
        (tmp, c)
    }

    #[test]
    fn cache_stores_and_returns_bytes() {
        let (_tmp, c) = cache(1000);
        assert!(c.is_empty());
        c.put("aa", b"hello").unwrap();
        assert_eq!(c.get("aa").as_deref(), Some(&b"hello"[..]));
        assert_eq!((c.len(), c.total_bytes()), (1, 5));
        assert!(c.contains("aa") && !c.contains("bb"));
        assert_eq!(c.get("bb"), None);
        c.put("aa", b"hi").unwrap();
        assert_eq!((c.len(), c.total_bytes()), (1, 2), "replacing adjusts the total");
        c.remove("aa");
        assert!(c.is_empty() && c.get("aa").is_none());
        assert_eq!(std::fs::read_dir(c.dir()).unwrap().count(), 0);
    }

    #[test]
    fn cache_evicts_least_recently_used_first() {
        let (_tmp, c) = cache(30);
        c.put("k1", &[1; 10]).unwrap();
        c.put("k2", &[2; 10]).unwrap();
        c.put("k3", &[3; 10]).unwrap();
        assert_eq!(c.total_bytes(), 30);
        assert!(c.get("k1").is_some(), "k1 becomes the most recently used");
        c.put("k4", &[4; 10]).unwrap();
        assert!(!c.contains("k2"), "k2 was the oldest");
        assert!(c.contains("k1") && c.contains("k3") && c.contains("k4"));
        assert!(!c.dir().join("k2.thumb").exists(), "evicted from disk too");
        c.put("k5", &[5; 25]).unwrap();
        assert_eq!(c.len(), 1, "a big item pushes out several small ones");
        assert!(c.contains("k5"));
        assert!(c.total_bytes() <= c.max_bytes());
    }

    #[test]
    fn cache_ignores_items_larger_than_the_budget() {
        let (_tmp, c) = cache(10);
        c.put("big", &[0; 11]).unwrap();
        assert!(c.is_empty());
        c.put("fits", &[0; 10]).unwrap();
        assert_eq!(c.total_bytes(), 10);
    }

    #[test]
    fn cache_rejects_keys_that_could_escape_the_directory() {
        let (_tmp, c) = cache(100);
        for bad in ["", "../x", "a/b", "a.b", "k\0"] {
            assert_eq!(
                c.put(bad, b"x").err().map(|e| e.kind),
                Some(ErrorKind::InvalidArgument),
                "{bad:?}"
            );
            assert_eq!(c.get(bad), None);
        }
    }

    #[test]
    fn cache_survives_a_restart_and_enforces_a_smaller_budget() {
        let tmp = tempfile::tempdir().unwrap();
        let dir = tmp.path().join("t");
        {
            let c = ThumbnailCache::new(&dir, 1000).unwrap();
            c.put("old", &[1; 40]).unwrap();
            std::thread::sleep(Duration::from_millis(20));
            c.put("mid", &[2; 40]).unwrap();
            std::thread::sleep(Duration::from_millis(20));
            c.put("new", &[3; 40]).unwrap();
        }
        std::fs::write(dir.join("stray.tmp"), b"junk").unwrap();
        let c = ThumbnailCache::new(&dir, 1000).unwrap();
        assert_eq!((c.len(), c.total_bytes()), (3, 120));
        let c = ThumbnailCache::new(&dir, 90).unwrap();
        assert!(!c.contains("old"), "oldest file goes first");
        assert!(c.contains("mid") && c.contains("new"));
    }

    #[test]
    fn cache_drops_entries_whose_file_vanished() {
        let (_tmp, c) = cache(100);
        c.put("gone", b"data").unwrap();
        std::fs::remove_file(c.dir().join("gone.thumb")).unwrap();
        assert_eq!(c.get("gone"), None);
        assert!(!c.contains("gone"));
        assert_eq!(c.total_bytes(), 0);
    }

    struct Probe {
        running: AtomicUsize,
        peak: AtomicUsize,
        started: std::sync::Mutex<Vec<String>>,
    }

    fn probe() -> Arc<Probe> {
        Arc::new(Probe {
            running: AtomicUsize::new(0),
            peak: AtomicUsize::new(0),
            started: std::sync::Mutex::new(Vec::new()),
        })
    }

    /// A job that records itself, then waits for `gate`.
    fn gated(p: &Arc<Probe>, name: &str, gate: Arc<Notify>) -> impl Future<Output = ()> + Send + 'static {
        let p = Arc::clone(p);
        let name = name.to_owned();
        async move {
            p.started.lock().unwrap().push(name);
            let now = p.running.fetch_add(1, Ordering::SeqCst) + 1;
            p.peak.fetch_max(now, Ordering::SeqCst);
            gate.notified().await;
            p.running.fetch_sub(1, Ordering::SeqCst);
        }
    }

    async fn settle() {
        for _ in 0..20 {
            tokio::task::yield_now().await;
        }
        tokio::time::sleep(Duration::from_millis(20)).await;
    }

    fn started(p: &Probe) -> Vec<String> {
        p.started.lock().unwrap().clone()
    }

    #[tokio::test]
    async fn at_most_two_in_flight_per_location_and_newest_first() {
        let jobs = ThumbJobs::new(tokio::runtime::Handle::current());
        let p = probe();
        let gate = Arc::new(Notify::new());
        for name in ["a", "b", "c", "d", "e"] {
            assert!(jobs.request(name, "loc", gated(&p, name, Arc::clone(&gate))));
        }
        settle().await;
        assert_eq!(started(&p), ["a", "b"], "the first two start at once");
        assert_eq!((jobs.running("loc"), jobs.queued("loc")), (2, 3));
        gate.notify_one();
        settle().await;
        assert_eq!(
            started(&p).last().map(String::as_str),
            Some("e"),
            "the newest request goes next"
        );
        gate.notify_waiters();
        settle().await;
        gate.notify_waiters();
        settle().await;
        gate.notify_waiters();
        settle().await;
        assert_eq!(started(&p).len(), 5);
        assert!(p.peak.load(Ordering::SeqCst) <= 2);
        assert_eq!((jobs.running("loc"), jobs.queued("loc")), (0, 0));
    }

    #[tokio::test]
    async fn locations_do_not_block_each_other() {
        let jobs = ThumbJobs::new(tokio::runtime::Handle::current());
        let p = probe();
        let gate = Arc::new(Notify::new());
        for (i, loc) in ["one", "one", "one", "two", "two"].iter().enumerate() {
            jobs.request(
                &format!("k{i}"),
                loc,
                gated(&p, &format!("k{i}"), Arc::clone(&gate)),
            );
        }
        settle().await;
        assert_eq!((jobs.running("one"), jobs.queued("one")), (2, 1));
        assert_eq!((jobs.running("two"), jobs.queued("two")), (2, 0));
        gate.notify_waiters();
        settle().await;
        gate.notify_waiters();
        settle().await;
    }

    #[tokio::test]
    async fn duplicate_keys_are_ignored_until_the_job_finishes() {
        let jobs = ThumbJobs::new(tokio::runtime::Handle::current());
        let p = probe();
        let gate = Arc::new(Notify::new());
        assert!(jobs.request("k", "loc", gated(&p, "k", Arc::clone(&gate))));
        assert!(!jobs.request("k", "loc", gated(&p, "dup", Arc::clone(&gate))));
        settle().await;
        assert!(
            !jobs.request("k", "loc", gated(&p, "dup", Arc::clone(&gate))),
            "still running"
        );
        gate.notify_waiters();
        settle().await;
        assert!(
            jobs.request("k", "loc", gated(&p, "again", Arc::clone(&gate))),
            "free again"
        );
        settle().await;
        assert_eq!(started(&p), ["k", "again"]);
        gate.notify_waiters();
        settle().await;
    }

    #[tokio::test]
    async fn cancel_removes_queued_jobs_before_they_run() {
        let jobs = ThumbJobs::new(tokio::runtime::Handle::current());
        let p = probe();
        let gate = Arc::new(Notify::new());
        for name in ["a", "b", "c", "d"] {
            jobs.request(name, "loc", gated(&p, name, Arc::clone(&gate)));
        }
        settle().await;
        assert!(jobs.cancel("c"));
        assert!(!jobs.cancel("c"), "already gone");
        assert!(!jobs.cancel("never-requested"));
        assert_eq!(jobs.queued("loc"), 1);
        gate.notify_waiters();
        settle().await;
        gate.notify_waiters();
        settle().await;
        assert_eq!(started(&p), ["a", "b", "d"], "c never ran");
    }

    #[tokio::test]
    async fn cancelling_a_running_job_frees_its_slot() {
        let jobs = ThumbJobs::new(tokio::runtime::Handle::current());
        let p = probe();
        let gate = Arc::new(Notify::new());
        for name in ["a", "b", "c"] {
            jobs.request(name, "loc", gated(&p, name, Arc::clone(&gate)));
        }
        settle().await;
        assert_eq!(started(&p), ["a", "b"]);
        assert!(jobs.cancel("a"));
        settle().await;
        assert_eq!(
            started(&p),
            ["a", "b", "c"],
            "the waiting job took the freed slot"
        );
        assert_eq!(jobs.running("loc"), 2);
        gate.notify_waiters();
        settle().await;
        assert_eq!(jobs.running("loc"), 0);
    }

    #[tokio::test]
    async fn finished_jobs_report_through_their_own_channel() {
        let jobs = ThumbJobs::new(tokio::runtime::Handle::current());
        let (tx, rx) = oneshot::channel();
        jobs.request("k", "loc", async move {
            let _ = tx.send(42);
        });
        assert_eq!(rx.await.unwrap(), 42);
        settle().await;
        assert_eq!(jobs.running("loc"), 0);
    }
}
