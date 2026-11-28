// SPDX-License-Identifier: LGPL-2.1-or-later

use crate::app::Core;
use crate::error::Result;
use crate::index::crawl::{Crawl, CrawlOutcome};
use crate::index::sched::{lower_this_thread, Conditions, Mode};
use crate::index::store::{IndexStore, IndexedEntry, RootInfo, RootState, QUERY_LIMIT};
use crate::locations::{Location, LocationKind};
use crate::search::SearchHit;
use crate::uri::{LocationId, Uri};
use crate::vpath::VPath;
use std::collections::HashSet;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex, MutexGuard, Weak};
use tokio::sync::watch;

pub const SERVER_REFRESH_MS: i64 = 6 * 60 * 60 * 1000;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct IndexTarget {
    pub location: LocationId,
    pub name: String,
    pub local: bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct IndexStatus {
    pub location: LocationId,
    pub name: String,
    pub local: bool,
    pub state: RootState,
    pub names: i64,
    pub checked_ms: Option<i64>,
    pub waits_for_wifi: bool,
}

struct Worker {
    stop: Arc<AtomicBool>,
    mode: Mode,
}

pub struct NameIndex {
    pub store: IndexStore,
    conditions: Mutex<Conditions>,
    worker: Mutex<Option<Worker>>,
    servers: Mutex<Option<Arc<AtomicBool>>>,
    refreshed: Mutex<HashSet<i64>>,
    changed: watch::Sender<u64>,
}

fn lock<T>(m: &Mutex<T>) -> MutexGuard<'_, T> {
    match m.lock() {
        Ok(g) => g,
        Err(p) => p.into_inner(),
    }
}

fn now_ms() -> i64 {
    crate::org::now_ms()
}

impl NameIndex {
    pub fn new(store: IndexStore) -> NameIndex {
        NameIndex {
            store,
            conditions: Mutex::default(),
            worker: Mutex::default(),
            servers: Mutex::default(),
            refreshed: Mutex::default(),
            changed: watch::channel(0).0,
        }
    }

    pub fn subscribe(&self) -> watch::Receiver<u64> {
        self.changed.subscribe()
    }

    fn notify(&self) {
        self.changed.send_modify(|n| *n = n.wrapping_add(1));
    }

    pub fn set_settings(&self, on_charger: bool, on_mobile: bool) {
        let mut conditions = lock(&self.conditions);
        conditions.index_on_charger = on_charger;
        conditions.index_on_mobile = on_mobile;
    }

    pub fn conditions(&self) -> Conditions {
        *lock(&self.conditions)
    }

    pub fn mode(&self) -> Mode {
        self.conditions().mode()
    }

    pub fn running(&self) -> Option<Mode> {
        lock(&self.worker).as_ref().map(|w| w.mode)
    }

    pub fn servers_running(&self) -> bool {
        lock(&self.servers).is_some()
    }
}

pub fn indexed_by_default(location: &Location) -> bool {
    matches!(
        location.kind,
        LocationKind::UserFolder | LocationKind::Android | LocationKind::Volume
    )
}

fn hidden_below(root: &VPath, folder: &VPath) -> bool {
    folder
        .strip_prefix(root)
        .is_some_and(|rel| rel.components().any(|c| c.first() == Some(&b'.')))
}

pub fn index_hit(
    location: &str,
    root_path: &VPath,
    found: IndexedEntry,
    include_hidden: bool,
) -> Option<SearchHit> {
    if !include_hidden && hidden_below(root_path, &found.folder) {
        return None;
    }
    let folder_uri = Uri::new(location.to_owned(), found.folder);
    let uri = folder_uri.join(&found.entry.name).ok()?;
    Some(SearchHit {
        uri,
        entry: found.entry,
        folder_uri,
    })
}

impl Core {
    pub fn index_targets(&self) -> Vec<IndexTarget> {
        self.locations
            .locations()
            .into_iter()
            .filter(|l| {
                indexed_by_default(l)
                    || (matches!(l.kind, LocationKind::Server { .. })
                        && self.location_prefs.get(&l.id).is_ok_and(|p| p.index_names))
            })
            .map(|l| IndexTarget {
                local: l.is_local(),
                location: l.id,
                name: l.name,
            })
            .collect()
    }

    pub fn index_status(&self) -> Vec<IndexStatus> {
        let Some(index) = &self.index else {
            return Vec::new();
        };
        let roots = index.store.roots().unwrap_or_default();
        let no_wifi = index.conditions().waits_for_wifi();
        self.index_targets()
            .into_iter()
            .map(|t| {
                let root = roots
                    .iter()
                    .find(|r| r.location == t.location && r.path.is_root());
                let state = root.map_or(RootState::Pending, |r| r.state);
                IndexStatus {
                    waits_for_wifi: no_wifi && !t.local && state == RootState::Pending,
                    state,
                    names: root.map_or(0, |r| r.names),
                    checked_ms: root.and_then(|r| r.checked_ms),
                    location: t.location,
                    name: t.name,
                    local: t.local,
                }
            })
            .collect()
    }

    pub fn index_ready_root(&self, location: &str) -> Option<RootInfo> {
        let index = self.index.as_ref()?;
        if !self.index_targets().iter().any(|t| t.location == location) {
            return None;
        }
        index.store.ready_root(location, &VPath::root())
    }

    pub fn index_query(
        &self,
        root: &RootInfo,
        text: &str,
        include_hidden: bool,
        keep: &dyn Fn(&SearchHit) -> bool,
    ) -> Vec<SearchHit> {
        let Some(index) = &self.index else {
            return Vec::new();
        };
        let mut hits = Vec::new();
        let _ = index.store.query_each(&[root.id], text, |found| {
            if let Some(hit) = index_hit(&root.location, &root.path, found, include_hidden) {
                if keep(&hit) {
                    hits.push(hit);
                }
            }
            hits.len() < QUERY_LIMIT
        });
        hits
    }

    pub async fn index_refresh_root(&self, root: &RootInfo, stop: &AtomicBool) -> Result<CrawlOutcome> {
        let Some(index) = &self.index else {
            return Ok(CrawlOutcome::Done);
        };
        let provider = self.provider(&root.location)?;
        let now = now_ms;
        let out = Crawl {
            store: &index.store,
            provider: provider.as_ref(),
            stop,
            now_ms: &now,
            events: None,
        }
        .run(root)
        .await;
        if matches!(out, Ok(CrawlOutcome::Done)) {
            lock(&index.refreshed).insert(root.id);
        }
        index.notify();
        out
    }

    pub fn index_set_conditions(self: &Arc<Self>, change: impl FnOnce(&mut Conditions)) {
        let Some(index) = &self.index else { return };
        let waited = index.conditions().waits_for_wifi();
        change(&mut lock(&index.conditions));
        if index.conditions().waits_for_wifi() != waited {
            index.notify();
        }
        self.index_kick();
    }

    pub fn index_restart(self: &Arc<Self>) {
        let Some(index) = &self.index else { return };
        if let Some(w) = lock(&index.worker).take() {
            w.stop.store(true, Ordering::Relaxed);
        }
        if let Some(stop) = lock(&index.servers).take() {
            stop.store(true, Ordering::Relaxed);
        }
        self.index_kick();
    }

    pub fn index_kick(self: &Arc<Self>) {
        let Some(index) = &self.index else { return };
        self.index_kick_servers(index);
        let mode = index.mode();
        let mut worker = lock(&index.worker);
        if let Some(w) = worker.as_ref() {
            if w.mode == mode {
                return;
            }
            w.stop.store(true, Ordering::Relaxed);
            *worker = None;
        }
        if mode == Mode::Paused {
            return;
        }
        let stop = Arc::new(AtomicBool::new(false));
        *worker = Some(Worker {
            stop: stop.clone(),
            mode,
        });
        drop(worker);
        let core = Arc::downgrade(self);
        match mode {
            Mode::Idle => {
                let spawned = std::thread::Builder::new()
                    .name("lautta-index".into())
                    .spawn(move || {
                        lower_this_thread();
                        let rt = tokio::runtime::Builder::new_current_thread()
                            .enable_all()
                            .on_thread_start(lower_this_thread)
                            .build();
                        if let Ok(rt) = rt {
                            rt.block_on(run_worker(core, stop));
                        }
                    });
                if let Err(e) = spawned {
                    log::warn!("index worker not started: {e}");
                }
            }
            _ => {
                self.runtime.spawn(run_worker(core, stop));
            }
        }
    }

    fn index_kick_servers(self: &Arc<Self>, index: &NameIndex) {
        let mut servers = lock(&index.servers);
        if !index.conditions().servers_allowed() {
            if let Some(stop) = servers.take() {
                stop.store(true, Ordering::Relaxed);
            }
            return;
        }
        if servers.is_some() {
            return;
        }
        let stop = Arc::new(AtomicBool::new(false));
        *servers = Some(stop.clone());
        drop(servers);
        self.runtime.spawn(run_servers(Arc::downgrade(self), stop));
    }

    fn index_due(&self, root: &RootInfo, local: bool) -> bool {
        let Some(index) = &self.index else { return false };
        match root.state {
            RootState::Pending | RootState::Crawling | RootState::Failed => true,
            RootState::Ready if local => !lock(&index.refreshed).contains(&root.id),
            RootState::Ready => root.checked_ms.is_none_or(|t| now_ms() - t > SERVER_REFRESH_MS),
        }
    }

    pub fn index_forget_unwanted(&self) {
        let Some(index) = &self.index else { return };
        let targets: HashSet<String> = self.index_targets().into_iter().map(|t| t.location).collect();
        for root in index.store.roots().unwrap_or_default() {
            let known = self.locations.get(&root.location).is_some();
            if known && !targets.contains(&root.location) {
                let _ = index.store.remove_root(root.id);
            }
        }
    }
}

async fn refresh_due(core: &Weak<Core>, stop: &Arc<AtomicBool>, local: bool) {
    let Some(c) = core.upgrade() else { return };
    c.index_forget_unwanted();
    let targets = c.index_targets();
    drop(c);
    for target in targets {
        if stop.load(Ordering::Relaxed) {
            break;
        }
        if target.local != local {
            continue;
        }
        let Some(c) = core.upgrade() else { return };
        let Some(index) = &c.index else { return };
        let Ok(root) = index.store.root(&target.location, &VPath::root()) else {
            continue;
        };
        if !c.index_due(&root, target.local) {
            continue;
        }
        if let Err(e) = c.index_refresh_root(&root, stop).await {
            log::info!("index of {} not refreshed: {e}", target.location);
        }
    }
}

async fn run_servers(core: Weak<Core>, stop: Arc<AtomicBool>) {
    refresh_due(&core, &stop, false).await;
    if let Some(c) = core.upgrade() {
        if let Some(index) = &c.index {
            let mut servers = lock(&index.servers);
            if servers.as_ref().is_some_and(|s| Arc::ptr_eq(s, &stop)) {
                *servers = None;
            }
            drop(servers);
            index.notify();
        }
    }
}

async fn run_worker(core: Weak<Core>, stop: Arc<AtomicBool>) {
    refresh_due(&core, &stop, true).await;
    if let Some(c) = core.upgrade() {
        if let Some(index) = &c.index {
            let mut worker = lock(&index.worker);
            if worker.as_ref().is_some_and(|w| Arc::ptr_eq(&w.stop, &stop)) {
                *worker = None;
            }
            drop(worker);
            index.notify();
        }
    }
}
