// SPDX-License-Identifier: LGPL-2.1-or-later
//! The PDF viewer's actions on [`Core`] (PRV-12): opening a PDF from any
//! location, rendering its pages for `image://lautta-pdf/`, reading its
//! outline and links and searching its text. Parsing, drawing and matching
//! are `lautta-pdf`'s; this module reads the file, keeps the open
//! documents, runs the search page by page and moves the CPU work off the
//! runtime's threads. A panic in that work (a PDF that trips hayro or the
//! code around it) fails that one request, not the app (RS-4).

use crate::app::Core;
use crate::error::{Error, ErrorKind, Result};
use crate::org::recents::RecentKind;
use crate::panics::catch;
use crate::provider::Lane;
use crate::uri::Uri;
use lautta_pdf::{PdfDocument, RenderedPage, MAX_PDF_BYTES};
use serde::Serialize;
use std::collections::BTreeMap;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex, MutexGuard, OnceLock};
use tokio::sync::mpsc;

pub use lautta_pdf::{Link, LinkTarget, OutlineItem, PageRect, PageSize, PdfError, TextHit};

/// How many documents stay parsed: the one shown and the one before it.
const KEPT_DOCUMENTS: usize = 2;
/// Most hits a search of one document finds; it stops there, so memory
/// stays bounded.
pub const MAX_SEARCH_HITS: usize = 10_000;

/// Why a PDF cannot be shown: the file could not be read, or it is not a
/// PDF the viewer can show.
#[derive(Debug, Clone, PartialEq)]
pub enum PdfFailure {
    File(Error),
    Pdf(PdfError),
}

impl From<Error> for PdfFailure {
    fn from(e: Error) -> Self {
        PdfFailure::File(e)
    }
}

impl From<PdfError> for PdfFailure {
    fn from(e: PdfError) -> Self {
        PdfFailure::Pdf(e)
    }
}

/// Engineering name of a PDF problem (doc/QML-API.md, `PdfDocument`).
pub fn pdf_error_name(e: PdfError) -> &'static str {
    match e {
        PdfError::Invalid => "invalid",
        PdfError::Encrypted => "encrypted",
        PdfError::Empty => "empty",
        PdfError::NoSuchPage => "noSuchPage",
        PdfError::TooLarge => "tooLarge",
    }
}

/// An open PDF as the viewer sees it.
#[derive(Debug, Clone)]
pub struct OpenedPdf {
    pub pages: Vec<PageSize>,
    pub size: Option<u64>,
    /// Where reading stopped last time (0-based, on a page it has).
    pub last_page: Option<usize>,
    /// The outline (bookmarks), depth first; empty when there is none.
    pub outline: Vec<OutlineItem>,
}

type Kept = Vec<(Uri, Arc<PdfDocument>)>;

static DOCUMENTS: OnceLock<Mutex<Kept>> = OnceLock::new();

fn documents() -> MutexGuard<'static, Kept> {
    let m = DOCUMENTS.get_or_init(Mutex::default);
    match m.lock() {
        Ok(g) => g,
        Err(p) => p.into_inner(),
    }
}

fn keep(uri: &Uri, doc: &Arc<PdfDocument>) {
    let mut kept = documents();
    kept.retain(|(u, _)| u != uri);
    kept.insert(0, (uri.clone(), Arc::clone(doc)));
    kept.truncate(KEPT_DOCUMENTS);
}

fn kept(uri: &Uri) -> Option<Arc<PdfDocument>> {
    documents()
        .iter()
        .find(|(u, _)| u == uri)
        .map(|(_, d)| Arc::clone(d))
}

/// Stops keeping this parse; a newer parse of its file stays.
fn forget(doc: &Arc<PdfDocument>) {
    documents().retain(|(_, d)| !Arc::ptr_eq(d, doc));
}

/// The hits of a search on one searched page (0-based), none or more.
#[derive(Debug, Clone, PartialEq)]
pub struct PageHits {
    pub page: usize,
    pub hits: Vec<TextHit>,
}

/// One area of a hit on its page (fractions of the shown page from its top
/// left corner, as [`PageRect`]) and whether the hit is the current one.
#[derive(Debug, Clone, Copy, PartialEq, Serialize)]
pub struct HitArea {
    pub x: f32,
    pub y: f32,
    pub width: f32,
    pub height: f32,
    pub current: bool,
}

/// What a search of a document has found so far, in page order, and the
/// hit being shown. Pages may arrive in any order.
#[derive(Debug, Default)]
pub struct PdfSearchResults {
    /// Only pages with hits.
    pages: BTreeMap<usize, Vec<TextHit>>,
    searched: usize,
    total: usize,
    /// The hit shown: its page and its index there.
    current: Option<(usize, usize)>,
}

impl PdfSearchResults {
    /// Adds a searched page. The first hit to arrive becomes the current
    /// one: [`Core::search_pdf`] starts at the page being read, so that is
    /// the first hit at or after it.
    pub fn add(&mut self, page: PageHits) {
        self.searched += 1;
        if page.hits.is_empty() {
            return;
        }
        self.total += page.hits.len();
        self.current.get_or_insert((page.page, 0));
        self.pages.insert(page.page, page.hits);
    }

    pub fn searched_pages(&self) -> usize {
        self.searched
    }

    pub fn hit_count(&self) -> usize {
        self.total
    }

    /// The current hit's number (0-based, in page order) and its page.
    pub fn current(&self) -> Option<(usize, usize)> {
        let (page, index) = self.current?;
        let before: usize = self.pages.range(..page).map(|(_, h)| h.len()).sum();
        Some((before + index, page))
    }

    /// The areas of the hits on `page`, in content order.
    pub fn areas_on(&self, page: usize) -> Vec<HitArea> {
        let Some(hits) = self.pages.get(&page) else {
            return Vec::new();
        };
        let current = self.current.filter(|(p, _)| *p == page).map(|(_, i)| i);
        hits.iter()
            .enumerate()
            .flat_map(|(i, hit)| {
                hit.areas.iter().map(move |a| HitArea {
                    x: a.x,
                    y: a.y,
                    width: a.width,
                    height: a.height,
                    current: current == Some(i),
                })
            })
            .collect()
    }

    /// Makes the next hit (`forward`) or the previous one current, going
    /// round from the last to the first and back.
    pub fn step(&mut self, forward: bool) {
        if let Some((page, index)) = self.current {
            self.current = if forward {
                self.after(page, index)
            } else {
                self.before(page, index)
            };
        }
    }

    fn after(&self, page: usize, index: usize) -> Option<(usize, usize)> {
        if self.pages.get(&page).is_some_and(|h| index + 1 < h.len()) {
            return Some((page, index + 1));
        }
        let mut later = self.pages.range(page + 1..);
        let (next, _) = later.next().or_else(|| self.pages.iter().next())?;
        Some((*next, 0))
    }

    fn before(&self, page: usize, index: usize) -> Option<(usize, usize)> {
        if index > 0 {
            return Some((page, index - 1));
        }
        let mut earlier = self.pages.range(..page);
        let (prev, hits) = earlier.next_back().or_else(|| self.pages.iter().next_back())?;
        // Kept pages have hits.
        Some((*prev, hits.len() - 1))
    }
}

/// Searches `doc` for `query` page by page from page `from` to the end and
/// then from the first page, passing each searched page to `send` (false:
/// nobody listens any more). Stops when `cancel` is set or after `limit`
/// hits.
fn search_pages(
    doc: &PdfDocument,
    query: &str,
    from: usize,
    limit: usize,
    cancel: &AtomicBool,
    mut send: impl FnMut(PageHits) -> bool,
) {
    let count = doc.page_count();
    let mut left = limit;
    for page in (0..count).map(|k| (from % count + k) % count) {
        if cancel.load(Ordering::Relaxed) {
            return;
        }
        // Only fails for a page the document does not have.
        let mut hits = doc.search_page(page, query).unwrap_or_default();
        hits.truncate(left);
        left -= hits.len();
        if !send(PageHits { page, hits }) || left == 0 {
            return;
        }
    }
}

/// Splits `<uri>?page=N` of the page provider (N counts from 0). The URI
/// may arrive percent-encoded once more (QML helpers).
pub fn parse_page_id(id: &str) -> Option<(Uri, usize)> {
    let (uri_part, query) = id.rsplit_once('?')?;
    let page = query.strip_prefix("page=")?.parse().ok()?;
    Some((crate::app_viewers::parse_image_uri(uri_part)?, page))
}

async fn blocking<T: Send + 'static>(f: impl FnOnce() -> T + Send + 'static) -> Result<T> {
    tokio::task::spawn_blocking(f)
        .await
        .map_err(|e| Error::new(ErrorKind::Internal, e.to_string()))
}

/// `work`, with a panic in it as [`PdfError::Invalid`]: the viewer shows
/// the file or the page as one it cannot read. `lost` runs after a panic.
fn caught<T>(
    work: impl FnOnce() -> std::result::Result<T, PdfError>,
    lost: impl FnOnce(),
) -> std::result::Result<T, PdfError> {
    catch(work).unwrap_or_else(|_| {
        lost();
        Err(PdfError::Invalid)
    })
}

/// Runs `work` on `doc` on the blocking pool. After a panic the document
/// is no longer kept: hayro unwraps its locks, so a lock the panic poisoned
/// would trip every later use; the next request parses the file again.
async fn run_on<T: Send + 'static>(
    doc: Arc<PdfDocument>,
    work: impl FnOnce(&PdfDocument) -> std::result::Result<T, PdfError> + Send + 'static,
) -> std::result::Result<T, PdfFailure> {
    Ok(blocking(move || caught(|| work(&doc), || forget(&doc))).await??)
}

impl Core {
    /// Reads and parses a PDF (always afresh, so a changed file is seen) and
    /// keeps it for [`Core::pdf_page`]. `password` opens an encrypted one
    /// (`None`: no password given); it is used for this parse only and never
    /// stored. Notes the PDF in Recents (ORG-2).
    pub async fn open_pdf(
        &self,
        uri: &Uri,
        password: Option<&str>,
    ) -> std::result::Result<OpenedPdf, PdfFailure> {
        let provider = self.provider(&uri.location)?;
        let entry = provider.stat(&uri.path, true, Lane::Interactive).await?;
        if entry.is_dir() {
            return Err(Error::kind(ErrorKind::IsADirectory).into());
        }
        if entry.size.is_some_and(|s| s > MAX_PDF_BYTES) {
            return Err(PdfError::TooLarge.into());
        }
        let handle = provider.open_read(&uri.path, Lane::Interactive).await?;
        // A file that grew past the limit since the stat is cut there and
        // then usually fails to parse; nothing larger is held in memory.
        let bytes = crate::provider::read_all(handle.as_ref(), MAX_PDF_BYTES).await?;
        let password = password.map(str::to_owned);
        let (doc, outline) = blocking(move || {
            let parse = || {
                match password {
                    Some(password) => PdfDocument::open_with_password(bytes, &password),
                    None => PdfDocument::open(bytes),
                }
                .map(|d| {
                    let outline = d.outline();
                    (Arc::new(d), outline)
                })
            };
            caught(parse, || ())
        })
        .await??;
        keep(uri, &doc);
        self.note_viewed(uri, RecentKind::Previewed);
        let last_page = self
            .recents
            .page(uri)
            .unwrap_or_else(|e| {
                log::debug!("reading position not read: {e}");
                None
            })
            .and_then(|p| usize::try_from(p).ok())
            .filter(|p| *p < doc.page_count());
        Ok(OpenedPdf {
            pages: doc.page_sizes().to_vec(),
            size: entry.size,
            last_page,
            outline,
        })
    }

    /// Remembers that reading `uri` stopped at `page` (0-based), with the
    /// recents (ORG-2). Blocking: call from a blocking thread.
    pub fn remember_pdf_page(&self, uri: &Uri, page: usize) {
        let page = u32::try_from(page).unwrap_or(u32::MAX);
        if let Err(e) = self.recents.set_page(uri, page) {
            log::debug!("reading position not saved: {e}");
        }
    }

    /// Page `page` (0-based) of a PDF, `width` pixels wide.
    pub async fn pdf_page(
        &self,
        uri: &Uri,
        page: usize,
        width: u32,
    ) -> std::result::Result<RenderedPage, PdfFailure> {
        self.on_document(uri, move |doc| doc.render(page, width)).await
    }

    /// Searches a PDF's text for `query` (case-insensitive, whitespace runs
    /// alike; `lautta-pdf`'s matching) page by page, from page `from`
    /// (0-based: the page being read) to the end and then from the first
    /// page, sending every searched page to `out` as it is done. Stops when
    /// `cancel` is set, when `out` is closed or after [`MAX_SEARCH_HITS`]
    /// hits. A blank query searches nothing. An encrypted PDF can be
    /// searched while it is kept after unlocking.
    pub async fn search_pdf(
        &self,
        uri: &Uri,
        query: &str,
        from: usize,
        cancel: Arc<AtomicBool>,
        out: mpsc::Sender<PageHits>,
    ) -> std::result::Result<(), PdfFailure> {
        if query.trim().is_empty() {
            return Ok(());
        }
        let query = query.to_owned();
        self.on_document(uri, move |doc| {
            search_pages(doc, &query, from, MAX_SEARCH_HITS, &cancel, |p| {
                out.blocking_send(p).is_ok()
            });
            Ok(())
        })
        .await
    }

    /// The links of page `page` (0-based) of a PDF: to its own pages and to
    /// http(s) URIs, nothing else.
    pub async fn pdf_links(&self, uri: &Uri, page: usize) -> std::result::Result<Vec<Link>, PdfFailure> {
        self.on_document(uri, move |doc| doc.links(page)).await
    }

    /// [`run_on`] with the kept document of `uri`, opened when not kept.
    async fn on_document<T: Send + 'static>(
        &self,
        uri: &Uri,
        work: impl FnOnce(&PdfDocument) -> std::result::Result<T, PdfError> + Send + 'static,
    ) -> std::result::Result<T, PdfFailure> {
        run_on(self.pdf_document(uri).await?, work).await
    }

    /// The kept document, opening the file when it is not kept (an
    /// encrypted one then fails: its password is not kept).
    async fn pdf_document(&self, uri: &Uri) -> std::result::Result<Arc<PdfDocument>, PdfFailure> {
        if let Some(doc) = kept(uri) {
            return Ok(doc);
        }
        self.open_pdf(uri, None).await?;
        kept(uri).ok_or(PdfFailure::File(Error::kind(ErrorKind::Internal)))
    }
}

#[cfg(test)]
mod tests {
    use super::{
        caught, forget, keep, kept, parse_page_id, pdf_error_name, run_on, search_pages, HitArea, PageHits,
        PageRect, PdfDocument, PdfError, PdfFailure, PdfSearchResults, TextHit,
    };
    use crate::uri::Uri;
    use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
    use std::sync::Arc;

    /// Tests that keep documents take turns: the kept list is the process's.
    static KEPT_LIST: tokio::sync::Mutex<()> = tokio::sync::Mutex::const_new(());

    /// A parse of a one-page PDF `width` points wide.
    fn parse(width: u32) -> Arc<PdfDocument> {
        use lautta_pdf::sample::{build, page};
        Arc::new(PdfDocument::open(build(&[page(width, 10, "")])).unwrap())
    }

    #[test]
    fn page_ids_parse() {
        let (u, p) = parse_page_id("lautta://nv-a/d/a%20b.pdf?page=3").unwrap();
        assert_eq!(u.to_string(), "lautta://nv-a/d/a%20b.pdf");
        assert_eq!(p, 3);
        let (u, p) = parse_page_id("lautta%3A%2F%2Fnv-a%2Fx.pdf?page=0").unwrap();
        assert_eq!((u.to_string().as_str(), p), ("lautta://nv-a/x.pdf", 0));
        assert!(parse_page_id("lautta://nv-a/x.pdf").is_none());
        assert!(parse_page_id("lautta://nv-a/x.pdf?size=3").is_none());
        assert!(parse_page_id("lautta://nv-a/x.pdf?page=-1").is_none());
        assert!(parse_page_id("nonsense?page=1").is_none());
    }

    #[test]
    fn problems_have_stable_names() {
        for (e, name) in [
            (PdfError::Invalid, "invalid"),
            (PdfError::Encrypted, "encrypted"),
            (PdfError::Empty, "empty"),
            (PdfError::NoSuchPage, "noSuchPage"),
            (PdfError::TooLarge, "tooLarge"),
        ] {
            assert_eq!(pdf_error_name(e), name);
        }
    }

    /// A hit with one area per entry of `xs`, at that x.
    fn hit(xs: &[f32]) -> TextHit {
        TextHit {
            areas: xs
                .iter()
                .map(|&x| PageRect {
                    x,
                    y: 0.5,
                    width: 0.1,
                    height: 0.2,
                })
                .collect(),
        }
    }

    fn page_hits(page: usize, xs: &[f32]) -> PageHits {
        PageHits {
            page,
            hits: xs.iter().map(|x| hit(&[*x])).collect(),
        }
    }

    /// Results of a search that went through pages 2, 3, 0 and 1: page 2 has
    /// two hits, page 0 one, the others none.
    fn results() -> PdfSearchResults {
        let mut r = PdfSearchResults::default();
        r.add(page_hits(2, &[0.1, 0.2]));
        r.add(page_hits(3, &[]));
        r.add(page_hits(0, &[0.3]));
        r.add(page_hits(1, &[]));
        r
    }

    /// The x of the current hit's areas on `page`.
    fn current_xs(r: &PdfSearchResults, page: usize) -> Vec<f32> {
        r.areas_on(page)
            .iter()
            .filter(|a| a.current)
            .map(|a| a.x)
            .collect()
    }

    #[test]
    fn results_count_pages_and_hits() {
        let r = results();
        assert_eq!((r.searched_pages(), r.hit_count()), (4, 3));
        assert_eq!(PdfSearchResults::default().current(), None);
    }

    #[test]
    fn the_first_hit_to_arrive_is_current_and_numbered_in_page_order() {
        let mut r = results();
        assert_eq!(r.current(), Some((1, 2)), "page 0's hit comes first");
        r.add(page_hits(5, &[0.9]));
        assert_eq!(r.current(), Some((1, 2)), "a later hit changes nothing");
        assert_eq!(current_xs(&r, 2), [0.1]);
        assert!(current_xs(&r, 0).is_empty());
    }

    #[test]
    fn stepping_goes_round() {
        let mut r = results();
        let mut seen = Vec::new();
        for _ in 0..4 {
            r.step(true);
            seen.push(r.current().unwrap());
        }
        assert_eq!(seen, [(2, 2), (0, 0), (1, 2), (2, 2)]);
        seen.clear();
        for _ in 0..4 {
            r.step(false);
            seen.push(r.current().unwrap());
        }
        assert_eq!(seen, [(1, 2), (0, 0), (2, 2), (1, 2)]);
        let mut none = PdfSearchResults::default();
        none.step(true);
        none.step(false);
        assert_eq!(none.current(), None);
    }

    #[test]
    fn stepping_crosses_pages_at_both_ends_of_a_page() {
        let mut r = PdfSearchResults::default();
        r.add(page_hits(1, &[0.1, 0.2, 0.3]));
        r.add(page_hits(4, &[0.4, 0.5]));
        r.add(page_hits(7, &[0.6]));
        for _ in 0..3 {
            r.step(true);
        }
        assert_eq!(current_xs(&r, 4), [0.4], "past the last hit of page 1");
        r.step(false);
        assert_eq!(current_xs(&r, 1), [0.3], "back to the last hit of page 1");
        for _ in 0..3 {
            r.step(true);
        }
        assert_eq!(current_xs(&r, 7), [0.6]);
        r.step(false);
        assert_eq!(current_xs(&r, 4), [0.5], "the last of the page before");
    }

    #[test]
    fn every_area_of_a_hit_is_listed_with_the_current_one_marked() {
        let mut r = PdfSearchResults::default();
        r.add(PageHits {
            page: 0,
            hits: vec![hit(&[0.1, 0.2]), hit(&[0.3])],
        });
        let areas = r.areas_on(0);
        assert_eq!(
            areas[0],
            HitArea {
                x: 0.1,
                y: 0.5,
                width: 0.1,
                height: 0.2,
                current: true
            }
        );
        let flags: Vec<(f32, bool)> = areas.iter().map(|a| (a.x, a.current)).collect();
        assert_eq!(flags, [(0.1, true), (0.2, true), (0.3, false)]);
        r.step(true);
        assert_eq!(current_xs(&r, 0), [0.3]);
        assert!(r.areas_on(1).is_empty());
    }

    /// Five pages; "apple" is on pages 0 (twice), 2 and 4.
    fn fruit() -> PdfDocument {
        use lautta_pdf::sample::{build, page, text};
        let shown = |s: &str| page(200, 100, &text(10, 50, s));
        PdfDocument::open(build(&[
            shown("apple and apple"),
            shown("pear"),
            shown("Apple"),
            shown("plum"),
            shown("apple pie"),
        ]))
        .unwrap()
    }

    /// The pages `search_pages` sends, with their hit counts; the receiver
    /// goes away after `listen` pages.
    fn searched(from: usize, limit: usize, listen: usize) -> Vec<(usize, usize)> {
        let mut sent = Vec::new();
        search_pages(&fruit(), "apple", from, limit, &AtomicBool::new(false), |p| {
            sent.push((p.page, p.hits.len()));
            sent.len() < listen
        });
        sent
    }

    #[test]
    fn the_search_starts_at_the_page_being_read_and_goes_round() {
        assert_eq!(searched(0, 100, 99), [(0, 2), (1, 0), (2, 1), (3, 0), (4, 1)]);
        assert_eq!(searched(3, 100, 99), [(3, 0), (4, 1), (0, 2), (1, 0), (2, 1)]);
        assert_eq!(searched(7, 100, 99)[0], (2, 1), "past the end counts round");
        assert_eq!(searched(usize::MAX, 100, 1), [(0, 2)], "without overflowing");
    }

    #[test]
    fn the_search_stops_at_the_limit_or_when_nobody_listens() {
        assert_eq!(searched(0, 3, 99), [(0, 2), (1, 0), (2, 1)]);
        assert_eq!(searched(0, 1, 99), [(0, 1)], "a page's hits are cut");
        assert_eq!(searched(1, 100, 2), [(1, 0), (2, 1)]);
    }

    #[test]
    fn a_cancelled_search_stops_before_the_next_page() {
        let cancel = AtomicBool::new(false);
        let mut sent = Vec::new();
        search_pages(&fruit(), "apple", 0, 100, &cancel, |p| {
            sent.push(p.page);
            cancel.store(true, Ordering::Relaxed);
            true
        });
        assert_eq!(sent, [0]);
    }

    #[test]
    fn two_documents_are_kept_newest_first() {
        let _turn = KEPT_LIST.blocking_lock();
        let doc = parse;
        let (a, b, c) = (
            Uri::parse("lautta://kept-test/a.pdf").unwrap(),
            Uri::parse("lautta://kept-test/b.pdf").unwrap(),
            Uri::parse("lautta://kept-test/c.pdf").unwrap(),
        );
        keep(&a, &doc(1));
        keep(&b, &doc(2));
        keep(&a, &doc(3));
        assert_eq!(
            kept(&a).unwrap().page_sizes()[0].width,
            3.0,
            "replaced, not added"
        );
        assert_eq!(kept(&b).unwrap().page_sizes()[0].width, 2.0);
        keep(&c, &doc(4));
        assert!(kept(&b).is_none(), "the oldest goes");
        assert!(kept(&a).is_some() && kept(&c).is_some());
    }

    #[test]
    fn a_panic_is_a_pdf_that_cannot_be_read() {
        let lost = AtomicUsize::new(0);
        let note = || {
            lost.fetch_add(1, Ordering::Relaxed);
        };
        assert_eq!(
            caught(|| -> Result<u8, PdfError> { panic!("hayro tripped") }, note),
            Err(PdfError::Invalid)
        );
        assert_eq!(lost.load(Ordering::Relaxed), 1, "told once");
        assert_eq!(caught(|| Ok(7), note), Ok(7));
        assert_eq!(
            caught(|| Err::<u8, _>(PdfError::NoSuchPage), note),
            Err(PdfError::NoSuchPage),
            "the work's own error is kept"
        );
        assert_eq!(lost.load(Ordering::Relaxed), 1, "only a panic is told");
    }

    #[test]
    fn forgetting_a_parse_leaves_the_others_and_a_newer_one() {
        let _turn = KEPT_LIST.blocking_lock();
        let a = Uri::parse("lautta://forget-test/a.pdf").unwrap();
        let b = Uri::parse("lautta://forget-test/b.pdf").unwrap();
        let old = parse(1);
        keep(&a, &old);
        keep(&b, &parse(2));
        forget(&old);
        assert!(kept(&a).is_none());
        assert!(kept(&b).is_some(), "other documents stay");
        let newer = parse(3);
        keep(&a, &newer);
        forget(&old);
        assert!(
            kept(&a).is_some_and(|d| Arc::ptr_eq(&d, &newer)),
            "a newer parse of the file stays"
        );
    }

    #[tokio::test]
    async fn a_panic_on_a_kept_document_is_an_error_and_forgets_it() {
        let _turn = KEPT_LIST.lock().await;
        let u = Uri::parse("lautta://forget-test/c.pdf").unwrap();
        let doc = parse(5);
        keep(&u, &doc);
        let res = run_on(Arc::clone(&doc), |_| -> Result<(), PdfError> {
            panic!("hayro tripped")
        })
        .await;
        assert_eq!(
            res,
            Err(PdfFailure::Pdf(PdfError::Invalid)),
            "what the viewer shows as unreadable"
        );
        assert!(kept(&u).is_none(), "parsed again on the next request");
        keep(&u, &doc);
        assert_eq!(run_on(Arc::clone(&doc), |d| Ok(d.page_count())).await, Ok(1));
        assert!(kept(&u).is_some(), "kept while nothing panics");
    }
}
