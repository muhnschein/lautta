// SPDX-License-Identifier: LGPL-2.1-or-later
//! `PdfDocument` and requests of `image://lautta-pdf/<uri>?page=N`, which render one page at the
//! `Image`'s `sourceSize.width`. Rendering and searching are `lautta_core::app_pdf`'s; nothing here
//! reaches the network.

use super::images::{finished, on_cancel, Reply};
use super::{parse_uri, qstr, run_then};
use crate::json::to_json;
use crate::runtime::{core, handle};
use lautta_core::app::Core;
use lautta_core::app_pdf::{
    parse_page_id, pdf_error_name, Link, LinkTarget, OpenedPdf, OutlineItem, PageHits, PageSize, PdfError,
    PdfFailure, PdfSearchResults,
};
use qmetaobject::prelude::*;
use qmetaobject::{qml_register_type, queued_callback, QPointer};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;

pub fn register() {
    qml_register_type::<PdfDocument>(&crate::qml_uri(), 1, 0, &crate::cstr("PdfDocument"));
}

pub const DEFAULT_PAGE_WIDTH: u32 = 1080;

#[derive(QObject, Default)]
pub struct PdfDocument {
    base: qt_base_class!(trait QObject),
    uri: qt_property!(QString; WRITE setUri NOTIFY uriChanged),
    uriChanged: qt_signal!(),
    loading: qt_property!(bool; NOTIFY stateChanged),
    loaded: qt_property!(bool; NOTIFY stateChanged),
    pageCount: qt_property!(i32; NOTIFY stateChanged),
    size: qt_property!(f64; NOTIFY stateChanged),
    /// Core error kind when the file could not be read.
    errorKind: qt_property!(QString; NOTIFY stateChanged),
    errorMessage: qt_property!(QString; NOTIFY stateChanged),
    /// "invalid", "encrypted", "empty" or "tooLarge"; empty otherwise.
    problem: qt_property!(QString; NOTIFY stateChanged),
    /// Last `unlock` password did not open the PDF.
    wrongPassword: qt_property!(bool; NOTIFY stateChanged),
    /// 0-based; -1 when unknown.
    lastPage: qt_property!(i32; NOTIFY stateChanged),
    /// `[{title, level, page}]`, depth first; `page` from 0, -1 when the item goes nowhere in this
    /// document.
    outline: qt_property!(QString; NOTIFY stateChanged),
    stateChanged: qt_signal!(),
    /// After `requestLinks`; `[]` when none or unreadable.
    linksReady: qt_signal!(linkPage: i32, pageLinks: QString),

    heightFor: qt_method!(fn(&self, index: i32, width: f64) -> f64),
    reload: qt_method!(fn(&mut self)),
    /// Password is not kept.
    unlock: qt_method!(fn(&mut self, password: QString)),
    rememberPage: qt_method!(fn(&self, index: i32)),
    requestLinks: qt_method!(fn(&self, index: i32)),

    searching: qt_property!(bool; NOTIFY searchChanged),
    /// All pages once a search has run through.
    searchedPages: qt_property!(i32; NOTIFY searchChanged),
    hitCount: qt_property!(i32; NOTIFY searchChanged),
    /// From 1 in page order; 0 when none.
    currentHit: qt_property!(i32; NOTIFY searchChanged),
    /// 0-based; -1 when none.
    currentHitPage: qt_property!(i32; NOTIFY searchChanged),
    /// Changes when hits arrive or another hit is shown, so `hitsJson` bindings re-evaluate.
    hitsRevision: qt_property!(i32; NOTIFY searchChanged),
    searchChanged: qt_signal!(),

    /// From `fromPage` (0-based, the page being read) to the end, then from the first. Earlier hits
    /// are forgotten; blank query searches nothing.
    search: qt_method!(fn(&mut self, query: QString, fromPage: i32)),
    stopSearch: qt_method!(fn(&mut self)),
    nextHit: qt_method!(fn(&mut self)),
    previousHit: qt_method!(fn(&mut self)),
    /// JSON array of `x`, `y`, `width`, `height` (fractions of the page from top left) and
    /// `current` (part of the hit shown).
    hitsJson: qt_method!(fn(&self, index: i32) -> QString),

    pages: Vec<PageSize>,
    generation: u64,
    results: PdfSearchResults,
    search_generation: u64,
    search_cancel: Option<Arc<AtomicBool>>,
}

impl Drop for PdfDocument {
    fn drop(&mut self) {
        if let Some(flag) = &self.search_cancel {
            flag.store(true, Ordering::Relaxed);
        }
    }
}

impl PdfDocument {
    fn setUri(&mut self, value: QString) {
        if self.uri == value {
            return;
        }
        self.uri = value;
        self.uriChanged();
        self.reload();
    }

    fn heightFor(&self, index: i32, width: f64) -> f64 {
        usize::try_from(index)
            .ok()
            .and_then(|i| self.pages.get(i))
            .map_or(0.0, |p| width * f64::from(p.height) / f64::from(p.width.max(1.0)))
    }

    fn reload(&mut self) {
        self.open(String::new());
    }

    fn unlock(&mut self, password: QString) {
        self.open(password.to_string());
    }

    fn open(&mut self, password: String) {
        self.generation += 1;
        let gen = self.generation;
        self.clear();
        self.stop();
        self.publish();
        let (Some(core), Some(uri)) = (core(), parse_uri(&self.uri)) else {
            self.loading = false;
            self.stateChanged();
            return;
        };
        let attempt = !password.is_empty();
        self.loading = true;
        self.stateChanged();
        run_then(
            self,
            async move { Ok(core.open_pdf(&uri, attempt.then_some(password.as_str())).await) },
            move |me, res| {
                if me.generation == gen {
                    me.loading = false;
                    me.apply(res.unwrap_or_else(|e| Err(PdfFailure::File(e))), attempt);
                    me.stateChanged();
                }
            },
        );
    }

    fn rememberPage(&self, index: i32) {
        let (Some(core), Some(uri), Ok(page)) = (core(), parse_uri(&self.uri), usize::try_from(index)) else {
            return;
        };
        if !self.loaded || page >= self.pages.len() {
            return;
        }
        handle().spawn_blocking(move || core.remember_pdf_page(&uri, page));
    }

    fn requestLinks(&self, index: i32) {
        let (Some(core), Some(uri), Ok(page)) = (core(), parse_uri(&self.uri), usize::try_from(index)) else {
            return;
        };
        if !self.loaded || page >= self.pages.len() {
            return;
        }
        let gen = self.generation;
        run_then(
            self,
            async move { Ok(core.pdf_links(&uri, page).await) },
            move |me, res| {
                if me.generation != gen {
                    return;
                }
                let links = match res {
                    Ok(Ok(links)) => links,
                    Ok(Err(f)) => {
                        log::debug!("links of page {page} not read: {}", failure_text(&f));
                        Vec::new()
                    }
                    Err(e) => {
                        log::debug!("links of page {page} not read: {e}");
                        Vec::new()
                    }
                };
                me.linksReady(index, qstr(&links_json(&links)));
            },
        );
    }

    fn apply(&mut self, res: Result<OpenedPdf, PdfFailure>, attempt: bool) {
        match res {
            Ok(pdf) => {
                self.loaded = true;
                self.pageCount = i32::try_from(pdf.pages.len()).unwrap_or(i32::MAX);
                self.size = pdf.size.map_or(-1.0, |s| s as f64);
                self.lastPage = pdf.last_page.and_then(|p| i32::try_from(p).ok()).unwrap_or(-1);
                self.outline = qstr(&outline_json(&pdf.outline));
                self.pages = pdf.pages;
            }
            Err(PdfFailure::Pdf(PdfError::Encrypted)) => {
                self.problem = qstr(pdf_error_name(PdfError::Encrypted));
                self.wrongPassword = attempt;
            }
            Err(PdfFailure::File(e)) => {
                self.errorKind = qstr(e.kind.name());
                self.errorMessage = qstr(&e.message);
            }
            Err(PdfFailure::Pdf(p)) => self.problem = qstr(pdf_error_name(p)),
        }
    }

    fn clear(&mut self) {
        self.loaded = false;
        self.pageCount = 0;
        self.size = -1.0;
        self.pages.clear();
        self.errorKind = QString::default();
        self.errorMessage = QString::default();
        self.problem = QString::default();
        self.wrongPassword = false;
        self.lastPage = -1;
        self.outline = qstr("[]");
    }
}

/// Results published as they arrive. Late results of a stopped or replaced search are ignored.
impl PdfDocument {
    fn search(&mut self, query: QString, fromPage: i32) {
        self.stop();
        let query = query.to_string();
        let (Some(core), Some(uri)) = (core(), parse_uri(&self.uri)) else {
            self.publish();
            return;
        };
        if !self.loaded || query.trim().is_empty() {
            self.publish();
            return;
        }
        self.searching = true;
        self.publish();
        let cancel = Arc::new(AtomicBool::new(false));
        self.search_cancel = Some(Arc::clone(&cancel));
        let gen = self.search_generation;
        let from = usize::try_from(fromPage).unwrap_or(0);
        let me = QPointer::from(&*self);
        let post = queued_callback(move |page: Option<PageHits>| {
            if let Some(doc) = me.as_pinned() {
                doc.borrow_mut().found(gen, page);
            }
        });
        handle().spawn(async move {
            let (tx, mut rx) = tokio::sync::mpsc::channel(8);
            let search = tokio::spawn(async move { core.search_pdf(&uri, &query, from, cancel, tx).await });
            while let Some(page) = rx.recv().await {
                post(Some(page));
            }
            if let Ok(Err(f)) = search.await {
                log::debug!("PDF search failed: {}", failure_text(&f));
            }
            post(None);
        });
    }

    fn found(&mut self, gen: u64, page: Option<PageHits>) {
        if gen != self.search_generation {
            return;
        }
        match page {
            Some(p) => self.results.add(p),
            None => {
                self.searching = false;
                self.search_cancel = None;
            }
        }
        self.publish();
    }

    fn stopSearch(&mut self) {
        self.stop();
        self.publish();
    }

    fn nextHit(&mut self) {
        self.results.step(true);
        self.publish();
    }

    fn previousHit(&mut self) {
        self.results.step(false);
        self.publish();
    }

    fn hitsJson(&self, index: i32) -> QString {
        let areas = usize::try_from(index)
            .map(|i| self.results.areas_on(i))
            .unwrap_or_default();
        qstr(&to_json(&areas))
    }

    /// Late results are ignored; [`Self::publish`] shows the change.
    fn stop(&mut self) {
        if let Some(flag) = self.search_cancel.take() {
            flag.store(true, Ordering::Relaxed);
        }
        self.search_generation += 1;
        self.searching = false;
        self.results = PdfSearchResults::default();
    }

    fn publish(&mut self) {
        let count = |n: usize| i32::try_from(n).unwrap_or(i32::MAX);
        let current = self.results.current();
        self.searchedPages = count(self.results.searched_pages());
        self.hitCount = count(self.results.hit_count());
        self.currentHit = current.map_or(0, |(n, _)| count(n + 1));
        self.currentHitPage = current.map_or(-1, |(_, p)| count(p));
        self.hitsRevision = self.hitsRevision.checked_add(1).unwrap_or(0);
        self.searchChanged();
    }
}

fn page_number(page: Option<usize>) -> i64 {
    page.and_then(|p| i64::try_from(p).ok()).unwrap_or(-1)
}

fn outline_json(items: &[OutlineItem]) -> String {
    let rows: Vec<_> = items
        .iter()
        .map(|i| serde_json::json!({ "title": i.title, "level": i.level, "page": page_number(i.page) }))
        .collect();
    to_json(&rows)
}

/// Area as fractions of the shown page. In-document link: `page` (from 0), `top` (fraction of
/// target page height, -1 when not given), empty `uri`. Web link: `page` -1 and http(s) `uri`.
fn links_json(links: &[Link]) -> String {
    let rows: Vec<_> = links
        .iter()
        .map(|l| {
            let (page, top, uri) = match &l.target {
                LinkTarget::Page { index, top } => {
                    (page_number(Some(*index)), top.map_or(-1.0, f64::from), "")
                }
                LinkTarget::Uri(uri) => (-1, -1.0, uri.as_str()),
            };
            serde_json::json!({
                "x": l.area.x, "y": l.area.y, "width": l.area.width, "height": l.area.height,
                "page": page, "top": top, "uri": uri,
            })
        })
        .collect();
    to_json(&rows)
}

fn failure_text(f: &PdfFailure) -> String {
    match f {
        PdfFailure::File(e) => format!("{}: {}", e.kind.name(), e.message),
        PdfFailure::Pdf(p) => p.to_string(),
    }
}

/// [`DEFAULT_PAGE_WIDTH`] when the request gives no width.
pub(crate) fn page_request(core: Arc<Core>, id: &str, width: i32, req: u64, reply: Reply) {
    let Some((uri, page)) = parse_page_id(id) else {
        reply.fail("bad page id");
        return;
    };
    let width = u32::try_from(width)
        .ok()
        .filter(|w| *w > 0)
        .unwrap_or(DEFAULT_PAGE_WIDTH);
    let task = handle().spawn(async move {
        let res = core.pdf_page(&uri, page, width).await;
        finished(req);
        match res {
            Ok(p) => {
                let _ =
                    tokio::task::spawn_blocking(move || reply.done_rgba(&p.rgba, p.width, p.height)).await;
            }
            Err(f) => reply.fail(&failure_text(&f)),
        }
    });
    let abort = task.abort_handle();
    on_cancel(req, Box::new(move || abort.abort()));
}
