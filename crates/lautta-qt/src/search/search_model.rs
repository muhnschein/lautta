// SPDX-License-Identifier: LGPL-2.1-or-later

use crate::json::{from_json, to_json};
use crate::runtime::{core, handle};
use lautta_core::app::Core;
use lautta_core::app_search::{
    highlight_span, ScopeEvent, SearchGroup, SearchRequest, SearchScope, LOCATION_SCOPE_PREFIX,
};
use lautta_core::locations::LocationKind;
use lautta_core::mime::category_of_name;
use lautta_core::search::{MatchMode, SearchHit};
use lautta_core::settings::Settings;
use lautta_core::Uri;
use qmetaobject::prelude::*;
use qmetaobject::{queued_callback, QPointer, USER_ROLE};
use serde::Serialize;
use std::collections::HashMap;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;

struct Row {
    group: usize,
    group_key: String,
    place: String,
    uri: String,
    folder_uri: String,
    name: String,
    lossy: bool,
    is_dir: bool,
    size: i64,
    modified: i64,
    category: &'static str,
    hl_start: i32,
    hl_len: i32,
    mime_type: String,
    thumbnail_source: String,
    text_thumb: bool,
    text_mono: bool,
    type_icon: &'static str,
}

#[derive(Serialize)]
struct ScopeChoice {
    value: String,
    label: String,
    kind: &'static str,
    icon: &'static str,
}

#[derive(Serialize, Clone)]
struct GroupState {
    key: String,
    name: String,
    kind: &'static str,
    state: &'static str,
    folders: u64,
    hits: u64,
    errors: u64,
    rows: usize,
    #[serde(rename = "errorKind")]
    error_kind: String,
}

enum Event {
    Rows(usize, Vec<Row>),
    Progress(usize, u64),
    GroupDone(usize, Result<(u64, u64, u64), (String, String)>),
    Remembered,
    Finished,
}

const ROLES: [&str; 19] = [
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
    "group",
    "place",
    "mimeType",
    "thumbnailSource",
    "textThumb",
    "textMono",
    "typeIcon",
    "groupIndex",
];

#[derive(QObject, Default)]
pub struct SearchModel {
    base: qt_base_class!(trait QAbstractListModel),

    rootUri: qt_property!(QString; NOTIFY rootUriChanged WRITE set_root_uri),
    rootUriChanged: qt_signal!(),
    rootName: qt_property!(QString; NOTIFY rootUriChanged),
    scope: qt_property!(QString; NOTIFY scopeChanged WRITE set_scope),
    scopeChanged: qt_signal!(),
    query: qt_property!(QString; NOTIFY filtersChanged),
    types: qt_property!(QString; NOTIFY filtersChanged),
    sizePreset: qt_property!(QString; NOTIFY filtersChanged),
    datePreset: qt_property!(QString; NOTIFY filtersChanged),
    includeHidden: qt_property!(bool; NOTIFY filtersChanged),
    filtersChanged: qt_signal!(),

    running: qt_property!(bool; NOTIFY runningChanged),
    runningChanged: qt_signal!(),
    hitCount: qt_property!(i32; NOTIFY progressChanged),
    folderCount: qt_property!(i32; NOTIFY progressChanged),
    errorCount: qt_property!(i32; NOTIFY progressChanged),
    errorKind: qt_property!(QString; NOTIFY progressChanged),
    errorMessage: qt_property!(QString; NOTIFY progressChanged),
    groupsJson: qt_property!(QString; NOTIFY progressChanged),
    progressChanged: qt_signal!(),
    depthLimit: qt_property!(i32; NOTIFY progressChanged),
    recentSearchesJson: qt_property!(QString; NOTIFY recentChanged),
    recentChanged: qt_signal!(),

    start: qt_method!(fn(&mut self) -> bool),
    cancel: qt_method!(fn(&mut self)),
    clear: qt_method!(fn(&mut self)),
    refreshRecent: qt_method!(fn(&mut self)),
    clearRecent: qt_method!(fn(&mut self)),
    canSearch: qt_method!(fn(&self) -> bool),
    hasServers: qt_method!(fn(&self) -> bool),
    scopeLocationsJson: qt_method!(fn(&self) -> QString),

    rows: Vec<Row>,
    group_sizes: Vec<usize>,
    groups: Vec<GroupState>,
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
            0 => text(if self.scope() == SearchScope::Folder {
                &r.place
            } else {
                &r.group_key
            }),
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
            11 => text(&r.group_key),
            12 => text(&r.place),
            13 => text(&r.mime_type),
            14 => text(&r.thumbnail_source),
            15 => QVariant::from(r.text_thumb),
            16 => QVariant::from(r.text_mono),
            17 => text(r.type_icon),
            18 => QVariant::from(r.group as i32),
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

struct RowMaker {
    groups: Vec<SearchGroup>,
    text: String,
    mode: MatchMode,
    settings: Settings,
}

impl RowMaker {
    fn row(&self, core: &Core, group: usize, root: usize, hit: &SearchHit) -> Option<Row> {
        let spec = self.groups.get(group)?;
        let root = spec.roots.get(root)?;
        let e = &hit.entry;
        let name = e.display_name();
        let span = highlight_span(&name, &self.text, self.mode);
        let look = core.item_look(&hit.uri, e.is_dir(), &self.settings);
        Some(Row {
            group,
            group_key: spec.key.clone(),
            place: spec.place(root, &hit.folder_uri),
            uri: hit.uri.to_string(),
            folder_uri: hit.folder_uri.to_string(),
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
            mime_type: look.mime_type,
            thumbnail_source: look.thumbnail_source,
            text_thumb: look.text_thumb,
            text_mono: look.text_mono,
            type_icon: look.type_icon,
            name,
        })
    }
}

fn insert_at(group_sizes: &[usize], group: usize) -> usize {
    group_sizes.iter().take(group + 1).sum()
}

impl SearchModel {
    fn set_root_uri(&mut self, uri: QString) {
        self.rootUri = uri;
        self.rootName = QString::from(self.root_name().as_str());
        self.rootUriChanged();
        self.refreshRecent();
    }

    fn set_scope(&mut self, scope: QString) {
        if scope == self.scope {
            return;
        }
        self.scope = scope;
        self.scopeChanged();
        self.refreshRecent();
    }

    fn scope(&self) -> SearchScope {
        SearchScope::parse(&self.scope.to_string()).unwrap_or(SearchScope::Folder)
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

    fn recents_key(&self) -> String {
        self.scope().recents_key(self.root().as_ref())
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
        let text = self.query.to_string();
        SearchRequest {
            mode: MatchMode::for_text(&text),
            text,
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

    fn hasServers(&self) -> bool {
        core().is_some_and(|c| {
            c.locations
                .locations()
                .iter()
                .any(|l| matches!(l.kind, LocationKind::Server { .. }))
        })
    }

    fn scopeLocationsJson(&self) -> QString {
        let list: Vec<ScopeChoice> = core()
            .map(|c| c.scope_locations())
            .unwrap_or_default()
            .into_iter()
            .map(|l| ScopeChoice {
                value: format!("{LOCATION_SCOPE_PREFIX}{}", l.id),
                label: l.name,
                kind: l.kind.name(),
                icon: l.icon,
            })
            .collect();
        QString::from(to_json(&list).as_str())
    }

    fn refreshRecent(&mut self) {
        let key = self.recents_key();
        let list = match core() {
            Some(core) if !key.is_empty() => core.recent_searches.list(&key).unwrap_or_default(),
            _ => Vec::new(),
        };
        self.recentSearchesJson = QString::from(to_json(&list).as_str());
        self.recentChanged();
    }

    fn clearRecent(&mut self) {
        let key = self.recents_key();
        if let (Some(core), false) = (core(), key.is_empty()) {
            let _ = core.recent_searches.clear(&key);
        }
        self.refreshRecent();
    }

    fn publish_groups(&mut self) {
        self.groupsJson = QString::from(to_json(&self.groups).as_str());
        self.progressChanged();
    }

    fn clear(&mut self) {
        self.cancel();
        self.begin_reset_model();
        self.rows.clear();
        self.end_reset_model();
        self.group_sizes.clear();
        self.groups.clear();
        self.hitCount = 0;
        self.folderCount = 0;
        self.errorCount = 0;
        self.errorKind = QString::default();
        self.errorMessage = QString::default();
        self.publish_groups();
    }

    fn cancel(&mut self) {
        if let Some(flag) = self.cancel_flag.take() {
            flag.store(true, Ordering::Relaxed);
        }
        self.generation += 1;
        let mut changed = false;
        for g in &mut self.groups {
            if g.state == "searching" {
                g.state = "stopped";
                changed = true;
            }
        }
        if changed {
            self.publish_groups();
        }
        if self.running {
            self.running = false;
            self.runningChanged();
        }
    }

    fn start(&mut self) -> bool {
        let Some(core) = core() else {
            return false;
        };
        let scope = self.scope();
        let root = self.root();
        let request = self.request();
        if !request.is_restricted() || (scope == SearchScope::Folder && root.is_none()) {
            return false;
        }
        let groups = core.search_groups(&scope, root.as_ref());
        if groups.is_empty() {
            return false;
        }
        self.clear();
        self.depthLimit = match (&scope, &root) {
            (SearchScope::Folder, Some(r)) => core.search_depth(r).map_or(-1, |d| d as i32),
            _ => -1,
        };
        self.group_sizes = vec![0; groups.len()];
        self.groups = groups
            .iter()
            .map(|g| GroupState {
                key: g.key.clone(),
                name: g.name.clone(),
                kind: g.kind.name(),
                state: "searching",
                folders: 0,
                hits: 0,
                errors: 0,
                rows: 0,
                error_kind: String::new(),
            })
            .collect();
        self.running = true;
        self.runningChanged();
        self.publish_groups();
        let cancel = Arc::new(AtomicBool::new(false));
        self.cancel_flag = Some(cancel.clone());
        let generation = self.generation;
        let me = QPointer::from(&*self);
        let post = queued_callback(move |event: Event| {
            if let Some(model) = me.as_pinned() {
                model.borrow_mut().apply(generation, event);
            }
        });
        let maker = RowMaker {
            groups: groups.clone(),
            text: request.text.clone(),
            mode: request.mode,
            settings: core.settings(),
        };
        let key = scope.recents_key(root.as_ref());
        handle().spawn(async move {
            let (tx, mut rx) = tokio::sync::mpsc::channel(8);
            let search_core = core.clone();
            let search = tokio::spawn(async move {
                search_core
                    .search_scope(&groups, &request, &key, cancel, tx)
                    .await;
            });
            while let Some(event) = rx.recv().await {
                match event {
                    ScopeEvent::Hits { group, root, hits } => {
                        let mut rows: Vec<Row> = hits
                            .iter()
                            .filter_map(|h| maker.row(&core, group, root, h))
                            .collect();
                        rows.sort_by_key(|a| a.name.to_lowercase());
                        post(Event::Rows(group, rows));
                    }
                    ScopeEvent::Progress { group, folders } => post(Event::Progress(group, folders)),
                    ScopeEvent::Remembered => post(Event::Remembered),
                    ScopeEvent::Done { group, result } => post(Event::GroupDone(
                        group,
                        result
                            .map(|s| (s.folders, s.hits, s.errors))
                            .map_err(|e| (e.kind.name().to_owned(), e.message)),
                    )),
                }
            }
            let _ = search.await;
            post(Event::Finished);
        });
        true
    }

    fn apply(&mut self, generation: u64, event: Event) {
        if generation != self.generation && !matches!(event, Event::Remembered) {
            return;
        }
        match event {
            Event::Remembered => self.refreshRecent(),
            Event::Rows(group, rows) => self.add_rows(group, rows),
            Event::Progress(group, folders) => {
                if let Some(g) = self.groups.get_mut(group) {
                    g.folders = g.folders.max(folders);
                }
                self.folderCount = self.groups.iter().map(|g| g.folders).sum::<u64>() as i32;
                self.publish_groups();
            }
            Event::GroupDone(group, result) => {
                if let Some(g) = self.groups.get_mut(group) {
                    match result {
                        Ok((folders, hits, errors)) => {
                            g.state = "done";
                            g.folders = folders;
                            g.hits = hits;
                            g.errors = errors;
                        }
                        Err((kind, message)) => {
                            g.state = "failed";
                            g.error_kind = kind.clone();
                            if self.groups.len() == 1 {
                                self.errorKind = QString::from(kind.as_str());
                                self.errorMessage = QString::from(message.as_str());
                            }
                        }
                    }
                }
                self.folderCount = self.groups.iter().map(|g| g.folders).sum::<u64>() as i32;
                self.errorCount = self.groups.iter().map(|g| g.errors).sum::<u64>() as i32;
                self.publish_groups();
            }
            Event::Finished => {
                self.running = false;
                self.cancel_flag = None;
                self.runningChanged();
                self.refreshRecent();
            }
        }
    }

    fn add_rows(&mut self, group: usize, rows: Vec<Row>) {
        if rows.is_empty() || group >= self.group_sizes.len() {
            return;
        }
        let at = insert_at(&self.group_sizes, group);
        let n = rows.len();
        self.begin_insert_rows(at as i32, (at + n) as i32 - 1);
        self.rows.splice(at..at, rows);
        self.end_insert_rows();
        self.group_sizes[group] += n;
        if let Some(g) = self.groups.get_mut(group) {
            g.rows = self.group_sizes[group];
        }
        self.hitCount = self.rows.len() as i32;
        self.publish_groups();
    }
}

#[cfg(test)]
mod tests {
    use super::insert_at;

    #[test]
    fn rows_of_a_group_go_after_its_own_and_before_later_groups() {
        let sizes = [2, 0, 3];
        assert_eq!(insert_at(&sizes, 0), 2);
        assert_eq!(insert_at(&sizes, 1), 2);
        assert_eq!(insert_at(&sizes, 2), 5);
        assert_eq!(insert_at(&[], 0), 0);
    }
}
