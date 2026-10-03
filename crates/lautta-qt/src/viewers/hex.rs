// SPDX-License-Identifier: LGPL-2.1-or-later
//! `HexModel`: rows of 16 bytes read on demand with ranged reads (PRV-4),
//! so a huge remote file scrolls without being downloaded.

use super::{parse_uri, qstr, run_then};
use crate::runtime::core;
use lautta_core::app_viewers::parse_offset;
use lautta_core::org::recents::RecentKind;
use lautta_core::preview::hex::{HexRow, HexView, ROW_BYTES};
use qmetaobject::prelude::*;
use qmetaobject::{qml_register_type, QPointer};
use std::cell::RefCell;
use std::collections::{HashMap, HashSet};
use std::sync::Arc;

pub fn register() {
    qml_register_type::<HexModel>(&crate::qml_uri(), 1, 0, &crate::cstr("HexModel"));
}

/// Rows fetched together; two pages of this size cover a phone screen.
const CHUNK_ROWS: i32 = 64;
const ROLE_OFFSET_TEXT: i32 = 0x100;
const ROLE_OFFSET: i32 = 0x101;
const ROLE_HEX: i32 = 0x102;
const ROLE_ASCII: i32 = 0x103;
const ROLE_LOADED: i32 = 0x104;

#[derive(QObject, Default)]
pub struct HexModel {
    base: qt_base_class!(trait QAbstractListModel),
    uri: qt_property!(QString; WRITE setUri NOTIFY uriChanged),
    uriChanged: qt_signal!(),
    loading: qt_property!(bool; NOTIFY stateChanged),
    /// File size in bytes.
    size: qt_property!(f64; NOTIFY stateChanged),
    count: qt_property!(i32; NOTIFY stateChanged),
    errorKind: qt_property!(QString; NOTIFY stateChanged),
    errorMessage: qt_property!(QString; NOTIFY stateChanged),
    stateChanged: qt_signal!(),
    /// The row showing a hexadecimal offset (`0x1F40` or `1f40`), -1 when it
    /// is not a valid offset inside the file.
    rowOfOffset: qt_method!(fn(&self, text: QString) -> i32),

    view: Option<Arc<tokio::sync::Mutex<HexView>>>,
    chunks: HashMap<i32, Vec<HexRow>>,
    pending: RefCell<HashSet<i32>>,
    generation: u64,
}

impl HexModel {
    fn setUri(&mut self, value: QString) {
        if self.uri == value {
            return;
        }
        self.uri = value;
        self.uriChanged();
        self.open();
    }

    fn reset(&mut self) {
        self.begin_reset_model();
        self.view = None;
        self.chunks.clear();
        self.pending.borrow_mut().clear();
        self.count = 0;
        self.size = 0.0;
        self.end_reset_model();
    }

    fn open(&mut self) {
        self.generation += 1;
        let gen = self.generation;
        self.reset();
        self.errorKind = QString::default();
        let (Some(core), Some(uri)) = (core(), parse_uri(&self.uri)) else {
            self.stateChanged();
            return;
        };
        self.loading = true;
        self.stateChanged();
        let seen = uri.clone();
        let c = core.clone();
        run_then(self, async move { core.open_hex(&uri).await }, move |me, res| {
            if me.generation != gen {
                return;
            }
            me.loading = false;
            match res {
                Ok(view) => {
                    me.begin_reset_model();
                    me.size = view.size() as f64;
                    me.count = i32::try_from(view.row_count()).unwrap_or(i32::MAX);
                    me.view = Some(Arc::new(tokio::sync::Mutex::new(view)));
                    me.end_reset_model();
                    c.note_viewed(&seen, RecentKind::Previewed);
                }
                Err(e) => {
                    me.errorKind = qstr(e.kind.name());
                    me.errorMessage = qstr(&e.message);
                }
            }
            me.stateChanged();
        });
    }

    fn rowOfOffset(&self, text: QString) -> i32 {
        let Some(offset) = parse_offset(&text.to_string()) else {
            return -1;
        };
        if self.view.is_none() || offset >= self.size as u64 {
            return -1;
        }
        i32::try_from(offset / ROW_BYTES as u64).unwrap_or(-1)
    }

    /// Starts fetching the chunk holding `row` unless it is cached or on
    /// its way.
    fn want(&self, row: i32) {
        let chunk = row / CHUNK_ROWS;
        let Some(view) = self.view.clone() else {
            return;
        };
        if self.chunks.contains_key(&chunk) || !self.pending.borrow_mut().insert(chunk) {
            return;
        }
        let ptr = QPointer::from(self);
        let gen = self.generation;
        crate::runtime::spawn_then(
            async move {
                let first = u64::try_from(chunk * CHUNK_ROWS).unwrap_or(0);
                view.lock().await.rows(first, CHUNK_ROWS as usize).await
            },
            move |res| {
                let Some(pinned) = ptr.as_pinned() else {
                    return;
                };
                let mut me = pinned.borrow_mut();
                if me.generation == gen {
                    me.chunk_arrived(chunk, res);
                }
            },
        );
    }

    fn chunk_arrived(&mut self, chunk: i32, res: lautta_core::Result<Vec<HexRow>>) {
        self.pending.borrow_mut().remove(&chunk);
        match res {
            Ok(rows) => {
                let n = i32::try_from(rows.len()).unwrap_or(0);
                self.chunks.insert(chunk, rows);
                let first = chunk * CHUNK_ROWS;
                let (a, b) = (self.row_index(first), self.row_index(first + n.max(1) - 1));
                self.data_changed(a, b);
            }
            Err(e) => {
                self.errorKind = qstr(e.kind.name());
                self.errorMessage = qstr(&e.message);
                self.stateChanged();
            }
        }
    }

    fn row(&self, row: i32) -> Option<&HexRow> {
        self.chunks
            .get(&(row / CHUNK_ROWS))
            .and_then(|c| c.get(usize::try_from(row % CHUNK_ROWS).ok()?))
    }
}

impl QAbstractListModel for HexModel {
    fn row_count(&self) -> i32 {
        self.count
    }

    fn data(&self, index: QModelIndex, role: i32) -> QVariant {
        let row = index.row();
        let Some(r) = self.row(row) else {
            self.want(row);
            return match role {
                ROLE_LOADED => false.into(),
                ROLE_OFFSET => (row as f64 * ROW_BYTES as f64).into(),
                ROLE_OFFSET_TEXT | ROLE_HEX | ROLE_ASCII => QString::default().into(),
                _ => QVariant::default(),
            };
        };
        match role {
            ROLE_OFFSET_TEXT => qstr(&r.offset_text).into(),
            ROLE_OFFSET => (r.offset as f64).into(),
            ROLE_HEX => qstr(&r.hex).into(),
            ROLE_ASCII => qstr(&r.ascii).into(),
            ROLE_LOADED => true.into(),
            _ => QVariant::default(),
        }
    }

    fn role_names(&self) -> HashMap<i32, QByteArray> {
        let mut m = HashMap::new();
        m.insert(ROLE_OFFSET_TEXT, "offsetText".into());
        m.insert(ROLE_OFFSET, "offset".into());
        m.insert(ROLE_HEX, "hex".into());
        m.insert(ROLE_ASCII, "ascii".into());
        m.insert(ROLE_LOADED, "loaded".into());
        m
    }
}
