// SPDX-License-Identifier: LGPL-2.1-or-later
//! `SearchModel { rootUri }` (SRC-2..4): recursive name search below a
//! folder, results streamed in per folder and grouped by it.

use crate::json::{from_json, to_json};
use crate::runtime::{core, handle};
use lautta_core::app_search::{highlight_span, section_label, SearchRequest};
use lautta_core::mime::category_of_name;
use lautta_core::search::{MatchMode, SearchHit, SearchSummary};
use lautta_core::Uri;
use qmetaobject::prelude::*;
use qmetaobject::{queued_callback, QPointer, USER_ROLE};
use std::collections::HashMap;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;

struct Row {
    uri: String,
    folder_uri: String,
    section: String,
    name: String,
    lossy: bool,
    is_dir: bool,
    size: i64,
    modified: i64,
    category: &'static str,
    hl_start: i32,
    hl_len: i32,
}

enum Event {
    Batch(Vec<SearchHit>),
    Done(Result<SearchSummary, (String, String)>),
}

const ROLES: [&str; 11] = [
    "section",
    "uri",
    "name",
    "nameIsLossy",
    "folderUri",
    "isDir",
    "size",
    "modified",
    "category",
    "matchStart",
    "matchLength",
];

#[derive(QObject, Default)]
pub struct SearchModel {
    base: qt_base_class!(trait QAbstractListModel),

    rootUri: qt_property!(QString; NOTIFY rootUriChanged WRITE set_root_uri),
    rootUriChanged: qt_signal!(),
    /// Display name of the search root ("Documents").
    rootName: qt_property!(QString; NOTIFY rootUriChanged),
    /// Typed text; blank with filters lists everything that passes them.
    query: qt_property!(QString; NOTIFY filtersChanged),
    /// `substring` or `glob`.
    matchMode: qt_property!(QString; NOTIFY filtersChanged),
    /// JSON array of `folder`, `image`, `video`, `audio`, `document`,
    /// `archive`, `text`; empty means any.
    types: qt_property!(QString; NOTIFY filtersChanged),
    /// `any`, `gt1m`, `gt10m`, `gt100m`, `lt100k`, `lt1m`.
    sizePreset: qt_property!(QString; NOTIFY filtersChanged),
    /// `any`, `today`, `week`, `month`, `year`.
    datePreset: qt_property!(QString; NOTIFY filtersChanged),
    includeHidden: qt_property!(bool; NOTIFY filtersChanged),
    filtersChanged: qt_signal!(),

    running: qt_property!(bool; NOTIFY runningChanged),
    runningChanged: qt_signal!(),
    hitCount: qt_property!(i32; NOTIFY progressChanged),
    folderCount: qt_property!(i32; NOTIFY progressChanged),
    errorCount: qt_property!(i32; NOTIFY progressChanged),
    /// Why the whole search failed (`ErrorKind` name), empty when it did not.
    errorKind: qt_property!(QString; NOTIFY progressChanged),
    errorMessage: qt_property!(QString; NOTIFY progressChanged),
    progressChanged: qt_signal!(),
    /// Depth limit of the last search; -1 is unlimited (SRC-3).
    depthLimit: qt_property!(i32; NOTIFY progressChanged),
    recentSearchesJson: qt_property!(QString; NOTIFY recentChanged),
    recentChanged: qt_signal!(),

    start: qt_method!(fn(&mut self) -> bool),
    cancel: qt_method!(fn(&mut self)),
    clear: qt_method!(fn(&mut self)),
    refreshRecent: qt_method!(fn(&mut self)),
    clearRecent: qt_method!(fn(&mut self)),
    /// Whether the typed text or a filter narrows the search.
    canSearch: qt_method!(fn(&self) -> bool),

    rows: Vec<Row>,
    generation: u64,
    cancel_flag: Option<Arc<AtomicBool>>,
}

impl Drop for SearchModel {
    fn drop(&mut self) {
        if let Some(flag) = &self.cancel_flag {
            flag.store(true, Ordering::Relaxed);
        }
    }
}

impl QAbstractListModel for SearchModel {
    fn row_count(&self) -> i32 {
        self.rows.len() as i32
    }

    fn data(&self, index: QModelIndex, role: i32) -> QVariant {
        let Some(r) = usize::try_from(index.row()).ok().and_then(|i| self.rows.get(i)) else {
            return QVariant::default();
        };
        let text = |s: &str| QVariant::from(QString::from(s));
        match role - USER_ROLE {
            0 => text(&r.section),
            1 => text(&r.uri),
            2 => text(&r.name),
            3 => QVariant::from(r.lossy),
            4 => text(&r.folder_uri),
            5 => QVariant::from(r.is_dir),
            6 => QVariant::from(r.size),
            7 => QVariant::from(r.modified),
            8 => text(r.category),
            9 => QVariant::from(r.hl_start),
            10 => QVariant::from(r.hl_len),
            _ => QVariant::default(),
        }
    }

    fn role_names(&self) -> HashMap<i32, QByteArray> {
        ROLES
            .iter()
            .enumerate()
            .map(|(i, n)| (USER_ROLE + i as i32, QByteArray::from(*n)))
            .collect()
    }
}

fn parse_mode(mode: &str) -> MatchMode {
    if mode == "glob" {
        MatchMode::Glob
    } else {
        MatchMode::Substring
    }
}

impl SearchModel {
    fn set_root_uri(&mut self, uri: QString) {
        self.rootUri = uri;
        self.rootName = QString::from(self.root_name().as_str());
        self.rootUriChanged();
        self.refreshRecent();
    }

    fn root(&self) -> Option<Uri> {
        Uri::parse(&self.rootUri.to_string()).ok()
    }

    fn root_name(&self) -> String {
        let Some(root) = self.root() else {
            return String::new();
        };
        match root.name() {
            Some(n) => String::from_utf8_lossy(n).into_owned(),
            None => core()
                .and_then(|c| c.location(&root.location))
                .map(|l| l.name)
                .unwrap_or_default(),
        }
    }

    fn request(&self) -> SearchRequest {
        let none = |s: &QString| {
            let s = s.to_string();
            if s.is_empty() {
                "any".to_owned()
            } else {
                s
            }
        };
        SearchRequest {
            text: self.query.to_string(),
            mode: parse_mode(&self.matchMode.to_string()),
            types: from_json::<Vec<String>>(&self.types.to_string())
                .unwrap_or_default()
                .into_iter()
                .collect(),
            size_preset: none(&self.sizePreset),
            date_preset: none(&self.datePreset),
            include_hidden: self.includeHidden,
        }
    }

    fn canSearch(&self) -> bool {
        self.request().is_restricted()
    }

    fn refreshRecent(&mut self) {
        let list = match (core(), self.root()) {
            (Some(core), Some(root)) => core.recent_searches_of(&root),
            _ => Vec::new(),
        };
        self.recentSearchesJson = QString::from(to_json(&list).as_str());
        self.recentChanged();
    }

    fn clearRecent(&mut self) {
        if let (Some(core), Some(root)) = (core(), self.root()) {
            let _ = core.recent_searches.clear(&root.location);
        }
        self.refreshRecent();
    }

    fn clear(&mut self) {
        self.cancel();
        self.begin_reset_model();
        self.rows.clear();
        self.end_reset_model();
        self.hitCount = 0;
        self.folderCount = 0;
        self.errorCount = 0;
        self.errorKind = QString::default();
        self.errorMessage = QString::default();
        self.progressChanged();
    }

    fn cancel(&mut self) {
        if let Some(flag) = self.cancel_flag.take() {
            flag.store(true, Ordering::Relaxed);
        }
        // Late events of the stopped search are ignored.
        self.generation += 1;
        if self.running {
            self.running = false;
            self.runningChanged();
        }
    }

    /// Starts (or restarts) the search; false when there is nothing to
    /// search for or no root.
    fn start(&mut self) -> bool {
        let (Some(core), Some(root)) = (core(), self.root()) else {
            return false;
        };
        let request = self.request();
        if !request.is_restricted() {
            return false;
        }
        self.clear();
        self.depthLimit = core.search_depth(&root).map_or(-1, |d| d as i32);
        self.running = true;
        self.runningChanged();
        self.progressChanged();
        let cancel = Arc::new(AtomicBool::new(false));
        self.cancel_flag = Some(cancel.clone());
        let generation = self.generation;
        let me = QPointer::from(&*self);
        let post = queued_callback(move |event: Event| {
            if let Some(model) = me.as_pinned() {
                model.borrow_mut().apply(generation, event);
            }
        });
        handle().spawn(async move {
            let (tx, mut rx) = tokio::sync::mpsc::channel(8);
            let search = tokio::spawn(async move { core.search_tree(root, &request, cancel, tx).await });
            while let Some(batch) = rx.recv().await {
                post(Event::Batch(batch));
            }
            let done = match search.await {
                Ok(Ok(summary)) => Ok(summary),
                Ok(Err(e)) => Err((e.kind.name().to_owned(), e.message)),
                Err(e) => Err(("Internal".to_owned(), e.to_string())),
            };
            post(Event::Done(done));
        });
        true
    }

    fn apply(&mut self, generation: u64, event: Event) {
        if generation != self.generation {
            return;
        }
        match event {
            Event::Batch(hits) => self.add_hits(&hits),
            Event::Done(result) => {
                self.running = false;
                self.cancel_flag = None;
                match result {
                    Ok(summary) => {
                        self.folderCount = summary.folders as i32;
                        self.errorCount = summary.errors as i32;
                    }
                    Err((kind, message)) => {
                        self.errorKind = QString::from(kind.as_str());
                        self.errorMessage = QString::from(message.as_str());
                    }
                }
                self.runningChanged();
                self.progressChanged();
                self.refreshRecent();
            }
        }
    }

    fn add_hits(&mut self, hits: &[SearchHit]) {
        let Some(root) = self.root() else { return };
        let root_name = self.root_name();
        let text = self.query.to_string();
        let mode = parse_mode(&self.matchMode.to_string());
        let mut rows: Vec<Row> = hits
            .iter()
            .map(|h| row_of(h, &root, &root_name, &text, mode))
            .collect();
        rows.sort_by(|a, b| a.name.to_lowercase().cmp(&b.name.to_lowercase()));
        let first = self.rows.len() as i32;
        self.begin_insert_rows(first, first + rows.len() as i32 - 1);
        self.rows.extend(rows);
        self.end_insert_rows();
        self.hitCount = self.rows.len() as i32;
        self.progressChanged();
    }
}

fn row_of(hit: &SearchHit, root: &Uri, root_name: &str, text: &str, mode: MatchMode) -> Row {
    let e = &hit.entry;
    let name = e.display_name();
    let span = highlight_span(&name, text, mode);
    Row {
        uri: hit.uri.to_string(),
        folder_uri: hit.folder_uri.to_string(),
        section: section_label(root, root_name, &hit.folder_uri),
        lossy: e.name_is_lossy(),
        is_dir: e.is_dir(),
        size: e.size.map_or(-1, |s| i64::try_from(s).unwrap_or(i64::MAX)),
        modified: e.modified_ms().unwrap_or(-1),
        category: if e.is_dir() {
            "folder"
        } else {
            category_of_name(&e.name).icon_name()
        },
        hl_start: span.map_or(0, |s| s.0 as i32),
        hl_len: span.map_or(0, |s| s.1 as i32),
        name,
    }
}
