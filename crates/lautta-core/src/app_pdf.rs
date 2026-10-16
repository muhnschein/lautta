// SPDX-License-Identifier: LGPL-2.1-or-later
//! The PDF viewer's actions on [`Core`] (PRV-12): opening a PDF from any
//! location, rendering its pages for `image://lautta-pdf/` and reading its
//! outline and links. Parsing and
//! drawing are `lautta-pdf`'s; this module reads the file, keeps the open
//! documents and moves the CPU work off the runtime's threads.

use crate::app::Core;
use crate::error::{Error, ErrorKind, Result};
use crate::org::recents::RecentKind;
use crate::provider::Lane;
use crate::uri::Uri;
use lautta_pdf::{PdfDocument, RenderedPage, MAX_PDF_BYTES};
use std::sync::{Arc, Mutex, MutexGuard, OnceLock};

pub use lautta_pdf::{Link, LinkTarget, OutlineItem, PageRect, PageSize, PdfError};

/// How many documents stay parsed: the one shown and the one before it.
const KEPT_DOCUMENTS: usize = 2;

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

impl Core {
    /// Reads and parses a PDF (always afresh, so a changed file is seen) and
    /// keeps it for [`Core::pdf_page`]. `password` opens an encrypted one
    /// (empty: none); it is used for this parse only and never stored. Notes
    /// the PDF in Recents (ORG-2).
    pub async fn open_pdf(&self, uri: &Uri, password: &str) -> std::result::Result<OpenedPdf, PdfFailure> {
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
        let password = password.to_owned();
        let (doc, outline) = blocking(move || {
            PdfDocument::open_with_password(bytes, &password).map(|d| {
                let outline = d.outline();
                (Arc::new(d), outline)
            })
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
        let doc = self.pdf_document(uri).await?;
        Ok(blocking(move || doc.render(page, width)).await??)
    }

    /// The links of page `page` (0-based) of a PDF: to its own pages and to
    /// http(s) URIs, nothing else.
    pub async fn pdf_links(&self, uri: &Uri, page: usize) -> std::result::Result<Vec<Link>, PdfFailure> {
        let doc = self.pdf_document(uri).await?;
        Ok(blocking(move || doc.links(page)).await??)
    }

    /// The kept document, opening the file when it is not kept (an
    /// encrypted one then fails: its password is not kept).
    async fn pdf_document(&self, uri: &Uri) -> std::result::Result<Arc<PdfDocument>, PdfFailure> {
        if let Some(doc) = kept(uri) {
            return Ok(doc);
        }
        self.open_pdf(uri, "").await?;
        kept(uri).ok_or(PdfFailure::File(Error::kind(ErrorKind::Internal)))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

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

    #[test]
    fn two_documents_are_kept_newest_first() {
        let doc = |w| {
            Arc::new(
                PdfDocument::open(lautta_pdf::sample::build(&[lautta_pdf::sample::page(w, 10, "")])).unwrap(),
            )
        };
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
}
