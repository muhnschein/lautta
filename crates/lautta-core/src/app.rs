// SPDX-License-Identifier: LGPL-2.1-or-later
//! The app context: one [`Core`] wires the stores, the location registry and
//! the transfer engine together (SPEC §5.3) and offers the user-level
//! actions the UI calls. Everything here is Qt-free; the Qt layer only
//! forwards to these methods and turns results into model updates.

use crate::bridge::{Attention, BridgeClient, BridgeConfig, BridgeStatus, RemoteKind, RemoteLocation};
use crate::db::Db;
use crate::dircache::DirCache;
use crate::entry::{cap, Entry};
use crate::error::{Error, ErrorKind, Result};
use crate::locations::{Location, LocationKind, LocationRegistry, LocationStatus};
use crate::ops::plan::{PlanOptions, Planner};
use crate::ops::{OperationKind, Plan};
use crate::org::favourites::Favourites;
use crate::org::recents::Recents;
use crate::paths::AppPaths;
use crate::provider::archive::{archive_location_id, ArchiveProvider};
use crate::provider::{list_all, Lane, Provider, ProviderResolver, RenameMode};
use crate::questions::Questions;
use crate::search::RecentSearches;
use crate::settings::{LocationPrefsStore, Settings};
use crate::sort::SortKey;
use crate::transfer::clock::SystemClock;
use crate::transfer::{Engine, EngineConfig, EngineDeps, TransferId, TransferOptions, Trasher};
use crate::trash::Trash;
use crate::undo::{UndoAction, UndoRecorder, UndoStep};
use crate::uri::Uri;
use crate::viewprefs::{ViewMode, ViewPrefs};
use crate::workcopy::WorkingCopies;
use async_trait::async_trait;
use std::sync::atomic::AtomicBool;
use std::sync::{Arc, Mutex, MutexGuard};

/// Listings kept in memory (BRW-5).
const DIRCACHE_ENTRIES: usize = 64;

/// The result of starting an operation (OPS-1).
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Started {
    /// Queued as a transfer.
    Transfer(TransferId),
    /// Over the large-operation threshold: the UI shows the summary sheet and
    /// calls [`Core::start_plan`] when the user confirms.
    NeedsSummary(Box<Plan>),
}

/// Shared application state. Create one per process with [`Core::open`].
pub struct Core {
    pub paths: AppPaths,
    pub db: Db,
    pub locations: LocationRegistry,
    pub engine: Engine,
    pub trash: Trash,
    pub dircache: DirCache,
    pub favourites: Favourites,
    pub recents: Recents,
    pub recent_searches: RecentSearches,
    pub location_prefs: LocationPrefsStore,
    pub working_copies: WorkingCopies,
    /// Pending prompts (bridge identity and sign-in questions, NVB-6).
    pub questions: Arc<Questions>,
    /// The netvfs bridge client (SPEC §7); `None` only in tests without one.
    pub bridge: Option<BridgeClient>,
    settings: Mutex<Settings>,
    undo: Mutex<UndoRecorder>,
}

fn lock<T>(m: &Mutex<T>) -> MutexGuard<'_, T> {
    match m.lock() {
        Ok(g) => g,
        Err(p) => p.into_inner(),
    }
}

/// Moves local items to Recently deleted when their location keeps a trash
/// (OPS-8); everything else is deleted permanently by the engine.
struct LocalTrasher {
    locations: LocationRegistry,
    trash: Trash,
}

#[async_trait]
impl Trasher for LocalTrasher {
    async fn trash(&self, uri: &Uri) -> Result<bool> {
        trash_one(&self.locations, &self.trash, uri)
            .await
            .map(|id| id.is_some())
    }
}

/// Trashes one item; `None` when its location does not keep a trash or the
/// item is on another file system (then the caller deletes permanently).
async fn trash_one(locations: &LocationRegistry, trash: &Trash, uri: &Uri) -> Result<Option<i64>> {
    let provider = locations.provider(&uri.location)?;
    if !provider.capabilities().has(cap::TRASH) {
        return Ok(None);
    }
    let Some(path) = locations.to_local_path(uri) else {
        return Ok(None);
    };
    let trash = trash.clone();
    let uri = uri.clone();
    let res = tokio::task::spawn_blocking(move || trash.trash(&path, &uri))
        .await
        .map_err(|e| Error::new(ErrorKind::Internal, e.to_string()))?;
    match res {
        Ok(item) => Ok(Some(item.id)),
        Err(e) if e.kind == ErrorKind::CrossesDevice => Ok(None),
        Err(e) => Err(e),
    }
}

impl Core {
    /// Opens the database and the stores, scans the locations and starts the
    /// transfer engine. Must be called inside a tokio runtime.
    pub async fn open(paths: AppPaths) -> Result<Arc<Core>> {
        let bridge = BridgeConfig::for_app(&paths);
        Core::open_full(paths.clone(), LocationRegistry::new(paths), Some(bridge)).await
    }

    /// As [`Core::open`] with a prepared registry (tests use a temporary
    /// media root) and no bridge client.
    pub async fn open_with(paths: AppPaths, locations: LocationRegistry) -> Result<Arc<Core>> {
        Core::open_full(paths, locations, None).await
    }

    /// As [`Core::open`] with a prepared registry and an optional bridge
    /// configuration. Bridge locations join the registry as they come and
    /// go (NVB-4); transfers wait while the bridge is away (NVB-12).
    pub async fn open_full(
        paths: AppPaths,
        locations: LocationRegistry,
        bridge: Option<BridgeConfig>,
    ) -> Result<Arc<Core>> {
        let db_path = paths.database();
        let db = tokio::task::spawn_blocking(move || Db::open(&db_path))
            .await
            .map_err(|e| Error::new(ErrorKind::Internal, e.to_string()))??;
        locations.refresh();
        let trash = Trash::new(paths.trash_dir(), db.clone());
        let mut deps = EngineDeps::new(db.clone(), Arc::new(locations.clone()));
        deps.trasher = Some(Arc::new(LocalTrasher {
            locations: locations.clone(),
            trash: trash.clone(),
        }));
        let cfg = EngineConfig {
            scratch_dir: paths.cache_dir(),
            ..EngineConfig::default()
        };
        let engine = Engine::new(deps, cfg);
        let settings = Settings::default();
        let questions = Questions::new();
        let client = match bridge {
            Some(cfg) => Some(BridgeClient::start(cfg, questions.clone())?),
            None => None,
        };
        let core = Core {
            dircache: DirCache::new(db.clone(), DIRCACHE_ENTRIES)?,
            favourites: Favourites::new(db.clone()),
            recents: Recents::new(db.clone()),
            recent_searches: RecentSearches::new(db.clone()),
            location_prefs: LocationPrefsStore::new(db.clone()),
            working_copies: WorkingCopies::new(db.clone(), paths.clone(), Arc::new(SystemClock)),
            questions: questions.clone(),
            bridge: client.clone(),
            settings: Mutex::new(settings),
            undo: Mutex::new(UndoRecorder::default()),
            paths,
            db,
            locations,
            engine,
            trash,
        };
        let core = Arc::new(core);
        if let Some(client) = client {
            spawn_bridge_sync(&core, client);
        }
        Ok(core)
    }

    pub fn settings(&self) -> Settings {
        lock(&self.settings).clone()
    }

    /// Applies settings stored by the UI (dconf, SPEC §18).
    pub fn apply_settings(&self, settings: Settings) {
        let settings = settings.sanitised();
        self.recents.set_enabled(settings.recents_enabled);
        *lock(&self.settings) = settings;
    }

    /// The view settings every folder uses (BRW-4).
    pub fn view_prefs(&self) -> ViewPrefs {
        view_defaults(&self.settings())
    }

    pub fn provider(&self, location: &str) -> Result<Arc<dyn Provider>> {
        self.locations.provider(location)
    }

    pub fn location(&self, id: &str) -> Option<Location> {
        self.locations.get(id)
    }

    /// Lists a folder: the cached listing first through `cached` (BRW-5),
    /// then the fresh one, which also refreshes the cache.
    pub async fn list(&self, dir: &Uri, cached: impl FnOnce(Vec<Entry>)) -> Result<Vec<Entry>> {
        if let Ok(Some(hit)) = self.dircache.get(dir) {
            cached(hit.entries);
        }
        let provider = self.provider(&dir.location)?;
        let fresh = list_all(provider.as_ref(), &dir.path, Lane::Interactive).await?;
        let _ = self.dircache.put(dir, &fresh);
        Ok(fresh)
    }

    pub async fn stat(&self, uri: &Uri) -> Result<Entry> {
        self.provider(&uri.location)?
            .stat(&uri.path, true, Lane::Interactive)
            .await
    }

    /// New folder (exclusive create, §10 table).
    pub async fn make_folder(&self, parent: &Uri, name: &[u8]) -> Result<Uri> {
        let uri = parent.join(name)?;
        self.provider(&uri.location)?.make_dir(&uri.path, true).await?;
        self.invalidate(parent);
        Ok(uri)
    }

    /// New empty file.
    pub async fn make_file(&self, parent: &Uri, name: &[u8]) -> Result<Uri> {
        let uri = parent.join(name)?;
        self.provider(&uri.location)?.make_file(&uri.path).await?;
        self.invalidate(parent);
        Ok(uri)
    }

    /// Renames without replacing (§10 table); favourites follow and the
    /// rename can be undone for 10 s (OPS-9).
    pub async fn rename(&self, uri: &Uri, new_name: &[u8]) -> Result<Uri> {
        let parent = uri
            .parent()
            .ok_or_else(|| Error::new(ErrorKind::InvalidArgument, "cannot rename a location"))?;
        let to = parent.join(new_name)?;
        if &to == uri {
            return Ok(to);
        }
        self.provider(&uri.location)?
            .rename(&uri.path, &to.path, RenameMode::NoReplace)
            .await?;
        self.after_move(uri, &to);
        lock(&self.undo).record(UndoAction::Rename {
            from: uri.clone(),
            to: to.clone(),
        });
        self.invalidate(&parent);
        Ok(to)
    }

    fn after_move(&self, from: &Uri, to: &Uri) {
        let _ = self.favourites.on_moved(from, to);
    }

    /// Deletes items after the remorse timer (OPS-8): local items on the home
    /// file system go to Recently deleted (undoable, OPS-9); the rest are
    /// deleted permanently through the transfer engine.
    pub async fn delete(&self, items: &[Uri]) -> Result<Option<TransferId>> {
        let use_trash = self.settings().recently_deleted;
        let mut trashed = Vec::new();
        let mut permanent = Vec::new();
        for uri in items {
            let id = if use_trash {
                trash_one(&self.locations, &self.trash, uri).await?
            } else {
                None
            };
            match id {
                Some(id) => trashed.push(id),
                None => permanent.push(uri.clone()),
            }
            if let Some(parent) = uri.parent() {
                self.invalidate(&parent);
            }
        }
        if !trashed.is_empty() {
            lock(&self.undo).record(UndoAction::Trash { ids: trashed });
        }
        if permanent.is_empty() {
            return Ok(None);
        }
        let dest = permanent[0].parent().unwrap_or_else(|| permanent[0].clone());
        let plan = self.plan(OperationKind::Delete, permanent, dest).await?;
        let title = plan_title(&plan);
        self.engine
            .add(plan, &title, TransferOptions::default())
            .await
            .map(Some)
    }

    /// Plans an operation (OPS-1).
    pub async fn plan(&self, kind: OperationKind, sources: Vec<Uri>, dest: Uri) -> Result<Plan> {
        let settings = self.settings();
        let dest_is_local = self.locations.to_local_path(&dest).is_some();
        let opts = PlanOptions {
            threshold_items: settings.large_op_items,
            threshold_bytes: settings.large_op_bytes,
            dest_is_local,
            ..PlanOptions::default()
        };
        let cancel = AtomicBool::new(false);
        Planner::plan(kind, sources, dest, &self.locations, &opts, &cancel, &|_| {}).await
    }

    /// Copy or move: plans, then queues small plans at once; large ones need
    /// the summary sheet first (OPS-1).
    pub async fn copy_or_move(&self, kind: OperationKind, sources: Vec<Uri>, dest: Uri) -> Result<Started> {
        let plan = self.plan(kind, sources, dest).await?;
        if plan.needs_summary {
            return Ok(Started::NeedsSummary(Box::new(plan)));
        }
        self.start_plan(plan).await.map(Started::Transfer)
    }

    /// Queues a planned operation. Same-location moves record undo (OPS-9).
    pub async fn start_plan(&self, plan: Plan) -> Result<TransferId> {
        let settings = self.settings();
        if plan.kind == OperationKind::Move {
            let pairs: Vec<(Uri, Uri)> = plan
                .items
                .iter()
                .filter(|i| i.src.location == i.dst.location && i.src.parent() != i.dst.parent())
                .map(|i| (i.src.clone(), i.dst.clone()))
                .collect();
            for (from, to) in &pairs {
                self.after_move(from, to);
            }
            if !pairs.is_empty() && pairs.len() == plan.items.len() {
                lock(&self.undo).record(UndoAction::Move { pairs });
            }
        }
        let options = TransferOptions {
            verify_checksums: settings.verify_checksums,
            preserve_mtime: settings.preserve_mtimes,
            preserve_mode: settings.preserve_permissions,
            trash: settings.recently_deleted,
            ..TransferOptions::default()
        };
        let title = plan_title(&plan);
        self.invalidate(&plan.destination);
        self.engine.add(plan, &title, options).await
    }

    /// Whether an undo is still offered (OPS-9, 10 s banner).
    pub fn can_undo(&self) -> bool {
        lock(&self.undo).is_fresh_now()
    }

    /// The action the undo banner offers, while it is fresh.
    pub fn undo_kind(&self) -> Option<UndoAction> {
        let undo = lock(&self.undo);
        if undo.is_fresh_now() {
            undo.peek().cloned()
        } else {
            None
        }
    }

    /// Undoes the last rename, same-location move or trash operation.
    pub async fn undo(&self) -> Result<bool> {
        let Some(action) = lock(&self.undo).take() else {
            return Ok(false);
        };
        for step in crate::undo::undo_steps(&action) {
            match step {
                UndoStep::Rename { from, to, .. } => {
                    self.provider(&from.location)?
                        .rename(&from.path, &to.path, RenameMode::NoReplace)
                        .await?;
                    self.after_move(&from, &to);
                    if let Some(p) = to.parent() {
                        self.invalidate(&p);
                    }
                }
                UndoStep::Restore { id } => {
                    self.restore(id).await?;
                }
            }
        }
        Ok(true)
    }

    /// Restores an item from Recently deleted to where it was.
    pub async fn restore(&self, id: i64) -> Result<Uri> {
        let trash = self.trash.clone();
        let locations = self.locations.clone();
        let item = tokio::task::spawn_blocking(move || {
            let item = trash.get(id)?;
            trash.restore(id, &|uri| locations.to_local_path(uri))?;
            Ok::<_, Error>(item)
        })
        .await
        .map_err(|e| Error::new(ErrorKind::Internal, e.to_string()))??;
        let uri = item.original_uri;
        if let Some(p) = uri.parent() {
            self.invalidate(&p);
        }
        Ok(uri)
    }

    /// Opens an archive as a read-only location (LOC-5) and returns its id.
    pub async fn open_archive(&self, uri: &Uri) -> Result<String> {
        let id = archive_location_id(uri);
        if self.locations.get(&id).is_some() {
            return Ok(id);
        }
        let source = self.provider(&uri.location)?;
        let cache = self.paths.cache_dir().join("archives");
        tokio::fs::create_dir_all(&cache).await?;
        let provider = ArchiveProvider::open(source, uri.path.clone(), cache).await?;
        let name = uri
            .path
            .name()
            .map(crate::vpath::display_name)
            .unwrap_or_default();
        let location = Location::remote(id.clone(), LocationKind::Archive, name, Some(uri.to_string()));
        self.locations.register(location, Arc::new(provider));
        Ok(id)
    }

    /// Drops the cached listing of a folder (after a change made by the app).
    pub fn invalidate(&self, dir: &Uri) {
        let _ = self.dircache.invalidate(dir);
    }

    /// Sets a location's status shown in Browse (bridge reconnects, NVB-12).
    pub fn set_location_status(&self, id: &str, status: LocationStatus, attention: Option<String>) {
        self.locations.set_status(id, status, attention);
    }
}

/// Keeps the registry's remote locations and the engine's bridge state in
/// step with the bridge client. Holds only a weak reference to the core.
fn spawn_bridge_sync(core: &Arc<Core>, client: BridgeClient) {
    let weak = Arc::downgrade(core);
    let mut locations = client.watch_locations();
    let mut status = client.watch_status();
    tokio::spawn(async move {
        loop {
            let Some(core) = weak.upgrade() else { return };
            let current = *status.borrow_and_update();
            let ready = current == BridgeStatus::Ready;
            core.engine.bridge_available(ready);
            let remote = locations.borrow_and_update().clone();
            core.locations
                .set_remote(remote_entries(&client, &remote, current));
            drop(core);
            tokio::select! {
                changed = locations.changed() => if changed.is_err() { return },
                changed = status.changed() => if changed.is_err() { return },
            }
        }
    });
}

/// Registry entries for the bridge's locations: hidden unless the bridge
/// is usable (UI-8), shown as connecting while it reconnects (NVB-12).
fn remote_entries(
    client: &BridgeClient,
    remote: &[RemoteLocation],
    status: BridgeStatus,
) -> Vec<(Location, Arc<dyn Provider>)> {
    let shown = matches!(status, BridgeStatus::Ready | BridgeStatus::Reconnecting);
    if !shown {
        return Vec::new();
    }
    remote
        .iter()
        .map(|r| {
            let kind = match r.kind {
                RemoteKind::Account => LocationKind::Server {
                    provider: r.provider.clone(),
                },
                RemoteKind::AdHoc => LocationKind::AdHoc,
            };
            let mut location = Location::remote(r.id.clone(), kind, r.name.clone(), r.url.clone());
            let (status, attention) = match (status, r.attention) {
                (BridgeStatus::Reconnecting, _) => (LocationStatus::Connecting, None),
                (_, Attention::AuthFailed) => {
                    (LocationStatus::NeedsAttention, Some("auth-failed".to_owned()))
                }
                (_, Attention::ServerIdentityChanged) => (
                    LocationStatus::NeedsAttention,
                    Some("server-identity-changed".to_owned()),
                ),
                _ => (LocationStatus::Ready, None),
            };
            location.status = status;
            location.attention = attention;
            let provider: Arc<dyn Provider> = Arc::new(client.provider(&r.id));
            (location, provider)
        })
        .collect()
}

fn view_defaults(s: &Settings) -> ViewPrefs {
    let base = ViewPrefs::default();
    ViewPrefs {
        sort: SortKey::parse(&s.sort_key).unwrap_or(base.sort),
        descending: s.sort_descending,
        folders_first: s.folders_first,
        show_hidden: s.show_hidden,
        view_mode: ViewMode::parse(&s.view_mode).unwrap_or(base.view_mode),
        thumbnails: s.thumbnails_local,
    }
}

/// A short engineering title for the transfer list (the first item's
/// name); the UI renders translated text from the kind and counts.
fn plan_title(plan: &Plan) -> String {
    plan.items
        .first()
        .and_then(|i| i.src.name().map(crate::vpath::display_name))
        .unwrap_or_default()
}
