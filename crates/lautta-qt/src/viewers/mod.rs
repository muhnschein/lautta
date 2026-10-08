// SPDX-License-Identifier: LGPL-2.1-or-later
//! Viewers area facades (doc/QML-API.md, "Viewers"): text, Markdown and
//! EXIF sources, the folder's images, the image providers for
//! remote files and the media source for QtMultimedia. The logic lives in
//! `lautta_core::app_viewers`; these types only forward and publish.

pub mod docs;
pub mod exif;
pub mod images;
pub mod media;
pub mod tools;

use crate::runtime::spawn_then;
use lautta_core::{Result, Uri};
use qmetaobject::{QObject, QPointer, QString};
use std::future::Future;

/// Registers this area's QML types under `Lautta 1.0`.
pub fn register() {
    docs::register();
    exif::register();
    images::register();
    media::register();
    tools::register();
}

pub(crate) fn parse_uri(s: &QString) -> Option<Uri> {
    Uri::parse(&s.to_string()).ok()
}

pub(crate) fn qstr(s: &str) -> QString {
    QString::from(s)
}

/// Runs `fut` on the runtime and hands its result to `done` with the object
/// (when it still exists, UI-4: nothing blocks the GUI thread).
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
