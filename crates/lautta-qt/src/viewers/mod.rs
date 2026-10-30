// SPDX-License-Identifier: LGPL-2.1-or-later
//! Viewers facades. Logic lives in `lautta_core::app_viewers`; these types only forward and
//! publish.

pub mod docs;
pub mod exif;
pub mod images;
pub mod media;
pub mod pdf;
pub mod tools;

use crate::runtime::spawn_then;
use lautta_core::{Result, Uri};
use qmetaobject::{QObject, QPointer, QString};
use std::future::Future;

pub fn register() {
    docs::register();
    exif::register();
    images::register();
    media::register();
    pdf::register();
    tools::register();
}

pub(crate) fn parse_uri(s: &QString) -> Option<Uri> {
    Uri::parse(&s.to_string()).ok()
}

pub(crate) fn qstr(s: &str) -> QString {
    QString::from(s)
}

/// `done` gets the object only if it still exists; nothing blocks the GUI thread.
pub(crate) fn run_then<O, T, F>(me: &O, fut: F, done: impl FnOnce(&mut O, Result<T>) + 'static)
where
    O: QObject + 'static,
    T: Send + 'static,
    F: Future<Output = Result<T>> + Send + 'static,
{
    let ptr = QPointer::from(me);
    spawn_then(fut, move |res| {
        if let Some(pinned) = ptr.as_pinned() {
            done(&mut pinned.borrow_mut(), res);
        }
    });
}
