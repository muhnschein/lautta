// SPDX-License-Identifier: LGPL-2.1-or-later
//! `TextDocument` and `MarkdownDocument` (PRV-4).

use super::{parse_uri, qstr, run_then};
use crate::runtime::core;
use lautta_core::app_viewers::ViewedText;
use lautta_core::org::recents::RecentKind;
use lautta_core::preview::highlight::{ColourJob, ColouredText, Palette};
use lautta_core::Error;
use qmetaobject::prelude::*;
use qmetaobject::qml_register_type;

pub fn register() {
    qml_register_type::<TextDocument>(&crate::qml_uri(), 1, 0, &crate::cstr("TextDocument"));
    qml_register_type::<MarkdownDocument>(&crate::qml_uri(), 1, 0, &crate::cstr("MarkdownDocument"));
}

/// A text file: `text` is what the viewer shows (LF breaks, no final break).
#[derive(QObject, Default)]
pub struct TextDocument {
    base: qt_base_class!(trait QObject),
    uri: qt_property!(QString; WRITE setUri NOTIFY uriChanged),
    uriChanged: qt_signal!(),
    loading: qt_property!(bool; NOTIFY stateChanged),
    loaded: qt_property!(bool; NOTIFY stateChanged),
    text: qt_property!(QString; NOTIFY stateChanged),
    /// Only the first MiB is shown (PRV-4).
    truncated: qt_property!(bool; NOTIFY stateChanged),
    validUtf8: qt_property!(bool; NOTIFY stateChanged),
    /// File size in bytes, -1 when unknown.
    size: qt_property!(f64; NOTIFY stateChanged),
    errorKind: qt_property!(QString; NOTIFY stateChanged),
    errorMessage: qt_property!(QString; NOTIFY stateChanged),
    /// The grammar of the colours, "" without (PRV-13).
    syntax: qt_property!(QString; NOTIFY stateChanged),
    /// The first lines (or all) have colours; they may come after the text.
    highlighted: qt_property!(bool; NOTIFY stateChanged),
    stateChanged: qt_signal!(),

    reload: qt_method!(fn(&mut self)),
    /// Line `index` (from 0) as StyledText: every character of the file
    /// escaped, whitespace kept, its pieces coloured with `colours`
    /// (`class=#rrggbb,…`, components/viewers/SyntaxColours.js); "" past
    /// the end (PRV-13).
    styledLine: qt_method!(fn(&self, index: i32, colours: QString) -> QString),

    generation: u64,
    lines: ColouredText,
}

impl TextDocument {
    fn setUri(&mut self, value: QString) {
        if self.uri == value {
            return;
        }
        self.uri = value;
        self.uriChanged();
        self.reload();
    }

    fn reload(&mut self) {
        self.generation += 1;
        let gen = self.generation;
        let (Some(core), Some(uri)) = (core(), parse_uri(&self.uri)) else {
            self.clear_content();
            self.loading = false;
            self.stateChanged();
            return;
        };
        self.loading = true;
        self.clear_error();
        self.stateChanged();
        let seen = uri.clone();
        let c = core.clone();
        run_then(self, async move { core.view_text(&uri).await }, move |me, res| {
            if me.generation == gen {
                me.loaded_with(res, &c, &seen);
            }
        });
    }

    fn loaded_with(
        &mut self,
        res: lautta_core::Result<ViewedText>,
        core: &lautta_core::app::Core,
        uri: &lautta_core::Uri,
    ) {
        self.loading = false;
        match res {
            Ok(v) => {
                if let Some(job) = self.apply(v) {
                    self.colour_later(job);
                }
                core.note_viewed(uri, RecentKind::Previewed);
            }
            Err(e) => {
                self.clear_content();
                self.set_error(&e);
            }
        }
        self.stateChanged();
    }

    /// Shows the text; returns the colouring still at work.
    fn apply(&mut self, v: ViewedText) -> Option<ColourJob> {
        self.loaded = true;
        self.size = v.size.map_or(-1.0, |s| s as f64);
        self.text = QString::from(v.lines.text());
        self.truncated = v.truncated;
        self.validUtf8 = v.valid_utf8;
        self.set_lines(v.lines);
        v.colouring
    }

    /// The colours, once they come, for the text that is still shown.
    fn colour_later(&mut self, job: ColourJob) {
        let gen = self.generation;
        run_then(self, async move { Ok(job.finish().await) }, move |me, res| {
            if let (true, Ok(Some(lines))) = (me.generation == gen, res) {
                if lines.coloured_lines() > 0 {
                    me.set_lines(lines);
                    me.stateChanged();
                }
            }
        });
    }

    fn set_lines(&mut self, lines: ColouredText) {
        self.highlighted = lines.coloured_lines() > 0;
        let syntax = lines.syntax().filter(|_| self.highlighted);
        self.syntax = qstr(syntax.unwrap_or_default());
        self.lines = lines;
    }

    fn styledLine(&self, index: i32, colours: QString) -> QString {
        let palette = Palette::parse(&colours.to_string());
        usize::try_from(index)
            .ok()
            .and_then(|i| self.lines.styled_line(i, &palette))
            .map_or_else(QString::default, |s| QString::from(s.as_str()))
    }

    fn clear_content(&mut self) {
        self.loaded = false;
        self.truncated = false;
        self.validUtf8 = false;
        self.size = -1.0;
        self.text = QString::default();
        self.set_lines(ColouredText::default());
    }

    fn clear_error(&mut self) {
        self.errorKind = QString::default();
        self.errorMessage = QString::default();
    }

    fn set_error(&mut self, e: &Error) {
        self.errorKind = qstr(e.kind.name());
        self.errorMessage = qstr(&e.message);
    }
}

/// A Markdown file as Qt rich text (PRV-4).
#[derive(QObject, Default)]
pub struct MarkdownDocument {
    base: qt_base_class!(trait QObject),
    uri: qt_property!(QString; WRITE setUri NOTIFY uriChanged),
    uriChanged: qt_signal!(),
    loading: qt_property!(bool; NOTIFY stateChanged),
    html: qt_property!(QString; NOTIFY stateChanged),
    truncated: qt_property!(bool; NOTIFY stateChanged),
    errorKind: qt_property!(QString; NOTIFY stateChanged),
    errorMessage: qt_property!(QString; NOTIFY stateChanged),
    stateChanged: qt_signal!(),
    reload: qt_method!(fn(&mut self)),
    generation: u64,
}

impl MarkdownDocument {
    fn setUri(&mut self, value: QString) {
        if self.uri == value {
            return;
        }
        self.uri = value;
        self.uriChanged();
        self.reload();
    }

    fn reload(&mut self) {
        self.generation += 1;
        let gen = self.generation;
        let (Some(core), Some(uri)) = (core(), parse_uri(&self.uri)) else {
            return;
        };
        self.loading = true;
        self.errorKind = QString::default();
        self.stateChanged();
        let seen = uri.clone();
        let c = core.clone();
        run_then(
            self,
            async move { core.render_markdown(&uri).await },
            move |me, res| {
                if me.generation != gen {
                    return;
                }
                me.loading = false;
                match res {
                    Ok(r) => {
                        me.html = QString::from(r.html.as_str());
                        me.truncated = r.truncated;
                        c.note_viewed(&seen, RecentKind::Previewed);
                    }
                    Err(e) => {
                        me.errorKind = qstr(e.kind.name());
                        me.errorMessage = qstr(&e.message);
                    }
                }
                me.stateChanged();
            },
        );
    }
}
