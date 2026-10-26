// SPDX-License-Identifier: LGPL-2.1-or-later
//! Thumbnails of server PDFs (SPEC PRV-14; local ones are the system
//! thumbnailer's, PRV-1): the first page, drawn by `lautta-pdf` as wide as
//! the thumbnail and cut to a square from the top, as JPEG.
//!
//! A phone has the CPU for one page at a time: [`PdfThumbs`] lets a render
//! wait for the one before it, also when the delegate that asked has gone
//! (a render cannot be stopped). A PDF that cannot be drawn, a render that
//! panicked included (RS-4), is remembered, so scrolling back to it costs
//! nothing, and one whose render took the process down (stack overflow,
//! out of memory) is marked on disk, so the next start does not try it again.

use crate::error::{Error, ErrorKind, Result};
use crate::panics::catch;
use crate::provider::{read_all, ReadHandle};
use crate::thumbs::encode;
use image::{DynamicImage, Rgb, RgbImage};
use lautta_pdf::{PdfDocument, PdfError};
use std::collections::VecDeque;
use std::path::Path;
use std::sync::{Arc, Mutex, PoisonError};
use tokio::sync::Semaphore;

/// PDFs remembered as not drawable: the oldest are forgotten first.
const MAX_REFUSED: usize = 256;

fn unsupported(msg: &str) -> Error {
    Error::new(ErrorKind::Unsupported, msg)
}

fn too_large() -> Error {
    unsupported("PDF too large for a thumbnail")
}

fn internal(e: impl ToString) -> Error {
    Error::new(ErrorKind::Internal, e.to_string())
}

fn pdf_problem(e: PdfError) -> Error {
    unsupported(&e.to_string())
}

/// The first page of a PDF as a JPEG thumbnail: `size` pixels wide (less
/// when the page is very tall, see `lautta_pdf::MAX_EDGE`) and, when the
/// page is taller than wide, cut to a square from the top. Pure CPU work:
/// call from a blocking context.
pub fn first_page(bytes: Vec<u8>, size: u32) -> Result<Vec<u8>> {
    let doc = PdfDocument::open(bytes).map_err(pdf_problem)?;
    let page = doc.render(0, size).map_err(pdf_problem)?;
    let side = page.height.min(page.width);
    let width = page.width as usize;
    // Pages are drawn on white, so every pixel is opaque and the alpha
    // channel can go. `RenderedPage` has 4 bytes per pixel, rows top to
    // bottom, no padding: the indices stay within `rgba`.
    let rgb = RgbImage::from_fn(page.width, side, |x, y| {
        let at = (y as usize * width + x as usize) * 4;
        Rgb([page.rgba[at], page.rgba[at + 1], page.rgba[at + 2]])
    });
    encode(&DynamicImage::ImageRgb8(rgb))
}

/// `draw(bytes, size)`, with a panic in it (a PDF that trips hayro or the
/// code around it) as a refusal of the PDF, not the end of the app (RS-4).
fn drawn(draw: Draw, bytes: Vec<u8>, size: u32) -> Result<Vec<u8>> {
    catch(|| draw(bytes, size)).unwrap_or_else(|_| Err(unsupported("drawing the PDF failed")))
}

/// What draws a thumbnail: [`first_page`]; tests give one that panics.
type Draw = fn(Vec<u8>, u32) -> Result<Vec<u8>>;

/// Runs `work` between a marker file and its removal. A marker that is
/// there already means an earlier run did not end, and `work` does not run.
fn guarded<T>(marker: &Path, work: impl FnOnce() -> Result<T>) -> Result<T> {
    if marker.exists() {
        return Err(unsupported("an earlier try at this PDF did not end"));
    }
    std::fs::write(marker, [])?;
    let made = work();
    // Stays when the process is gone before this line.
    let _ = std::fs::remove_file(marker);
    made
}

/// Turns, memory and a record of failures for PDF thumbnails.
pub struct PdfThumbs {
    /// The one render at a time.
    gate: Arc<Semaphore>,
    /// File versions that cannot be drawn, oldest first.
    refused: Mutex<VecDeque<String>>,
}

impl Default for PdfThumbs {
    fn default() -> Self {
        PdfThumbs {
            gate: Arc::new(Semaphore::new(1)),
            refused: Mutex::default(),
        }
    }
}

impl PdfThumbs {
    /// Whether `version` (a file's URI, mtime and etag, see
    /// `thumbs::thumb_key` with size 0) was found not drawable.
    pub fn is_refused(&self, version: &str) -> bool {
        self.refused
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .iter()
            .any(|v| v == version)
    }

    /// Remembers `version` as not drawable; true when that is new.
    fn refuse(&self, version: &str) -> bool {
        let mut list = self.refused.lock().unwrap_or_else(PoisonError::into_inner);
        if list.iter().any(|v| v == version) {
            return false;
        }
        if list.len() == MAX_REFUSED {
            list.pop_front();
        }
        list.push_back(version.to_owned());
        true
    }

    /// The thumbnail of the PDF behind `handle`, `size` pixels wide. A file
    /// larger than `max_bytes` is not read. `dir` holds the marker files and
    /// `version` names the file's version (see [`PdfThumbs::is_refused`]);
    /// a PDF that cannot be drawn is refused from then on.
    pub async fn make(
        &self,
        dir: &Path,
        version: &str,
        handle: &dyn ReadHandle,
        size: u32,
        max_bytes: u64,
    ) -> Result<Vec<u8>> {
        self.make_with(first_page, dir, version, handle, size, max_bytes)
            .await
    }

    /// [`PdfThumbs::make`], drawn by `draw`.
    async fn make_with(
        &self,
        draw: Draw,
        dir: &Path,
        version: &str,
        handle: &dyn ReadHandle,
        size: u32,
        max_bytes: u64,
    ) -> Result<Vec<u8>> {
        if self.is_refused(version) {
            return Err(unsupported("PDF cannot be drawn"));
        }
        if handle.size().is_some_and(|s| s > max_bytes) {
            return Err(too_large());
        }
        // A file that grew or lied about its size is cut one byte past the
        // limit, so nothing larger is held in memory.
        let bytes = read_all(handle, max_bytes.saturating_add(1)).await?;
        if bytes.len() as u64 > max_bytes {
            return Err(too_large());
        }
        let marker = dir.join(format!("{version}.crash"));
        let made = self
            .exclusive(move || guarded(&marker, || drawn(draw, bytes, size)))
            .await
            .and_then(|r| r);
        if let Err(e) = &made {
            if e.kind == ErrorKind::Unsupported && self.refuse(version) {
                log::debug!("PDF thumbnail not made: {}", e.message);
            }
        }
        made
    }

    /// Runs `work` on the blocking pool when no other render runs. Dropping
    /// the future before then cancels it; once it runs it goes on to its end
    /// and holds its turn until then.
    async fn exclusive<T: Send + 'static>(&self, work: impl FnOnce() -> T + Send + 'static) -> Result<T> {
        let turn = Arc::clone(&self.gate).acquire_owned().await.map_err(internal)?;
        tokio::task::spawn_blocking(move || {
            let _turn = turn;
            work()
        })
        .await
        .map_err(internal)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use lautta_pdf::sample::{build, page, LOCKED_AES256, LOCKED_RC4, RED_SQUARE};
    use std::sync::atomic::{AtomicUsize, Ordering};
    use std::time::Duration;

    /// A4 with a red square (50 pt) in the top left corner.
    const RED_TOP_LEFT: &str = "1 0 0 rg 0 792 50 50 re f";
    const BLUE_PAGE: &str = "0 0 1 rg 0 0 595 842 re f";

    fn pixels(jpeg: &[u8]) -> RgbImage {
        image::load_from_memory(jpeg).unwrap().to_rgb8()
    }

    fn is_red(p: &Rgb<u8>) -> bool {
        p.0[0] > 200 && p.0[1] < 60 && p.0[2] < 60
    }

    fn is_white(p: &Rgb<u8>) -> bool {
        p.0.iter().all(|c| *c > 235)
    }

    fn a4_then_blue() -> Vec<u8> {
        build(&[page(595, 842, RED_TOP_LEFT), page(595, 842, BLUE_PAGE)])
    }

    #[test]
    fn a_portrait_page_is_cut_to_a_square_from_the_top() {
        let out = first_page(a4_then_blue(), 64).unwrap();
        assert_eq!(&out[..2], [0xFF, 0xD8], "a JPEG");
        let img = pixels(&out);
        assert_eq!(img.dimensions(), (64, 64), "as wide as asked, as tall as wide");
        assert!(
            is_red(img.get_pixel(2, 2)),
            "the top left corner of the first page"
        );
        assert!(
            is_white(img.get_pixel(40, 40)),
            "page 2 is blue: only page 1 is drawn"
        );
        assert!(is_white(img.get_pixel(60, 60)));
    }

    #[test]
    fn the_thumbnail_is_as_wide_as_asked() {
        for size in [16, 100, 270] {
            let img = pixels(&first_page(a4_then_blue(), size).unwrap());
            assert_eq!(img.dimensions(), (size, size), "size {size}");
        }
    }

    #[test]
    fn a_landscape_page_is_whole() {
        let out = first_page(build(&[page(300, 150, RED_SQUARE)]), 64).unwrap();
        assert_eq!(pixels(&out).dimensions(), (64, 32));
    }

    #[test]
    fn a_very_tall_page_is_a_square_of_what_the_renderer_allows() {
        let strip = build(&[page(100, 10_000, "")]);
        let (w, h) = pixels(&first_page(strip, 64).unwrap()).dimensions();
        assert_eq!(w, h, "a square");
        assert!(
            w < 64,
            "narrower than asked, as the page limit of lautta-pdf says: {w}"
        );
    }

    #[test]
    fn pdfs_that_cannot_be_drawn_are_unsupported() {
        for (what, bytes) in [
            ("garbage", b"not a pdf".to_vec()),
            ("empty file", Vec::new()),
            ("no pages", build(&[])),
            ("AES-256 password", LOCKED_AES256.to_vec()),
            ("RC4 password", LOCKED_RC4.to_vec()),
        ] {
            let err = first_page(bytes, 64).unwrap_err();
            assert_eq!(err.kind, ErrorKind::Unsupported, "{what}");
        }
        assert_eq!(
            first_page(LOCKED_AES256.to_vec(), 64).unwrap_err().message,
            "the PDF is protected with a password"
        );
    }

    #[test]
    fn a_marker_surrounds_the_work_and_goes_with_it() {
        let dir = tempfile::tempdir().unwrap();
        let marker = dir.path().join("v.crash");
        let seen = guarded(&marker, || Ok(marker.exists())).unwrap();
        assert!(seen, "the marker is there while the work runs");
        assert!(!marker.exists(), "and gone after it");
        let failed: Result<()> = guarded(&marker, || Err(unsupported("bad")));
        assert_eq!(failed.unwrap_err().message, "bad", "the work's error is kept");
        assert!(!marker.exists(), "a failure takes the marker away too");
    }

    #[test]
    fn work_that_did_not_end_last_time_is_not_tried_again() {
        let dir = tempfile::tempdir().unwrap();
        let marker = dir.path().join("v.crash");
        let died = std::panic::catch_unwind(|| guarded(&marker, || -> Result<()> { panic!("render") }));
        assert!(died.is_err());
        assert!(marker.exists(), "a render that never returned leaves its marker");
        let ran = AtomicUsize::new(0);
        let again = guarded(&marker, || {
            ran.fetch_add(1, Ordering::Relaxed);
            Ok(())
        });
        assert_eq!(again.unwrap_err().kind, ErrorKind::Unsupported);
        assert_eq!(ran.load(Ordering::Relaxed), 0, "the work does not run");
        assert!(marker.exists(), "and the marker stays");
    }

    #[test]
    fn a_marker_that_cannot_be_written_stops_the_work() {
        let dir = tempfile::tempdir().unwrap();
        let marker = dir.path().join("no-such-folder").join("v.crash");
        let ran = AtomicUsize::new(0);
        let res = guarded(&marker, || {
            ran.fetch_add(1, Ordering::Relaxed);
            Ok(())
        });
        assert!(
            res.is_err_and(|e| e.kind != ErrorKind::Unsupported),
            "an I/O error, not a verdict"
        );
        assert_eq!(ran.load(Ordering::Relaxed), 0, "nothing renders unmarked");
    }

    /// A handle over `data` that counts reads and may claim another size.
    struct Counting {
        data: Vec<u8>,
        size: Option<u64>,
        reads: AtomicUsize,
        asked: AtomicUsize,
    }

    impl Counting {
        fn new(data: Vec<u8>) -> Counting {
            let size = Some(data.len() as u64);
            Counting {
                data,
                size,
                reads: AtomicUsize::new(0),
                asked: AtomicUsize::new(0),
            }
        }
    }

    #[async_trait::async_trait]
    impl ReadHandle for Counting {
        fn size(&self) -> Option<u64> {
            self.size
        }

        async fn read_at(&self, offset: u64, max: usize) -> Result<Vec<u8>> {
            self.reads.fetch_add(1, Ordering::Relaxed);
            let start = usize::try_from(offset).unwrap_or(usize::MAX).min(self.data.len());
            let end = (start + max).min(self.data.len());
            self.asked.fetch_add(end - start, Ordering::Relaxed);
            Ok(self.data[start..end].to_vec())
        }
    }

    fn pdf_bytes() -> Vec<u8> {
        a4_then_blue()
    }

    #[tokio::test]
    async fn a_pdf_is_read_and_drawn() {
        let dir = tempfile::tempdir().unwrap();
        let pdfs = PdfThumbs::default();
        let h = Counting::new(pdf_bytes());
        let out = pdfs.make(dir.path(), "v1", &h, 32, 1_000_000).await.unwrap();
        assert_eq!(pixels(&out).dimensions(), (32, 32));
        assert!(!pdfs.is_refused("v1"));
        assert_eq!(
            std::fs::read_dir(dir.path()).unwrap().count(),
            0,
            "no marker is left"
        );
    }

    #[tokio::test]
    async fn a_pdf_over_the_limit_is_not_read() {
        let dir = tempfile::tempdir().unwrap();
        let pdfs = PdfThumbs::default();
        let data = pdf_bytes();
        let h = Counting::new(data.clone());
        let err = pdfs
            .make(dir.path(), "v1", &h, 32, data.len() as u64 - 1)
            .await
            .unwrap_err();
        assert_eq!(err.kind, ErrorKind::Unsupported);
        assert_eq!(
            h.reads.load(Ordering::Relaxed),
            0,
            "refused by its size, before reading"
        );
        assert!(!pdfs.is_refused("v1"), "a larger limit can allow it later");
        assert!(pdfs
            .make(dir.path(), "v1", &h, 32, data.len() as u64)
            .await
            .is_ok());
    }

    #[tokio::test]
    async fn a_pdf_of_unknown_or_false_size_is_cut_one_byte_past_the_limit() {
        let dir = tempfile::tempdir().unwrap();
        let pdfs = PdfThumbs::default();
        let mut h = Counting::new(vec![b' '; 5_000_000]);
        h.size = None;
        let err = pdfs.make(dir.path(), "v1", &h, 32, 1000).await.unwrap_err();
        assert_eq!(err.kind, ErrorKind::Unsupported);
        assert_eq!(h.asked.load(Ordering::Relaxed), 1001, "not a byte more");
        h.size = Some(10);
        assert!(pdfs.make(dir.path(), "v2", &h, 32, 1000).await.is_err());
        assert_eq!(h.asked.load(Ordering::Relaxed), 2002, "a false size is no help");
        assert!(!pdfs.is_refused("v1") && !pdfs.is_refused("v2"));
    }

    #[tokio::test]
    async fn a_pdf_that_cannot_be_drawn_is_refused_and_not_read_again() {
        let dir = tempfile::tempdir().unwrap();
        let pdfs = PdfThumbs::default();
        let h = Counting::new(LOCKED_AES256.to_vec());
        let err = pdfs
            .make(dir.path(), "locked", &h, 32, 1_000_000)
            .await
            .unwrap_err();
        assert_eq!(err.kind, ErrorKind::Unsupported);
        assert!(pdfs.is_refused("locked"));
        assert!(!pdfs.is_refused("other"));
        let reads = h.reads.load(Ordering::Relaxed);
        assert!(reads > 0);
        let again = pdfs
            .make(dir.path(), "locked", &h, 64, 1_000_000)
            .await
            .unwrap_err();
        assert_eq!(again.kind, ErrorKind::Unsupported);
        assert_eq!(h.reads.load(Ordering::Relaxed), reads, "not read a second time");
        assert_eq!(
            std::fs::read_dir(dir.path()).unwrap().count(),
            0,
            "no marker is left"
        );
    }

    #[tokio::test]
    async fn a_failed_read_is_not_a_verdict_on_the_pdf() {
        struct Broken;
        #[async_trait::async_trait]
        impl ReadHandle for Broken {
            fn size(&self) -> Option<u64> {
                Some(10)
            }
            async fn read_at(&self, _: u64, _: usize) -> Result<Vec<u8>> {
                Err(Error::kind(ErrorKind::ConnectionLost))
            }
        }
        let dir = tempfile::tempdir().unwrap();
        let pdfs = PdfThumbs::default();
        let err = pdfs.make(dir.path(), "v", &Broken, 32, 1000).await.unwrap_err();
        assert_eq!(err.kind, ErrorKind::ConnectionLost);
        assert!(!pdfs.is_refused("v"), "a dropped connection may come back");
    }

    #[tokio::test]
    async fn a_marker_that_cannot_be_written_is_not_a_verdict_on_the_pdf() {
        let dir = tempfile::tempdir().unwrap();
        let gone = dir.path().join("not-there");
        let pdfs = PdfThumbs::default();
        let h = Counting::new(pdf_bytes());
        let err = pdfs.make(&gone, "v", &h, 32, 1_000_000).await.unwrap_err();
        assert_ne!(
            err.kind,
            ErrorKind::Unsupported,
            "a folder problem, not the PDF's"
        );
        assert!(!pdfs.is_refused("v"));
        assert!(
            pdfs.make(dir.path(), "v", &h, 32, 1_000_000).await.is_ok(),
            "drawn once the folder is there"
        );
    }

    #[tokio::test]
    async fn a_marker_from_a_crash_refuses_the_file_once_then_it_is_remembered() {
        let dir = tempfile::tempdir().unwrap();
        std::fs::write(dir.path().join("v.crash"), []).unwrap();
        let pdfs = PdfThumbs::default();
        let h = Counting::new(pdf_bytes());
        let err = pdfs.make(dir.path(), "v", &h, 32, 1_000_000).await.unwrap_err();
        assert_eq!(err.kind, ErrorKind::Unsupported);
        assert!(pdfs.is_refused("v"));
        assert!(
            dir.path().join("v.crash").exists(),
            "the marker is for the next start too"
        );
        assert!(
            pdfs.make(dir.path(), "w", &h, 32, 1_000_000).await.is_ok(),
            "other PDFs are not affected"
        );
    }

    /// A draw that trips over the PDF as hayro might.
    fn boom(_: Vec<u8>, _: u32) -> Result<Vec<u8>> {
        panic!("hayro tripped")
    }

    #[tokio::test]
    async fn a_render_that_panics_is_refused_and_the_app_goes_on() {
        let dir = tempfile::tempdir().unwrap();
        let pdfs = PdfThumbs::default();
        let h = Counting::new(pdf_bytes());
        let err = pdfs
            .make_with(boom, dir.path(), "v", &h, 32, 1_000_000)
            .await
            .unwrap_err();
        assert_eq!(err.kind, ErrorKind::Unsupported, "a verdict on the PDF: {err:?}");
        assert!(pdfs.is_refused("v"), "not drawn again this session");
        assert_eq!(
            std::fs::read_dir(dir.path()).unwrap().count(),
            0,
            "no marker: only a render that took the process down leaves one"
        );
        assert!(
            pdfs.make(dir.path(), "w", &h, 32, 1_000_000).await.is_ok(),
            "the next PDF gets its turn and is drawn"
        );
    }

    #[tokio::test]
    async fn making_one_waits_for_the_render_before() {
        let dir = tempfile::tempdir().unwrap();
        let pdfs = Arc::new(PdfThumbs::default());
        let turn = Arc::clone(&pdfs.gate).acquire_owned().await.unwrap();
        let made = {
            let (pdfs, path) = (Arc::clone(&pdfs), dir.path().to_path_buf());
            tokio::spawn(async move {
                let h = Counting::new(pdf_bytes());
                pdfs.make(&path, "v", &h, 32, 1_000_000).await
            })
        };
        settle().await;
        assert!(!made.is_finished(), "another render has the turn");
        drop(turn);
        assert!(made.await.unwrap().is_ok(), "then it is made");
    }

    #[test]
    fn only_the_newest_refusals_are_remembered() {
        let pdfs = PdfThumbs::default();
        assert!(pdfs.refuse("first"), "new");
        assert!(!pdfs.refuse("first"), "already known");
        for i in 0..MAX_REFUSED - 1 {
            assert!(pdfs.refuse(&format!("v{i}")));
        }
        assert!(
            pdfs.is_refused("first"),
            "the list is full, nothing forgotten yet"
        );
        assert!(pdfs.refuse("one more"));
        assert!(!pdfs.is_refused("first"), "the oldest goes");
        assert!(pdfs.is_refused("v0") && pdfs.is_refused("one more"));
        assert_eq!(pdfs.refused.lock().unwrap().len(), MAX_REFUSED);
    }

    async fn settle() {
        for _ in 0..20 {
            tokio::task::yield_now().await;
        }
        tokio::time::sleep(Duration::from_millis(20)).await;
    }

    /// Jobs for `exclusive` that count how many run at once and wait for
    /// `release`.
    struct Probe {
        running: AtomicUsize,
        peak: AtomicUsize,
        started: AtomicUsize,
        release: Mutex<std::sync::mpsc::Receiver<()>>,
    }

    impl Probe {
        fn job(self: &Arc<Self>) -> impl FnOnce() + Send + 'static {
            let me = Arc::clone(self);
            move || {
                let now = me.running.fetch_add(1, Ordering::SeqCst) + 1;
                me.peak.fetch_max(now, Ordering::SeqCst);
                me.started.fetch_add(1, Ordering::SeqCst);
                let _ = me.release.lock().unwrap().recv();
                me.running.fetch_sub(1, Ordering::SeqCst);
            }
        }
    }

    #[tokio::test(flavor = "multi_thread", worker_threads = 4)]
    async fn only_one_render_runs_at_a_time() {
        let (release, wait) = std::sync::mpsc::channel();
        let probe = Arc::new(Probe {
            running: AtomicUsize::new(0),
            peak: AtomicUsize::new(0),
            started: AtomicUsize::new(0),
            release: Mutex::new(wait),
        });
        let pdfs = Arc::new(PdfThumbs::default());
        let jobs: Vec<_> = (0..4)
            .map(|_| {
                let (pdfs, job) = (Arc::clone(&pdfs), probe.job());
                tokio::spawn(async move { pdfs.exclusive(job).await })
            })
            .collect();
        while probe.started.load(Ordering::SeqCst) == 0 {
            tokio::task::yield_now().await;
        }
        settle().await;
        assert_eq!(
            probe.started.load(Ordering::SeqCst),
            1,
            "the others wait for their turn"
        );
        for done in 1..=4 {
            release.send(()).unwrap();
            while probe.started.load(Ordering::SeqCst) < (done + 1).min(4)
                || probe.running.load(Ordering::SeqCst) > 1
            {
                tokio::task::yield_now().await;
            }
        }
        for j in jobs {
            j.await.unwrap().unwrap();
        }
        assert_eq!(probe.started.load(Ordering::SeqCst), 4);
        assert_eq!(probe.peak.load(Ordering::SeqCst), 1, "never two at once");
    }

    #[tokio::test]
    async fn a_render_whose_delegate_left_while_waiting_never_runs() {
        let pdfs = Arc::new(PdfThumbs::default());
        let (release, wait) = std::sync::mpsc::channel::<()>();
        let (running, up) = tokio::sync::oneshot::channel();
        let first = {
            let pdfs = Arc::clone(&pdfs);
            tokio::spawn(async move {
                pdfs.exclusive(move || {
                    let _ = running.send(());
                    let _ = wait.recv();
                })
                .await
            })
        };
        up.await.unwrap();
        let ran = Arc::new(AtomicUsize::new(0));
        let second = {
            let (pdfs, ran) = (Arc::clone(&pdfs), Arc::clone(&ran));
            tokio::spawn(async move {
                pdfs.exclusive(move || {
                    ran.fetch_add(1, Ordering::SeqCst);
                })
                .await
            })
        };
        settle().await;
        second.abort();
        assert!(second.await.unwrap_err().is_cancelled());
        release.send(()).unwrap();
        first.await.unwrap().unwrap();
        pdfs.exclusive(|| ()).await.unwrap();
        settle().await;
        assert_eq!(
            ran.load(Ordering::SeqCst),
            0,
            "dropped while it waited: it never ran"
        );
    }
}
