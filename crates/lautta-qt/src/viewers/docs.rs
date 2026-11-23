// SPDX-License-Identifier: LGPL-2.1-or-later

use super::{parse_uri, qstr, run_then};
use crate::runtime::core;
use lautta_core::app_viewers::ViewedText;
use lautta_core::org::recents::RecentKind;
use lautta_core::preview::highlight::{ColourJob, ColouredText, Palette};
use lautta_core::preview::markdown::{Block, Style};
use lautta_core::preview::rows::TextRows;
use lautta_core::Error;
use qmetaobject::prelude::*;
use qmetaobject::qml_register_type;

pub fn register() {
    qml_register_type::<TextDocument>(&crate::qml_uri(), 1, 0, &crate::cstr("TextDocument"));
    qml_register_type::<MarkdownDocument>(&crate::qml_uri(), 1, 0, &crate::cstr("MarkdownDocument"));
}

#[derive(QObject, Default)]
pub struct TextDocument {
    base: qt_base_class!(trait QObject),
    uri: qt_property!(QString; WRITE setUri NOTIFY uriChanged),
    uriChanged: qt_signal!(),
    loading: qt_property!(bool; NOTIFY stateChanged),
    loaded: qt_property!(bool; NOTIFY stateChanged),
    text: qt_property!(QString; NOTIFY stateChanged),
    truncated: qt_property!(bool; NOTIFY stateChanged),
    validUtf8: qt_property!(bool; NOTIFY stateChanged),
    size: qt_property!(f64; NOTIFY stateChanged),
    errorKind: qt_property!(QString; NOTIFY stateChanged),
    errorMessage: qt_property!(QString; NOTIFY stateChanged),
    rowCount: qt_property!(i32; NOTIFY stateChanged),
    lineCount: qt_property!(i32; NOTIFY stateChanged),
    syntax: qt_property!(QString; NOTIFY colouredChanged),
    highlighted: qt_property!(bool; NOTIFY colouredChanged),
    stateChanged: qt_signal!(),
    colouredChanged: qt_signal!(),

    reload: qt_method!(fn(&mut self)),
    rowNumber: qt_method!(fn(&self, index: i32) -> i32),
    styledRow: qt_method!(fn(&self, index: i32, colours: QString) -> QString),

    generation: u64,
    lines: ColouredText,
    rows: TextRows,
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

    fn apply(&mut self, v: ViewedText) -> Option<ColourJob> {
        self.loaded = true;
        self.size = v.size.map_or(-1.0, |s| s as f64);
        self.text = QString::from(v.lines.text());
        self.truncated = v.truncated;
        self.validUtf8 = v.valid_utf8;
        self.set_rows(TextRows::new(v.lines.text()));
        self.set_lines(v.lines);
        v.colouring
    }

    fn colour_later(&mut self, job: ColourJob) {
        let gen = self.generation;
        run_then(self, async move { Ok(job.finish().await) }, move |me, res| {
            if let (true, Ok(Some(lines))) = (me.generation == gen, res) {
                if lines.coloured_lines() > 0 {
                    me.set_lines(lines);
                }
            }
        });
    }

    fn set_lines(&mut self, lines: ColouredText) {
        self.highlighted = lines.coloured_lines() > 0;
        let syntax = lines.syntax().filter(|_| self.highlighted);
        self.syntax = qstr(syntax.unwrap_or_default());
        self.lines = lines;
        self.colouredChanged();
    }

    fn set_rows(&mut self, rows: TextRows) {
        self.rowCount = i32::try_from(rows.len()).unwrap_or(i32::MAX);
        self.lineCount = i32::try_from(rows.lines()).unwrap_or(i32::MAX);
        self.rows = rows;
    }

    fn rowNumber(&self, index: i32) -> i32 {
        let number = usize::try_from(index)
            .ok()
            .and_then(|i| self.rows.row(i))
            .map_or(0, |r| r.number);
        i32::try_from(number).unwrap_or(i32::MAX)
    }

    fn styledRow(&self, index: i32, colours: QString) -> QString {
        let Some(row) = usize::try_from(index).ok().and_then(|i| self.rows.row(i)) else {
            return QString::default();
        };
        let palette = Palette::parse(&colours.to_string());
        QString::from(
            self.lines
                .styled_row(row.line, row.start, row.text, &palette)
                .as_str(),
        )
    }

    fn clear_content(&mut self) {
        self.loaded = false;
        self.truncated = false;
        self.validUtf8 = false;
        self.size = -1.0;
        self.text = QString::default();
        self.set_rows(TextRows::default());
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

#[derive(QObject, Default)]
pub struct MarkdownDocument {
    base: qt_base_class!(trait QObject),
    uri: qt_property!(QString; WRITE setUri NOTIFY uriChanged),
    uriChanged: qt_signal!(),
    colours: qt_property!(QString; WRITE setColours NOTIFY coloursChanged),
    coloursChanged: qt_signal!(),
    loading: qt_property!(bool; NOTIFY stateChanged),
    blockCount: qt_property!(i32; NOTIFY stateChanged),
    truncated: qt_property!(bool; NOTIFY stateChanged),
    errorKind: qt_property!(QString; NOTIFY stateChanged),
    errorMessage: qt_property!(QString; NOTIFY stateChanged),
    stateChanged: qt_signal!(),
    reload: qt_method!(fn(&mut self)),
    block: qt_method!(fn(&self, index: i32) -> QString),
    blockKind: qt_method!(fn(&self, index: i32) -> QString),
    blockJoined: qt_method!(fn(&self, index: i32) -> bool),
    generation: u64,
    blocks: Vec<Block>,
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

    fn setColours(&mut self, value: QString) {
        if self.colours == value {
            return;
        }
        self.colours = value;
        self.coloursChanged();
        if !self.uri.is_empty() {
            self.reload();
        }
    }

    fn reload(&mut self) {
        self.generation += 1;
        let gen = self.generation;
        let (Some(core), Some(uri)) = (core(), parse_uri(&self.uri)) else {
            return;
        };
        let style = Style::parse(&self.colours.to_string());
        self.loading = true;
        self.errorKind = QString::default();
        self.stateChanged();
        let seen = uri.clone();
        let c = core.clone();
        run_then(
            self,
            async move { core.render_reading(&uri, style).await },
            move |me, res| {
                if me.generation != gen {
                    return;
                }
                me.loading = false;
                match res {
                    Ok(r) => {
                        me.blockCount = i32::try_from(r.blocks.len()).unwrap_or(i32::MAX);
                        me.blocks = r.blocks;
                        me.truncated = r.truncated;
                        c.note_viewed(&seen, RecentKind::Previewed);
                    }
                    Err(e) => {
                        me.blockCount = 0;
                        me.blocks = Vec::new();
                        me.errorKind = qstr(e.kind.name());
                        me.errorMessage = qstr(&e.message);
                    }
                }
                me.stateChanged();
            },
        );
    }
}

impl MarkdownDocument {
    fn at(&self, index: i32) -> Option<&Block> {
        usize::try_from(index).ok().and_then(|i| self.blocks.get(i))
    }

    fn block(&self, index: i32) -> QString {
        self.at(index)
            .map_or_else(QString::default, |b| QString::from(b.html.as_str()))
    }

    fn blockKind(&self, index: i32) -> QString {
        self.at(index)
            .map_or_else(QString::default, |b| QString::from(b.kind.name()))
    }

    fn blockJoined(&self, index: i32) -> bool {
        self.at(index).is_some_and(|b| b.joined)
    }
}
