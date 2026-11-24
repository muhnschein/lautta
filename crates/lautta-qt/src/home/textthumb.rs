// SPDX-License-Identifier: LGPL-2.1-or-later

use crate::directory::cancel_text;
use crate::runtime::{core, handle, spawn_then};
use lautta_core::Uri;
use qmetaobject::prelude::*;
use qmetaobject::QPointer;

#[derive(QObject, Default)]
pub struct TextThumb {
    base: qt_base_class!(trait QObject),
    uri: qt_property!(QString; NOTIFY uriChanged WRITE set_uri),
    uriChanged: qt_signal!(),
    text: qt_property!(QString; NOTIFY textChanged),
    textChanged: qt_signal!(),
    pending: Option<String>,
}

impl TextThumb {
    fn set_uri(&mut self, uri: QString) {
        if self.uri == uri {
            return;
        }
        self.uri = uri;
        self.uriChanged();
        self.cancel();
        self.show(None);
        self.ask();
    }

    fn ask(&mut self) {
        let (Some(core), Ok(uri)) = (core(), Uri::parse(&self.uri.to_string())) else {
            return;
        };
        let Ok(previews) = core.previews(&handle()) else {
            return;
        };
        let ticket = previews.request_text(core, uri);
        let key = ticket.key.clone();
        self.pending = Some(ticket.key);
        let me = QPointer::from(&*self);
        spawn_then(async move { ticket.rx.await.ok().flatten() }, move |text| {
            if let Some(thumb) = me.as_pinned() {
                thumb.borrow_mut().made(&key, text);
            }
        });
    }

    fn made(&mut self, key: &str, text: Option<String>) {
        if self.pending.as_deref() == Some(key) {
            self.pending = None;
            self.show(text);
        }
    }

    fn show(&mut self, text: Option<String>) {
        let text = QString::from(text.unwrap_or_default().as_str());
        if self.text != text {
            self.text = text;
            self.textChanged();
        }
    }

    fn cancel(&mut self) {
        if let Some(key) = self.pending.take() {
            cancel_text(&key);
        }
    }
}

impl Drop for TextThumb {
    fn drop(&mut self) {
        self.cancel();
    }
}
