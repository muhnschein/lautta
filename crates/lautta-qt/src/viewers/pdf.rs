// SPDX-License-Identifier: LGPL-2.1-or-later
//! The PDF viewer (PRV-12): `PdfDocument` (page count and sizes of a PDF)
//! and the requests of `image://lautta-pdf/<uri>?page=N`, which render
//! one page at the width the `Image` asks for (`sourceSize.width`).
//! Rendering is `lautta_core::app_pdf`'s; nothing here reaches the network.

use super::images::{finished, on_cancel, Reply};
use super::{parse_uri, qstr, run_then};
use crate::runtime::{core, handle};
use lautta_core::app::Core;
use lautta_core::app_pdf::{parse_page_id, pdf_error_name, OpenedPdf, PageSize, PdfFailure};
use qmetaobject::prelude::*;
use qmetaobject::qml_register_type;
use std::sync::Arc;

pub fn register() {
    qml_register_type::<PdfDocument>(&crate::qml_uri(), 1, 0, &crate::cstr("PdfDocument"));
}

/// Width a page is rendered at when the `Image` gives no `sourceSize`.
pub const DEFAULT_PAGE_WIDTH: u32 = 1080;

/// A PDF: how many pages, and how tall each is for a given width.
#[derive(QObject, Default)]
pub struct PdfDocument {
    base: qt_base_class!(trait QObject),
    uri: qt_property!(QString; WRITE setUri NOTIFY uriChanged),
    uriChanged: qt_signal!(),
    loading: qt_property!(bool; NOTIFY stateChanged),
    loaded: qt_property!(bool; NOTIFY stateChanged),
    pageCount: qt_property!(i32; NOTIFY stateChanged),
    /// File size in bytes, -1 when unknown.
    size: qt_property!(f64; NOTIFY stateChanged),
    /// Core error kind when the file could not be read (ErrorText.js).
    errorKind: qt_property!(QString; NOTIFY stateChanged),
    errorMessage: qt_property!(QString; NOTIFY stateChanged),
    /// Why the file is no PDF the viewer can show: "invalid", "encrypted",
    /// "empty" or "tooLarge"; empty otherwise.
    problem: qt_property!(QString; NOTIFY stateChanged),
    stateChanged: qt_signal!(),

    /// Height of page `index` shown `width` wide (0 for no such page).
    heightFor: qt_method!(fn(&self, index: i32, width: f64) -> f64),
    reload: qt_method!(fn(&mut self)),

    pages: Vec<PageSize>,
    generation: u64,
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
        self.generation += 1;
        let gen = self.generation;
        self.clear();
        let (Some(core), Some(uri)) = (core(), parse_uri(&self.uri)) else {
            self.loading = false;
            self.stateChanged();
            return;
        };
        self.loading = true;
        self.stateChanged();
        run_then(
            self,
            async move { Ok(core.open_pdf(&uri).await) },
            move |me, res| {
                if me.generation == gen {
                    me.loading = false;
                    me.apply(res.unwrap_or_else(|e| Err(PdfFailure::File(e))));
                    me.stateChanged();
                }
            },
        );
    }

    fn apply(&mut self, res: Result<OpenedPdf, PdfFailure>) {
        match res {
            Ok(pdf) => {
                self.loaded = true;
                self.pageCount = i32::try_from(pdf.pages.len()).unwrap_or(i32::MAX);
                self.size = pdf.size.map_or(-1.0, |s| s as f64);
                self.pages = pdf.pages;
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
    }
}

fn failure_text(f: &PdfFailure) -> String {
    match f {
        PdfFailure::File(e) => format!("{}: {}", e.kind.name(), e.message),
        PdfFailure::Pdf(p) => p.to_string(),
    }
}

/// `image://lautta-pdf/<uri>?page=N` at `width` pixels (the request's
/// width; [`DEFAULT_PAGE_WIDTH`] when it gives none).
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
