// SPDX-License-Identifier: LGPL-2.1-or-later
//! `DirectoryModel { uri }` (doc/QML-API.md, SPEC §9): a folder listing on
//! top of `lautta_core::listing::ListingState`. Listings stream in from the
//! core (cached copy first, BRW-5), diffs become row inserts/removes/updates,
//! local folders are watched (BRW-6) and the view settings are the global
//! ones (BRW-4).

use crate::json::from_json;
use crate::runtime::{self, core, spawn_then};
use lautta_core::app_directory::{file_url, ListEvent};
use lautta_core::entry::{cap, Entry, EntryFlags};
use lautta_core::error::ErrorKind;
use lautta_core::filter::FilterOptions;
use lautta_core::listing::{ListChange, ListingState};
use lautta_core::locations::LocationStatus;
use lautta_core::mime::{self, FileCategory};
use lautta_core::sort::SortKey;
use lautta_core::viewprefs::{suggest_grid, ViewMode, ViewPrefs};
use lautta_core::watch::{DirWatcher, DIR_DEBOUNCE};
use lautta_core::{Error, Uri};
use qmetaobject::prelude::*;
use qmetaobject::{queued_callback, QByteArray, QPointer, QVariantList};
use std::collections::HashMap;
use std::ffi::OsStr;
use std::os::unix::ffi::OsStrExt;
use std::path::PathBuf;
use tokio::sync::mpsc::unbounded_channel;
use tokio::task::AbortHandle;

const ROLE_BASE: i32 = 0x0100;
const ROLES: [&str; 20] = [
    "name",
    "nameIsLossy",
    "uri",
    "isDir",
    "isSymlink",
    "size",
    "modified",
    "created",
    "mode",
    "owner",
    "group",
    "hidden",
    "readonly",
    "mimeType",
    "category",
    "icon",
    "thumbnailSource",
    "selected",
    "transferState",
    "inaccessible",
];

/// Errors that mean "can't reach it right now": a cached copy stays visible
/// and the page says it is offline (BRW-7).
fn is_connectivity(kind: ErrorKind) -> bool {
    matches!(
        kind,
        ErrorKind::NetworkUnreachable
            | ErrorKind::TimedOut
            | ErrorKind::ConnectionLost
            | ErrorKind::BridgeUnavailable
    )
}

fn qs(s: &str) -> QString {
    QString::from(s)
}

#[derive(QObject, Default)]
pub struct DirectoryModel {
    base: qt_base_class!(trait QAbstractListModel),

    uri: qt_property!(QString; NOTIFY uriChanged WRITE set_uri),
    uriChanged: qt_signal!(),

    loading: qt_property!(bool; NOTIFY stateChanged),
    stale: qt_property!(bool; NOTIFY stateChanged),
    large: qt_property!(bool; NOTIFY stateChanged),
    offline: qt_property!(bool; NOTIFY stateChanged),
    errorKind: qt_property!(QString; NOTIFY stateChanged),
    errorMessage: qt_property!(QString; NOTIFY stateChanged),
    count: qt_property!(i32; NOTIFY stateChanged),
    /// When the cached copy on screen was fetched (ms since the epoch), -1 if none.
    cachedAt: qt_property!(f64; NOTIFY stateChanged),
    capabilities: qt_property!(QVariantList; NOTIFY stateChanged),
    writable: qt_property!(bool; NOTIFY stateChanged),
    locationName: qt_property!(QString; NOTIFY stateChanged),
    title: qt_property!(QString; NOTIFY stateChanged),
    stateChanged: qt_signal!(),

    selectedCount: qt_property!(i32; NOTIFY selectionChanged),
    selectionChanged: qt_signal!(),

    sortKey: qt_property!(QString; NOTIFY prefsChanged),
    descending: qt_property!(bool; NOTIFY prefsChanged),
    foldersFirst: qt_property!(bool; NOTIFY prefsChanged),
    showHidden: qt_property!(bool; NOTIFY prefsChanged),
    viewMode: qt_property!(QString; NOTIFY prefsChanged),
    thumbnails: qt_property!(bool; NOTIFY prefsChanged),
    suggestGrid: qt_property!(bool; NOTIFY prefsChanged),
    prefsChanged: qt_signal!(),

    filterText: qt_property!(QString; NOTIFY filterChanged WRITE set_filter_text),
    /// Hide files (the folder picker).
    foldersOnly: qt_property!(bool; NOTIFY filterChanged WRITE set_folders_only),
    filterChanged: qt_signal!(),

    refresh: qt_method!(fn(&mut self)),
    stopLoading: qt_method!(fn(&mut self)),
    select: qt_method!(fn(&mut self, uri: QString)),
    toggle: qt_method!(fn(&mut self, uri: QString)),
    selectAll: qt_method!(fn(&mut self)),
    clearSelection: qt_method!(fn(&mut self)),
    selectedUris: qt_method!(fn(&self) -> QVariantList),
    setViewPrefs: qt_method!(fn(&mut self, prefs_json: QString) -> bool),
    indexOf: qt_method!(fn(&self, uri: QString) -> i32),
    hasCapability: qt_method!(fn(&self, flag: QString) -> bool),
    nameKind: qt_method!(fn(&self, name: QString) -> QString),
    setTransferState: qt_method!(fn(&mut self, uri: QString, state: QString)),
    rename: qt_method!(fn(&mut self, uri: QString, name: QString)),
    createFolder: qt_method!(fn(&mut self, name: QString)),
    createFile: qt_method!(fn(&mut self, name: QString)),
    renamed: qt_signal!(oldUri: QString, newUri: QString),
    renameFailed: qt_signal!(uri: QString, kind: QString, message: QString),
    created: qt_signal!(uri: QString, isDir: bool),
    createFailed: qt_signal!(kind: QString, message: QString),

    dir: Option<Uri>,
    local_dir: Option<PathBuf>,
    state: ListingState,
    prefs: ViewPrefs,
    /// The view was set here (`setViewPrefs`): no grid suggestion.
    prefs_set: bool,
    filter: FilterOptions,
    flags: Vec<String>,
    remote_thumbs: bool,
    shown: usize,
    generation: u64,
    task: Option<AbortHandle>,
    pending: Option<Vec<Entry>>,
    dirty: bool,
    watcher: Option<DirWatcher>,
    transfer_states: HashMap<Vec<u8>, String>,
}

impl DirectoryModel {
    // ---- loading ----

    fn set_uri(&mut self, value: QString) {
        if self.uri == value {
            return;
        }
        self.uri = value;
        self.uriChanged();
        self.load();
    }

    fn load(&mut self) {
        self.generation += 1;
        self.stop_tasks();
        self.reset_state();
        let parsed = Uri::parse(&self.uri.to_string());
        let (Some(core), Ok(dir)) = (core(), parsed) else {
            self.fail(&Error::new(ErrorKind::InvalidArgument, "bad folder address"));
            return;
        };
        self.prefs = core.view_prefs();
        self.prefs_set = false;
        self.remote_thumbs = core.settings().thumbnails_remote;
        self.apply_prefs_state();
        self.flags = core.capability_flags(&dir.location);
        self.local_dir = core.locations.to_local_path(&dir);
        let location = core.location(&dir.location);
        self.offline = location
            .as_ref()
            .is_some_and(|l| l.status == LocationStatus::Offline);
        let location_name = location.map(|l| l.name).unwrap_or_default();
        self.title = qs(&match dir.name() {
            Some(n) => String::from_utf8_lossy(n).into_owned(),
            None => location_name.clone(),
        });
        self.locationName = qs(&location_name);
        self.dir = Some(dir);
        self.loading = true;
        self.start_watch();
        self.start_listing(true);
        self.publish();
    }

    fn stop_tasks(&mut self) {
        if let Some(t) = self.task.take() {
            t.abort();
        }
        self.watcher = None;
        self.pending = None;
        self.dirty = false;
    }

    fn reset_state(&mut self) {
        self.begin_reset_model();
        self.state = ListingState::from_prefs(&self.prefs);
        self.state.set_filter(self.filter.clone());
        self.shown = 0;
        self.end_reset_model();
        self.errorKind = QString::default();
        self.errorMessage = QString::default();
        self.cachedAt = -1.0;
        self.suggestGrid = false;
        self.transfer_states.clear();
        self.dir = None;
        self.local_dir = None;
    }

    fn start_listing(&mut self, use_cache: bool) {
        let (Some(core), Some(dir)) = (core(), self.dir.clone()) else {
            return;
        };
        let me = QPointer::from(&*self);
        let generation = self.generation;
        let cb = queued_callback(move |ev: ListEvent| {
            if let Some(m) = me.as_pinned() {
                m.borrow_mut().on_event(generation, ev);
            }
        });
        let task = runtime::handle().spawn(async move {
            let (tx, mut rx) = unbounded_channel();
            let forward = async {
                while let Some(ev) = rx.recv().await {
                    cb(ev);
                }
            };
            tokio::join!(core.list_stream(&dir, use_cache, tx), forward);
        });
        self.task = Some(task.abort_handle());
    }

    fn start_watch(&mut self) {
        let Some(path) = self.local_dir.clone() else {
            return;
        };
        let Ok((watcher, mut rx)) = DirWatcher::new(DIR_DEBOUNCE) else {
            return;
        };
        if watcher.watch(&path).is_err() {
            return;
        }
        let me = QPointer::from(&*self);
        let generation = self.generation;
        let cb = queued_callback(move |()| {
            if let Some(m) = me.as_pinned() {
                m.borrow_mut().on_watch(generation);
            }
        });
        runtime::handle().spawn(async move {
            while rx.recv().await.is_some() {
                cb(());
            }
        });
        self.watcher = Some(watcher);
    }

    fn on_watch(&mut self, generation: u64) {
        if generation != self.generation {
            return;
        }
        if self.loading {
            self.dirty = true;
        } else {
            self.refresh();
        }
    }

    fn refresh(&mut self) {
        if self.dir.is_none() {
            return;
        }
        if let Some(t) = self.task.take() {
            t.abort();
        }
        self.pending = Some(Vec::new());
        self.loading = true;
        self.start_listing(false);
        self.publish();
    }

    fn stopLoading(&mut self) {
        if let Some(t) = self.task.take() {
            t.abort();
        }
        self.pending = None;
        self.state.finish();
        self.loading = false;
        self.publish();
    }

    fn on_event(&mut self, generation: u64, ev: ListEvent) {
        if generation != self.generation {
            return;
        }
        match ev {
            ListEvent::Cached { entries, fetched_ms } => {
                let changes = self.state.show_cached(entries);
                self.apply(changes);
                self.cachedAt = fetched_ms as f64;
                self.pending = Some(Vec::new());
            }
            ListEvent::Batch(batch) => match self.pending.as_mut() {
                Some(p) => p.extend(batch),
                None => {
                    let changes = self.state.apply_batch(batch);
                    self.apply(changes);
                }
            },
            ListEvent::Done(Ok(())) => self.finish_ok(),
            ListEvent::Done(Err(e)) => self.fail(&e),
        }
        self.publish();
    }

    fn finish_ok(&mut self) {
        match self.pending.take() {
            Some(all) => {
                let changes = self.state.replace_with_fresh(all);
                self.apply(changes);
            }
            None => self.state.finish(),
        }
        self.loading = false;
        self.offline = false;
        self.cachedAt = -1.0;
        self.errorKind = QString::default();
        self.errorMessage = QString::default();
        self.maybe_suggest_grid();
        if std::mem::take(&mut self.dirty) {
            self.refresh();
        }
    }

    fn maybe_suggest_grid(&mut self) {
        if self.prefs_set || self.prefs.view_mode != ViewMode::List || self.state.is_large() {
            return;
        }
        let entries: Vec<Entry> = self.state.visible_entries().cloned().collect();
        if suggest_grid(&entries) {
            self.suggestGrid = true;
            self.viewMode = qs(ViewMode::Grid.as_str());
            self.prefsChanged();
        }
    }

    fn fail(&mut self, e: &Error) {
        self.pending = None;
        self.loading = false;
        self.state.finish();
        if is_connectivity(e.kind) && self.state.total_len() > 0 {
            self.offline = true;
            self.state.set_stale(true);
        }
        self.errorKind = qs(e.kind.name());
        self.errorMessage = qs(&e.message);
        self.publish();
    }

    // ---- model plumbing ----

    fn apply(&mut self, changes: Vec<ListChange>) {
        for change in changes {
            match change {
                ListChange::Insert { index, count } => {
                    self.begin_insert_rows(index as i32, (index + count - 1) as i32);
                    self.shown += count;
                    self.end_insert_rows();
                }
                ListChange::Remove { index, count } => {
                    self.begin_remove_rows(index as i32, (index + count - 1) as i32);
                    self.shown = self.shown.saturating_sub(count);
                    self.end_remove_rows();
                }
                ListChange::Update { index } => {
                    let i = self.row_index(index as i32);
                    self.data_changed(i, i);
                }
                ListChange::Reset => self.reset_rows(),
            }
        }
        if self.shown != self.state.len() {
            self.reset_rows();
        }
    }

    fn reset_rows(&mut self) {
        self.begin_reset_model();
        self.shown = self.state.len();
        self.end_reset_model();
    }

    fn publish(&mut self) {
        self.count = self.state.len() as i32;
        self.stale = self.state.is_stale();
        self.large = self.state.is_large();
        self.writable =
            self.flags.iter().any(|f| f == cap::WRITE) && !self.flags.iter().any(|f| f == cap::READ_ONLY);
        let mut list = QVariantList::default();
        for f in &self.flags {
            list.push(QVariant::from(qs(f)));
        }
        self.capabilities = list;
        self.stateChanged();
        let selected = self.state.selection_len() as i32;
        if selected != self.selectedCount {
            self.selectedCount = selected;
            self.selectionChanged();
        }
    }

    fn entry_uri(&self, e: &Entry) -> Option<Uri> {
        self.dir.as_ref()?.join(&e.name).ok()
    }

    fn name_of(&self, uri: &QString) -> Option<Vec<u8>> {
        let u = Uri::parse(&uri.to_string()).ok()?;
        (u.parent().as_ref() == self.dir.as_ref()).then(|| u.name().map(<[u8]>::to_vec))?
    }

    fn thumbnail_source(&self, e: &Entry) -> String {
        if !self.prefs.thumbnails || !self.state.thumbnails_enabled() {
            return String::new();
        }
        let category = mime::category_of(e);
        let media = matches!(category, FileCategory::Image | FileCategory::Video);
        if !media || e.flags.contains(EntryFlags::NOT_ACCESSIBLE) {
            return String::new();
        }
        if let Some(dir) = &self.local_dir {
            return file_url(&dir.join(OsStr::from_bytes(&e.name)));
        }
        match (category, &self.remote_thumbs, self.entry_uri(e)) {
            (FileCategory::Image, true, Some(u)) => format!("image://lautta-thumb/{u}"),
            _ => String::new(),
        }
    }

    fn role_value(&self, e: &Entry, role: usize) -> QVariant {
        let time = |t: Option<i64>| QVariant::from(t.unwrap_or(-1) as f64);
        match role {
            0 => qs(&e.display_name()).into(),
            1 => e.name_is_lossy().into(),
            2 => qs(&self.entry_uri(e).map(|u| u.to_string()).unwrap_or_default()).into(),
            3 => e.is_dir().into(),
            4 => e.is_symlink().into(),
            5 => QVariant::from(if e.is_dir() {
                -1.0
            } else {
                e.size.map_or(-1.0, |s| s as f64)
            }),
            6 => time(e.modified_ms()),
            7 => time(e.created.map(lautta_core::entry::system_time_to_ms)),
            8 => QVariant::from(e.mode.map_or(-1.0, f64::from)),
            9 => qs(e.owner.as_deref().unwrap_or("")).into(),
            10 => qs(e.group.as_deref().unwrap_or("")).into(),
            11 => e.is_hidden().into(),
            12 => e.flags.contains(EntryFlags::READONLY).into(),
            13 => qs(&mime::mime_of(e).unwrap_or_default()).into(),
            14 | 15 => qs(mime::category_of(e).icon_name()).into(),
            16 => qs(&self.thumbnail_source(e)).into(),
            17 => self.state.is_selected(&e.name).into(),
            18 => qs(self.transfer_states.get(&e.name).map_or("", String::as_str)).into(),
            19 => e.flags.contains(EntryFlags::NOT_ACCESSIBLE).into(),
            _ => QVariant::default(),
        }
    }

    // ---- view settings, filter ----

    fn apply_prefs_state(&mut self) {
        let p = self.prefs;
        self.sortKey = qs(p.sort.as_str());
        self.descending = p.descending;
        self.foldersFirst = p.folders_first;
        self.showHidden = p.show_hidden;
        self.viewMode = qs(p.view_mode.as_str());
        self.thumbnails = p.thumbnails;
        self.filter.show_hidden = p.show_hidden;
        self.prefsChanged();
    }

    /// Shows the folder with other view settings. Storing them for every
    /// folder is `App.setSetting`'s job.
    fn setViewPrefs(&mut self, prefs_json: QString) -> bool {
        if self.dir.is_none() {
            return false;
        }
        let Some(map) = from_json::<serde_json::Map<String, serde_json::Value>>(&prefs_json.to_string())
        else {
            return false;
        };
        let mut p = self.prefs;
        if let Some(k) = map
            .get("sortKey")
            .and_then(|v| v.as_str())
            .and_then(SortKey::parse)
        {
            p.sort = k;
        }
        let flag = |key: &str, slot: &mut bool| {
            if let Some(b) = map.get(key).and_then(serde_json::Value::as_bool) {
                *slot = b;
            }
        };
        flag("descending", &mut p.descending);
        flag("foldersFirst", &mut p.folders_first);
        flag("showHidden", &mut p.show_hidden);
        flag("thumbnails", &mut p.thumbnails);
        if let Some(m) = map
            .get("viewMode")
            .and_then(|v| v.as_str())
            .and_then(ViewMode::parse)
        {
            p.view_mode = m;
        }
        self.prefs_set = true;
        self.prefs = p;
        self.suggestGrid = false;
        self.apply_prefs_state();
        let mut changes = self.state.set_sort(self.prefs.sort_options());
        changes.extend(self.state.set_filter(self.filter.clone()));
        self.apply(changes);
        self.reload_roles();
        self.publish();
        true
    }

    /// Thumbnails may have been switched: every row's data changed.
    fn reload_roles(&mut self) {
        if self.shown == 0 {
            return;
        }
        let (first, last) = (self.row_index(0), self.row_index(self.shown as i32 - 1));
        self.data_changed(first, last);
    }

    fn set_filter_text(&mut self, text: QString) {
        self.filterText = text;
        self.filter.set_text(&self.filterText.to_string());
        self.refilter();
    }

    fn set_folders_only(&mut self, value: bool) {
        self.foldersOnly = value;
        self.filter.folders_only = value;
        self.refilter();
    }

    fn refilter(&mut self) {
        let changes = self.state.set_filter(self.filter.clone());
        self.apply(changes);
        self.filterChanged();
        self.publish();
    }

    // ---- selection ----

    fn select(&mut self, uri: QString) {
        if let Some(name) = self.name_of(&uri) {
            let changes = self.state.select(&name);
            self.apply(changes);
            self.publish();
        }
    }

    fn toggle(&mut self, uri: QString) {
        if let Some(name) = self.name_of(&uri) {
            let changes = self.state.toggle(&name);
            self.apply(changes);
            self.publish();
        }
    }

    fn selectAll(&mut self) {
        let changes = self.state.select_all();
        self.apply(changes);
        self.publish();
    }

    fn clearSelection(&mut self) {
        let changes = self.state.clear_selection();
        self.apply(changes);
        self.publish();
    }

    fn selectedUris(&self) -> QVariantList {
        let mut out = QVariantList::default();
        let Some(dir) = &self.dir else { return out };
        for name in self.state.selected_names() {
            if let Ok(u) = dir.join(&name) {
                out.push(QVariant::from(qs(&u.to_string())));
            }
        }
        out
    }

    // ---- queries ----

    fn indexOf(&self, uri: QString) -> i32 {
        self.name_of(&uri)
            .and_then(|n| self.state.index_of(&n))
            .map_or(-1, |i| i as i32)
    }

    fn hasCapability(&self, flag: QString) -> bool {
        let flag = flag.to_string();
        self.flags.iter().any(|f| *f == flag)
    }

    /// "folder", "file" or "" for what is called `name` here (before filtering).
    fn nameKind(&self, name: QString) -> QString {
        let kind = match self.state.entry_named(name.to_string().as_bytes()) {
            Some(e) if e.is_dir() => "folder",
            Some(_) => "file",
            None => "",
        };
        qs(kind)
    }

    fn setTransferState(&mut self, uri: QString, state: QString) {
        let Some(name) = self.name_of(&uri) else { return };
        let state = state.to_string();
        if state.is_empty() {
            self.transfer_states.remove(&name);
        } else {
            self.transfer_states.insert(name.clone(), state);
        }
        if let Some(i) = self.state.index_of(&name) {
            let idx = self.row_index(i as i32);
            self.data_changed(idx, idx);
        }
    }

    // ---- changes made here ----

    fn rename(&mut self, uri: QString, name: QString) {
        let (Some(core), Ok(u)) = (core(), Uri::parse(&uri.to_string())) else {
            return;
        };
        let me = QPointer::from(&*self);
        let new_name = name.to_string();
        spawn_then(
            async move { core.rename(&u, new_name.as_bytes()).await },
            move |res| {
                let Some(m) = me.as_pinned() else { return };
                let mut m = m.borrow_mut();
                match res {
                    Ok(new) => {
                        m.renamed(uri, qs(&new.to_string()));
                        m.refresh();
                    }
                    Err(e) => m.renameFailed(uri, qs(e.kind.name()), qs(&e.message)),
                }
            },
        );
    }

    fn createFolder(&mut self, name: QString) {
        self.create(name, true);
    }

    fn createFile(&mut self, name: QString) {
        self.create(name, false);
    }

    fn create(&mut self, name: QString, folder: bool) {
        let (Some(core), Some(dir)) = (core(), self.dir.clone()) else {
            return;
        };
        let me = QPointer::from(&*self);
        let new_name = name.to_string();
        spawn_then(
            async move {
                if folder {
                    core.make_folder(&dir, new_name.as_bytes()).await
                } else {
                    core.make_file(&dir, new_name.as_bytes()).await
                }
            },
            move |res| {
                let Some(m) = me.as_pinned() else { return };
                let mut m = m.borrow_mut();
                match res {
                    Ok(new) => {
                        m.created(qs(&new.to_string()), folder);
                        m.refresh();
                    }
                    Err(e) => m.createFailed(qs(e.kind.name()), qs(&e.message)),
                }
            },
        );
    }
}

impl QAbstractListModel for DirectoryModel {
    fn row_count(&self) -> i32 {
        self.shown as i32
    }

    fn data(&self, index: QModelIndex, role: i32) -> QVariant {
        let (Ok(row), Ok(r)) = (usize::try_from(index.row()), usize::try_from(role - ROLE_BASE)) else {
            return QVariant::default();
        };
        match self.state.entry_at(row) {
            Some(e) => self.role_value(e, r),
            None => QVariant::default(),
        }
    }

    fn role_names(&self) -> HashMap<i32, QByteArray> {
        ROLES
            .iter()
            .enumerate()
            .map(|(i, n)| (ROLE_BASE + i as i32, QByteArray::from(*n)))
            .collect()
    }
}
