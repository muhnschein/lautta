// SPDX-License-Identifier: LGPL-2.1-or-later
//! The `App` singleton (doc/QML-API.md): readiness, settings, clipboard,
//! undo, URI helpers and opening files outside the app.

use crate::json::{from_json, json_value_to_qvariant, to_json};
use crate::runtime::{core, spawn_then};
use lautta_core::clipboard::{Clipboard, ClipboardMode};
use lautta_core::mime::{category_of_name, extension_of, from_extension, viewer_for, FileCategory, Viewer};
use lautta_core::settings::Settings;
use lautta_core::undo::UndoAction;
use lautta_core::Uri;
use qmetaobject::prelude::*;
use qmetaobject::{QPointer, QSingletonInit};
use std::time::Duration;

/// How long the undo banner stays (OPS-9).
const UNDO_BANNER: Duration = Duration::from_secs(10);

#[derive(QObject, Default)]
pub struct App {
    base: qt_base_class!(trait QObject),
    ready: qt_property!(bool; NOTIFY ready_changed),
    ready_changed: qt_signal!(),
    startError: qt_property!(QString; NOTIFY ready_changed),
    version: qt_property!(QString; CONST),
    settingsJson: qt_property!(QString; NOTIFY settingsJsonChanged),
    settingsJsonChanged: qt_signal!(),
    canUndo: qt_property!(bool; NOTIFY undo_changed),
    undoText: qt_property!(QString; NOTIFY undo_changed),
    undo_changed: qt_signal!(),
    clipboardCount: qt_property!(i32; NOTIFY clipboard_changed),
    clipboardCut: qt_property!(bool; NOTIFY clipboard_changed),
    clipboard_changed: qt_signal!(),

    loadSettings: qt_method!(fn(&mut self, json: QString)),
    setting: qt_method!(fn(&self, key: QString) -> QVariant),
    setSetting: qt_method!(fn(&mut self, key: QString, value_json: QString) -> bool),

    childUri: qt_method!(fn(&self, parent: QString, name: QString) -> QString),
    parentUri: qt_method!(fn(&self, uri: QString) -> QString),
    nameOf: qt_method!(fn(&self, uri: QString) -> QString),
    displayAddress: qt_method!(fn(&self, uri: QString) -> QString),
    locationName: qt_method!(fn(&self, uri: QString) -> QString),
    isLocal: qt_method!(fn(&self, uri: QString) -> bool),
    localUrl: qt_method!(fn(&self, uri: QString) -> QString),
    categoryOf: qt_method!(fn(&self, name_or_uri: QString) -> QString),
    viewerFor: qt_method!(fn(&self, name_or_uri: QString, mime: QString) -> QString),

    copy: qt_method!(fn(&mut self, uris_json: QString) -> bool),
    cut: qt_method!(fn(&mut self, uris_json: QString) -> bool),
    clearClipboard: qt_method!(fn(&mut self)),
    canPasteInto: qt_method!(fn(&self, uri: QString) -> bool),
    clipboardJson: qt_method!(fn(&self) -> QString),
    takeClipboard: qt_method!(fn(&mut self) -> QString),

    noteUndo: qt_method!(fn(&mut self)),
    undo: qt_method!(fn(&mut self)),
    undone: qt_signal!(ok: bool),

    prepareExternal: qt_method!(fn(&self, uri: QString)),
    externalReady: qt_signal!(uri: QString, fileUrl: QString),
    externalFailed: qt_signal!(uri: QString, kind: QString, message: QString),

    clipboard: Clipboard,
    undo_generation: u64,
}

impl QSingletonInit for App {
    fn init(&mut self) {
        self.version = QString::from(env!("CARGO_PKG_VERSION"));
        match core() {
            Some(core) => {
                self.ready = true;
                self.settingsJson = QString::from(to_json(&core.settings().to_map()).as_str());
            }
            None => {
                self.startError = QString::from(crate::runtime::start_error().as_str());
            }
        }
    }
}

fn parse_uri(s: &QString) -> Option<Uri> {
    Uri::parse(&s.to_string()).ok()
}

fn parse_uris(json: &QString) -> Option<Vec<Uri>> {
    let list: Vec<String> = from_json(&json.to_string())?;
    list.iter().map(|s| Uri::parse(s).ok()).collect()
}

/// Engineering names of the viewers (doc/QML-API.md, `App.viewerFor`).
pub fn viewer_name(v: Viewer) -> &'static str {
    match v {
        Viewer::Image => "image",
        Viewer::Text => "text",
        Viewer::Markdown => "markdown",
        Viewer::Audio => "audio",
        Viewer::Video => "video",
        Viewer::Archive => "archive",
        Viewer::External => "external",
    }
}

/// The last path component of a URI, or the text itself when it is a name.
fn name_part(s: &str) -> Vec<u8> {
    match Uri::parse(s) {
        Ok(u) => u.name().map(<[u8]>::to_vec).unwrap_or_default(),
        Err(_) => s.as_bytes().to_vec(),
    }
}

impl App {
    fn loadSettings(&mut self, json: QString) {
        let Some(core) = core() else { return };
        let map = from_json(&json.to_string()).unwrap_or_default();
        core.apply_settings(Settings::from_map(&map));
        self.publish_settings();
    }

    fn publish_settings(&mut self) {
        if let Some(core) = core() {
            self.settingsJson = QString::from(to_json(&core.settings().to_map()).as_str());
            self.settingsJsonChanged();
        }
    }

    fn setting(&self, key: QString) -> QVariant {
        let Some(core) = core() else {
            return QVariant::default();
        };
        core.settings()
            .to_map()
            .get(&key.to_string())
            .map(json_value_to_qvariant)
            .unwrap_or_default()
    }

    fn setSetting(&mut self, key: QString, value_json: QString) -> bool {
        let Some(core) = core() else { return false };
        let Some(value) = from_json::<serde_json::Value>(&value_json.to_string()) else {
            return false;
        };
        let mut settings = core.settings();
        if !settings.set(&key.to_string(), value) {
            return false;
        }
        core.apply_settings(settings);
        self.publish_settings();
        true
    }

    fn childUri(&self, parent: QString, name: QString) -> QString {
        parse_uri(&parent)
            .and_then(|p| p.join(name.to_string().as_bytes()).ok())
            .map(|u| QString::from(u.to_string().as_str()))
            .unwrap_or_default()
    }

    fn parentUri(&self, uri: QString) -> QString {
        parse_uri(&uri)
            .and_then(|u| u.parent())
            .map(|u| QString::from(u.to_string().as_str()))
            .unwrap_or_default()
    }

    fn nameOf(&self, uri: QString) -> QString {
        let Some(u) = parse_uri(&uri) else {
            return QString::default();
        };
        match u.name() {
            Some(n) => QString::from(String::from_utf8_lossy(n).as_ref()),
            None => self.locationName(uri),
        }
    }

    fn displayAddress(&self, uri: QString) -> QString {
        match (core(), parse_uri(&uri)) {
            (Some(core), Some(u)) => QString::from(core.locations.display_address(&u).as_str()),
            _ => QString::default(),
        }
    }

    fn locationName(&self, uri: QString) -> QString {
        let loc = parse_uri(&uri)
            .map(|u| u.location)
            .unwrap_or_else(|| uri.to_string());
        core()
            .and_then(|c| c.location(&loc))
            .map(|l| QString::from(l.name.as_str()))
            .unwrap_or_default()
    }

    fn isLocal(&self, uri: QString) -> bool {
        matches!((core(), parse_uri(&uri)), (Some(c), Some(u)) if c.locations.to_local_path(&u).is_some())
    }

    fn localUrl(&self, uri: QString) -> QString {
        match (core(), parse_uri(&uri)) {
            (Some(c), Some(u)) => c
                .locations
                .to_local_path(&u)
                .map(|p| QString::from(format!("file://{}", p.display()).as_str()))
                .unwrap_or_default(),
            _ => QString::default(),
        }
    }

    fn categoryOf(&self, name_or_uri: QString) -> QString {
        QString::from(category_of_name(&name_part(&name_or_uri.to_string())).icon_name())
    }

    fn viewerFor(&self, name_or_uri: QString, mime: QString) -> QString {
        let name = name_part(&name_or_uri.to_string());
        let category = category_of_name(&name);
        let mime = if mime.is_empty() {
            extension_of(&name)
                .and_then(|e| from_extension(&e))
                .map(|(m, _)| m.to_owned())
                .unwrap_or_default()
        } else {
            mime.to_string()
        };
        let category = if category == FileCategory::Other && !mime.is_empty() {
            lautta_core::mime::category_from_mime(&mime)
        } else {
            category
        };
        QString::from(viewer_name(viewer_for(category, &mime)))
    }

    fn set_clipboard(&mut self, mode: ClipboardMode, uris_json: QString) -> bool {
        let Some(uris) = parse_uris(&uris_json) else {
            return false;
        };
        let ok = self.clipboard.set(mode, uris).is_ok();
        self.publish_clipboard();
        ok
    }

    fn publish_clipboard(&mut self) {
        let (count, cut) = match self.clipboard.contents() {
            Some(c) => (c.items.len() as i32, c.mode == ClipboardMode::Cut),
            None => (0, false),
        };
        self.clipboardCount = count;
        self.clipboardCut = cut;
        self.clipboard_changed();
    }

    fn copy(&mut self, uris_json: QString) -> bool {
        self.set_clipboard(ClipboardMode::Copy, uris_json)
    }

    fn cut(&mut self, uris_json: QString) -> bool {
        self.set_clipboard(ClipboardMode::Cut, uris_json)
    }

    fn clearClipboard(&mut self) {
        self.clipboard.clear();
        self.publish_clipboard();
    }

    fn canPasteInto(&self, uri: QString) -> bool {
        parse_uri(&uri).is_some_and(|u| !self.clipboard.is_empty() && self.clipboard.can_paste_into(&u))
    }

    fn clipboardJson(&self) -> QString {
        let items: Vec<String> = self
            .clipboard
            .contents()
            .map(|c| c.items.iter().map(Uri::to_string).collect())
            .unwrap_or_default();
        QString::from(to_json(&items).as_str())
    }

    /// The items to paste and whether they move; a cut is consumed (OPS-10).
    fn takeClipboard(&mut self) -> QString {
        let taken = self.clipboard.take();
        self.publish_clipboard();
        let value = taken.map(|c| {
            serde_json::json!({
                "cut": c.mode == ClipboardMode::Cut,
                "items": c.items.iter().map(Uri::to_string).collect::<Vec<_>>(),
            })
        });
        QString::from(to_json(&value).as_str())
    }

    /// Called after an undoable action finished: shows the banner for 10 s.
    fn noteUndo(&mut self) {
        let Some(core) = core() else { return };
        self.canUndo = core.can_undo();
        self.undoText = QString::from(match core.undo_kind() {
            Some(UndoAction::Rename { .. }) => "rename",
            Some(UndoAction::Move { .. }) => "move",
            Some(UndoAction::Trash { .. }) => "trash",
            None => "",
        });
        self.undo_changed();
        self.undo_generation += 1;
        let generation = self.undo_generation;
        let me = QPointer::from(&*self);
        qmetaobject::single_shot(UNDO_BANNER, move || {
            if let Some(app) = me.as_pinned() {
                let mut app = app.borrow_mut();
                if app.undo_generation == generation {
                    app.canUndo = false;
                    app.undo_changed();
                }
            }
        });
    }

    fn undo(&mut self) {
        let Some(core) = core() else { return };
        self.canUndo = false;
        self.undo_changed();
        let me = QPointer::from(&*self);
        spawn_then(async move { core.undo().await }, move |res| {
            if let Some(app) = me.as_pinned() {
                app.borrow().undone(matches!(res, Ok(true)));
            }
        });
    }

    /// Makes a file readable by other apps: local files as they are, remote
    /// ones copied to ~/Downloads/Lautta/Opened first (PRV-6).
    fn prepareExternal(&self, uri: QString) {
        let (Some(core), Some(u)) = (core(), parse_uri(&uri)) else {
            return;
        };
        let me = QPointer::from(self);
        let key = uri.clone();
        spawn_then(
            async move {
                if let Some(path) = core.locations.to_local_path(&u) {
                    return Ok(path);
                }
                let provider = core.provider(&u.location)?;
                let copy = core.working_copies.open_for_view(provider.as_ref(), &u).await?;
                Ok::<_, lautta_core::Error>(copy.local_path)
            },
            move |res| {
                let Some(app) = me.as_pinned() else { return };
                let app = app.borrow();
                match res {
                    Ok(path) => {
                        app.externalReady(key, QString::from(format!("file://{}", path.display()).as_str()))
                    }
                    Err(e) => app.externalFailed(
                        key,
                        QString::from(e.kind.name()),
                        QString::from(e.message.as_str()),
                    ),
                }
            },
        );
    }
}
