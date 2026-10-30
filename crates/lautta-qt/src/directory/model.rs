// SPDX-License-Identifier: LGPL-2.1-or-later
//! `DirectoryModel { uri }` (doc/QML-API.md, SPEC §9): a folder listing on
//! top of `lautta_core::listing::ListingState`. Listings stream in from the
//! core (cached copy first, BRW-5), diffs become row inserts/removes/updates,
//! local folders are watched (BRW-6) and the view settings are the global
//! ones (BRW-4), followed while the folder is open.

use crate::browse::Guard;
use crate::json::from_json;
use crate::runtime::{self, core, spawn_then};
use lautta_core::app::Core;
use lautta_core::app_directory::{file_url, ListEvent};
use lautta_core::entry::{cap, Entry, EntryFlags};
use lautta_core::error::ErrorKind;
use lautta_core::filter::FilterOptions;
use lautta_core::foldercount::FolderCount;
use lautta_core::listing::{ListChange, ListingState};
use lautta_core::locations::{LocationKind, LocationStatus};
use lautta_core::mime;
use lautta_core::sort::SortKey;
use lautta_core::thumbs::{app_thumbnail_url, thumbnail_source, ThumbSettings, ThumbSource};
use lautta_core::thumbs_text::{thumb_monospace, type_icon};
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
const ROLES: [&str; 26] = [
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
    "itemCount",
    "itemCountCapped",
    "textThumb",
    "textPreview",
    "textPreviewMono",
    "typeIcon",
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

/// What an item count was made for: it is counted again when the folder
/// changed (its modified time) or the hidden files were switched.
#[derive(Clone, Copy, PartialEq, Eq)]
struct CountStamp {
    modified: Option<i64>,
    show_hidden: bool,
}

/// The count of one folder in the view, `result` `None` until it is made
/// or when the folder cannot be read.
struct CountSlot {
    stamp: CountStamp,
    result: Option<FolderCount>,
}

/// What a text thumbnail was made for: made again when the file changed.
#[derive(Clone, Copy, PartialEq, Eq)]
struct PreviewStamp {
    modified: Option<i64>,
    size: Option<u64>,
}

/// The text thumbnail of one file in the view (PRV-15): `key` while it is
/// made, then `text`, `None` when the file has none.
struct PreviewSlot {
    stamp: PreviewStamp,
    key: Option<String>,
    text: Option<String>,
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
    /// Folders here have an item count (`requestCount`): the folder is on
    /// this device or a drive, not on a server or in an archive.
    folderCounts: qt_property!(bool; NOTIFY stateChanged),
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
    revisit: qt_method!(fn(&mut self)),
    stopLoading: qt_method!(fn(&mut self)),
    select: qt_method!(fn(&mut self, uri: QString)),
    toggle: qt_method!(fn(&mut self, uri: QString)),
    selectAll: qt_method!(fn(&mut self)),
    clearSelection: qt_method!(fn(&mut self)),
    selectedUris: qt_method!(fn(&self) -> QVariantList),
    setViewPrefs: qt_method!(fn(&mut self, prefs_json: QString) -> bool),
    indexOf: qt_method!(fn(&self, uri: QString) -> i32),
    /// The uri of row `index`, empty outside the rows (drag selection, SPEC §15.3).
    uriAt: qt_method!(fn(&self, index: i32) -> QString),
    hasCapability: qt_method!(fn(&self, flag: QString) -> bool),
    nameKind: qt_method!(fn(&self, name: QString) -> QString),
    /// "folder", "file" or "" for the item `uri` of this folder.
    kindOf: qt_method!(fn(&self, uri: QString) -> QString),
    /// Counts the items of the folder `uri` for the `itemCount` role,
    /// off the GUI thread; asks again only when the folder changed.
    requestCount: qt_method!(fn(&mut self, uri: QString)),
    /// Makes the `textPreview` of the file `uri` (PRV-15) in the thumbnail
    /// jobs; asks again only when the file changed.
    requestPreview: qt_method!(fn(&mut self, uri: QString)),
    /// The cell of `uri` went: a preview still queued is dropped.
    cancelPreview: qt_method!(fn(&mut self, uri: QString)),
    setTransferState: qt_method!(fn(&mut self, uri: QString, state: QString)),
    rename: qt_method!(fn(&mut self, uri: QString, name: QString)),
    createFolder: qt_method!(fn(&mut self, name: QString)),
    renamed: qt_signal!(oldUri: QString, newUri: QString),
    renameFailed: qt_signal!(uri: QString, kind: QString, message: QString),
    created: qt_signal!(uri: QString),
    createFailed: qt_signal!(kind: QString, message: QString),

    dir: Option<Uri>,
    local_dir: Option<PathBuf>,
    state: ListingState,
    prefs: ViewPrefs,
    /// The global view settings `prefs` last followed (BRW-4).
    global: ViewPrefs,
    /// The view was set here (`setViewPrefs`) or for every folder since
    /// the folder opened: no grid suggestion.
    prefs_set: bool,
    filter: FilterOptions,
    flags: Vec<String>,
    /// The settings that pick the files with a thumbnail, as of the last
    /// load or change; none before the folder is open.
    thumb_settings: Option<ThumbSettings>,
    shown: usize,
    generation: u64,
    task: Option<AbortHandle>,
    pending: Option<Vec<Entry>>,
    dirty: bool,
    /// Archives do not change while open: coming back never lists them again.
    archive: bool,
    watcher: Option<DirWatcher>,
    /// Ends the task following the settings when the model goes.
    settings_watch: Option<Guard>,
    transfer_states: HashMap<Vec<u8>, String>,
    /// Item counts of the folders in this one, by name; asked for by the
    /// cells that show them.
    counts: HashMap<Vec<u8>, CountSlot>,
    /// Text thumbnails of the files in this folder, by name.
    previews: HashMap<Vec<u8>, PreviewSlot>,
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
        self.global = self.prefs;
        self.prefs_set = false;
        self.thumb_settings = Some(ThumbSettings::new(&core.settings()));
        if self.settings_watch.is_none() {
            self.watch_settings(&core);
        }
        self.apply_prefs_state();
        self.flags = core.capability_flags(&dir.location);
        self.local_dir = core.locations.to_local_path(&dir);
        self.folderCounts = self.local_dir.is_some();
        let location = core.location(&dir.location);
        self.archive = location.as_ref().is_some_and(|l| l.kind == LocationKind::Archive);
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
        self.counts.clear();
        self.drop_previews();
        self.folderCounts = false;
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

    /// Follows the settings while the model lives: a change made on another
    /// page (Settings, or View options in another folder) shows here at
    /// once (BRW-4), not only when the folder is opened again.
    fn watch_settings(&mut self, core: &Core) {
        let guard = Guard::default();
        let mut stop = guard.subscribe();
        let mut changes = core.watch_settings();
        let me = QPointer::from(&*self);
        let cb = queued_callback(move |()| {
            if let Some(m) = me.as_pinned() {
                m.borrow_mut().follow_settings();
            }
        });
        runtime::handle().spawn(async move {
            loop {
                tokio::select! {
                    r = changes.changed() => if r.is_err() { return },
                    _ = stop.changed() => return,
                }
                changes.borrow_and_update();
                cb(());
            }
        });
        self.settings_watch = Some(guard);
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

    /// The folder is on screen again (BRW-11): a folder nobody watches is
    /// listed again, the cached rows staying until the fresh ones arrive.
    fn revisit(&mut self) {
        if self.watcher.is_none() && !self.archive && !self.loading {
            self.refresh();
        }
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

    fn thumb_source(&self, e: &Entry) -> ThumbSource {
        match &self.thumb_settings {
            Some(settings) if self.state.thumbnails_enabled() => {
                thumbnail_source(e, self.local_dir.is_some(), self.prefs.thumbnails, settings)
            }
            _ => ThumbSource::Icon,
        }
    }

    fn thumbnail_source(&self, e: &Entry) -> String {
        match self.thumb_source(e) {
            ThumbSource::Icon | ThumbSource::Text => String::new(),
            ThumbSource::System => self
                .local_dir
                .as_ref()
                .map_or_else(String::new, |dir| file_url(&dir.join(OsStr::from_bytes(&e.name)))),
            ThumbSource::App => self
                .entry_uri(e)
                .map_or_else(String::new, |u| app_thumbnail_url(&u)),
        }
    }

    fn preview_stamp(e: &Entry) -> PreviewStamp {
        PreviewStamp {
            modified: e.modified_ms(),
            size: e.size,
        }
    }

    /// The text thumbnail of `e` as it is now, empty while it is made or
    /// with thumbnails off.
    fn preview_of(&self, e: &Entry) -> &str {
        if self.thumb_source(e) != ThumbSource::Text {
            return "";
        }
        self.previews
            .get(&e.name)
            .filter(|slot| slot.stamp == Self::preview_stamp(e))
            .and_then(|slot| slot.text.as_deref())
            .unwrap_or("")
    }

    fn stamp_of(&self, e: &Entry) -> CountStamp {
        CountStamp {
            modified: e.modified_ms(),
            show_hidden: self.prefs.show_hidden,
        }
    }

    /// The count of the folder `e` as it is now; none while it is being
    /// made again after a change.
    fn count_of(&self, e: &Entry) -> Option<FolderCount> {
        let slot = self.counts.get(&e.name)?;
        if slot.stamp == self.stamp_of(e) {
            slot.result
        } else {
            None
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
            20 => self.count_of(e).map_or(-1, |c| count_role(c.shown)).into(),
            21 => self.count_of(e).is_some_and(|c| c.capped).into(),
            22 => (self.thumb_source(e) == ThumbSource::Text).into(),
            23 => qs(self.preview_of(e)).into(),
            24 => thumb_monospace(&e.name).into(),
            25 => qs(type_icon(&e.name)).into(),
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
        // A suggested grid (SPEC 15.3) stays until a view is chosen.
        let mode = if self.suggestGrid {
            ViewMode::Grid
        } else {
            p.view_mode
        };
        self.viewMode = qs(mode.as_str());
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
            self.view_chosen();
        }
        self.show_prefs(p);
        true
    }

    /// The global settings changed. What changed shows here at once; what
    /// was set for this folder alone and did not change stays (BRW-4).
    fn follow_settings(&mut self) {
        let Some(core) = core() else { return };
        let global = core.view_prefs();
        let thumbs = Some(ThumbSettings::new(&core.settings()));
        if global == self.global && thumbs == self.thumb_settings {
            return;
        }
        if global.view_mode != self.global.view_mode {
            self.view_chosen();
        }
        let prefs = self.prefs.follow(self.global, global);
        self.global = global;
        self.thumb_settings = thumbs;
        self.show_prefs(prefs);
    }

    /// A view was chosen for this folder or for all: it replaces a
    /// suggested grid and no other is suggested.
    fn view_chosen(&mut self) {
        self.prefs_set = true;
        self.suggestGrid = false;
    }

    /// Shows the folder with the view settings `p`.
    fn show_prefs(&mut self, p: ViewPrefs) {
        self.prefs = p;
        self.apply_prefs_state();
        let mut changes = self.state.set_sort(self.prefs.sort_options());
        changes.extend(self.state.set_filter(self.filter.clone()));
        self.apply(changes);
        self.reload_roles();
        self.publish();
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

    fn uriAt(&self, index: i32) -> QString {
        usize::try_from(index)
            .ok()
            .filter(|&i| i < self.shown)
            .and_then(|i| self.state.entry_at(i))
            .and_then(|e| self.entry_uri(e))
            .map_or_else(QString::default, |u| qs(&u.to_string()))
    }

    fn hasCapability(&self, flag: QString) -> bool {
        let flag = flag.to_string();
        self.flags.contains(&flag)
    }

    /// "folder", "file" or "" for what is called `name` here (before filtering).
    fn nameKind(&self, name: QString) -> QString {
        qs(kind_of(self.state.entry_named(name.to_string().as_bytes())))
    }

    /// By uri, so names that are not UTF-8 are found too (the selection
    /// panel's one item, SPEC §15.3).
    fn kindOf(&self, uri: QString) -> QString {
        let name = self.name_of(&uri).unwrap_or_default();
        qs(kind_of(self.state.entry_named(&name)))
    }

    fn requestCount(&mut self, uri: QString) {
        let (Some(core), Some(name)) = (core(), self.name_of(&uri)) else {
            return;
        };
        let Some(entry) = self.state.entry_named(&name) else {
            return;
        };
        if !self.folderCounts || !entry.is_dir() || entry.flags.contains(EntryFlags::NOT_ACCESSIBLE) {
            return;
        }
        let stamp = self.stamp_of(entry);
        if self.counts.get(&name).is_some_and(|slot| slot.stamp == stamp) {
            return;
        }
        let Some(folder) = self.entry_uri(entry) else {
            return;
        };
        self.counts
            .insert(name.clone(), CountSlot { stamp, result: None });
        let me = QPointer::from(&*self);
        let generation = self.generation;
        spawn_then(
            async move { core.count_folder(&folder, stamp.show_hidden).await },
            move |result| {
                if let Some(m) = me.as_pinned() {
                    m.borrow_mut().counted(generation, &name, stamp, result);
                }
            },
        );
    }

    /// A count arrived. One made for an older state of the folder, or for
    /// another folder, is dropped: the cell asked again.
    fn counted(&mut self, generation: u64, name: &[u8], stamp: CountStamp, result: Option<FolderCount>) {
        if generation != self.generation {
            return;
        }
        let Some(slot) = self.counts.get_mut(name).filter(|slot| slot.stamp == stamp) else {
            return;
        };
        slot.result = result;
        if let Some(i) = self.state.index_of(name) {
            let idx = self.row_index(i as i32);
            self.data_changed(idx, idx);
        }
    }

    fn requestPreview(&mut self, uri: QString) {
        let (Some(core), Some(name)) = (core(), self.name_of(&uri)) else {
            return;
        };
        let Some(entry) = self.state.entry_named(&name) else {
            return;
        };
        if self.thumb_source(entry) != ThumbSource::Text {
            return;
        }
        let stamp = Self::preview_stamp(entry);
        if self.previews.get(&name).is_some_and(|slot| slot.stamp == stamp) {
            return;
        }
        let (Some(file), Ok(previews)) = (self.entry_uri(entry), core.previews(&runtime::handle())) else {
            return;
        };
        if let Some(old) = self.previews.get(&name).and_then(|slot| slot.key.as_deref()) {
            previews.cancel_text(old);
        }
        let ticket = previews.request_text(core, file);
        let key = ticket.key.clone();
        self.previews.insert(
            name.clone(),
            PreviewSlot {
                stamp,
                key: Some(ticket.key),
                text: None,
            },
        );
        let me = QPointer::from(&*self);
        spawn_then(async move { ticket.rx.await.ok().flatten() }, move |text| {
            if let Some(m) = me.as_pinned() {
                m.borrow_mut().previewed(&key, &name, text);
            }
        });
    }

    /// A text thumbnail arrived. Only the slot's pending request counts: keys
    /// are never reused, so a cancelled or superseded one is dropped.
    fn previewed(&mut self, key: &str, name: &[u8], text: Option<String>) {
        let Some(slot) = self
            .previews
            .get_mut(name)
            .filter(|slot| slot.key.as_deref() == Some(key))
        else {
            return;
        };
        slot.key = None;
        slot.text = text;
        if let Some(i) = self.state.index_of(name) {
            let idx = self.row_index(i as i32);
            self.data_changed(idx, idx);
        }
    }

    fn cancelPreview(&mut self, uri: QString) {
        let Some(name) = self.name_of(&uri) else { return };
        if let Some(key) = self.previews.get(&name).and_then(|slot| slot.key.clone()) {
            self.previews.remove(&name);
            cancel_text(&key);
        }
    }

    /// Previews still queued are dropped with the folder.
    fn drop_previews(&mut self) {
        for slot in std::mem::take(&mut self.previews).into_values() {
            if let Some(key) = slot.key {
                cancel_text(&key);
            }
        }
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
        let (Some(core), Some(dir)) = (core(), self.dir.clone()) else {
            return;
        };
        let me = QPointer::from(&*self);
        let new_name = name.to_string();
        spawn_then(
            async move { core.make_folder(&dir, new_name.as_bytes()).await },
            move |res| {
                let Some(m) = me.as_pinned() else { return };
                let mut m = m.borrow_mut();
                match res {
                    Ok(new) => {
                        m.created(qs(&new.to_string()));
                        m.refresh();
                    }
                    Err(e) => m.createFailed(qs(e.kind.name()), qs(&e.message)),
                }
            },
        );
    }
}

fn cancel_text(key: &str) {
    if let Some(previews) = core().and_then(|core| core.previews(&runtime::handle()).ok()) {
        previews.cancel_text(key);
    }
}

/// The `itemCount` role: the count, `-1` when there is none.
fn count_role(shown: usize) -> i32 {
    i32::try_from(shown).unwrap_or(i32::MAX)
}

/// What `nameKind` and `kindOf` answer for an entry or none.
fn kind_of(entry: Option<&Entry>) -> &'static str {
    match entry {
        Some(e) if e.is_dir() => "folder",
        Some(_) => "file",
        None => "",
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

#[cfg(test)]
mod tests {
    use super::*;

    fn text_of<'a>(m: &'a DirectoryModel, name: &[u8]) -> Option<(Option<&'a str>, Option<&'a str>)> {
        m.previews
            .get(name)
            .map(|slot| (slot.key.as_deref(), slot.text.as_deref()))
    }

    #[test]
    fn a_text_thumbnail_counts_only_for_the_request_still_pending() {
        let mut m = DirectoryModel::default();
        let stamp = PreviewStamp {
            modified: Some(1),
            size: Some(3),
        };
        m.previews.insert(
            b"a.txt".to_vec(),
            PreviewSlot {
                stamp,
                key: Some("text:2:a".into()),
                text: None,
            },
        );
        m.previewed("text:1:a", b"a.txt", Some("old".into()));
        assert_eq!(
            text_of(&m, b"a.txt"),
            Some((Some("text:2:a"), None)),
            "a cancelled request's answer is dropped"
        );
        m.previewed("text:2:a", b"a.txt", Some("hei".into()));
        assert_eq!(text_of(&m, b"a.txt"), Some((None, Some("hei"))));
        m.previewed("text:2:a", b"a.txt", None);
        assert_eq!(text_of(&m, b"a.txt"), Some((None, Some("hei"))), "answered once");
        m.previewed("text:3:b", b"b.txt", Some("b".into()));
        assert_eq!(text_of(&m, b"b.txt"), None, "never asked for");
    }
}
