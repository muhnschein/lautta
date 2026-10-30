// SPDX-License-Identifier: LGPL-2.1-or-later
//! App context: `Core` wires stores, location registry and transfer engine. Qt-free.

use crate::bridge::{Attention, BridgeClient, BridgeConfig, BridgeStatus, RemoteKind, RemoteLocation};
use crate::browse_layout::BrowseLayout;
use crate::db::Db;
use crate::dircache::DirCache;
use crate::entry::Entry;
use crate::error::{Error, ErrorKind, Result};
use crate::locations::{ConnectionState, Location, LocationKind, LocationRegistry, LocationStatus};
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
use crate::uri::Uri;
use crate::viewprefs::{ViewMode, ViewPrefs};
use crate::volumes::VolumeWatcher;
use crate::workcopy::WorkingCopies;
use async_trait::async_trait;
use std::sync::atomic::AtomicBool;
use std::sync::{Arc, Mutex, MutexGuard};
use tokio::sync::watch;

const DIRCACHE_ENTRIES: usize = 64;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Started {
    Transfer(TransferId),
    NeedsSummary(Box<Plan>),
}

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
    pub browse_layout: BrowseLayout,
    pub working_copies: WorkingCopies,
    pub questions: Arc<Questions>,
    /// `None` only in tests.
    pub bridge: Option<BridgeClient>,
    settings: Mutex<Settings>,
    settings_changed: watch::Sender<u64>,
    /// Follows removable-media root; `None` without inotify, Browse then rescans on refresh.
    _volume_watch: Option<VolumeWatcher>,
}

fn lock<T>(m: &Mutex<T>) -> MutexGuard<'_, T> {
    match m.lock() {
        Ok(g) => g,
        Err(p) => p.into_inner(),
    }
}

struct LocalTrasher {
    locations: LocationRegistry,
    trash: Trash,
}

#[async_trait]
impl Trasher for LocalTrasher {
    async fn trash(&self, uri: &Uri) -> Result<bool> {
        if !self.locations.keeps_trash(&uri.location) {
            return Ok(false);
        }
        trash_one(&self.locations, &self.trash, uri).await.map(|_| true)
    }
}

async fn trash_one(locations: &LocationRegistry, trash: &Trash, uri: &Uri) -> Result<()> {
    // Unknown or vanished location is an error, not a permanent delete.
    locations.provider(&uri.location)?;
    let path = locations
        .to_local_path(uri)
        .ok_or_else(|| Error::new(ErrorKind::Unsupported, "not a local item"))?;
    let trash = trash.clone();
    let uri = uri.clone();
    tokio::task::spawn_blocking(move || trash.trash(&path, &uri))
        .await
        .map_err(|e| Error::new(ErrorKind::Internal, e.to_string()))?
        .map(|_| ())
}

impl Core {
    pub async fn open(paths: AppPaths) -> Result<Arc<Core>> {
        let bridge = BridgeConfig::for_app(&paths);
        Core::open_full(paths.clone(), LocationRegistry::new(paths), Some(bridge)).await
    }

    pub async fn open_with(paths: AppPaths, locations: LocationRegistry) -> Result<Arc<Core>> {
        Core::open_full(paths, locations, None).await
    }

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
        let volume_watch = watch_volumes(&locations);
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
            browse_layout: BrowseLayout::new(db.clone()),
            working_copies: WorkingCopies::new(db.clone(), paths.clone(), Arc::new(SystemClock)),
            questions: questions.clone(),
            bridge: client.clone(),
            settings: Mutex::new(settings),
            settings_changed: watch::channel(0).0,
            _volume_watch: volume_watch,
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

    pub fn apply_settings(&self, settings: Settings) {
        let settings = settings.sanitised();
        self.recents.set_enabled(settings.recents_enabled);
        let previous = std::mem::replace(&mut *lock(&self.settings), settings.clone());
        if previous.thumbnail_cache_mb != settings.thumbnail_cache_mb {
            self.follow_thumbnail_budget(&settings);
        }
        if previous != settings {
            self.settings_changed.send_modify(|generation| *generation += 1);
        }
    }

    pub fn watch_settings(&self) -> watch::Receiver<u64> {
        self.settings_changed.subscribe()
    }

    pub fn view_prefs(&self) -> ViewPrefs {
        view_defaults(&self.settings())
    }

    pub fn provider(&self, location: &str) -> Result<Arc<dyn Provider>> {
        self.locations.provider(location)
    }

    pub fn location(&self, id: &str) -> Option<Location> {
        self.locations.get(id)
    }

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

    pub async fn make_folder(&self, parent: &Uri, name: &[u8]) -> Result<Uri> {
        let uri = parent.join(name)?;
        self.provider(&uri.location)?.make_dir(&uri.path, true).await?;
        self.invalidate(parent);
        Ok(uri)
    }

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
        self.invalidate(&parent);
        Ok(to)
    }

    fn after_move(&self, from: &Uri, to: &Uri) {
        let _ = self.favourites.on_moved(from, to);
    }

    /// The one place that decides; [`Core::delete`] follows it.
    pub fn deletes_to_trash(&self, uri: &Uri) -> bool {
        self.settings().recently_deleted && self.locations.keeps_trash(&uri.location)
    }

    pub async fn delete(&self, items: &[Uri]) -> Result<Option<TransferId>> {
        let mut permanent = Vec::new();
        let mut failed = None;
        for uri in items {
            if !self.deletes_to_trash(uri) {
                permanent.push(uri.clone());
                continue;
            }
            if let Err(e) = trash_one(&self.locations, &self.trash, uri).await {
                failed = Some(e);
                break;
            }
        }
        for parent in items.iter().filter_map(Uri::parent) {
            self.invalidate(&parent);
        }
        if let Some(e) = failed {
            return Err(e);
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

    pub async fn copy_or_move(&self, kind: OperationKind, sources: Vec<Uri>, dest: Uri) -> Result<Started> {
        let plan = self.plan(kind, sources, dest).await?;
        if plan.needs_summary {
            return Ok(Started::NeedsSummary(Box::new(plan)));
        }
        self.start_plan(plan).await.map(Started::Transfer)
    }

    pub async fn start_plan(&self, plan: Plan) -> Result<TransferId> {
        let settings = self.settings();
        if plan.kind == OperationKind::Move {
            for item in &plan.items {
                if item.src.location == item.dst.location && item.src.parent() != item.dst.parent() {
                    self.after_move(&item.src, &item.dst);
                }
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

    pub fn invalidate(&self, dir: &Uri) {
        let _ = self.dircache.invalidate(dir);
    }

    pub fn set_location_status(&self, id: &str, status: LocationStatus, attention: Option<String>) {
        self.locations.set_status(id, status, attention);
    }
}

fn watch_volumes(locations: &LocationRegistry) -> Option<VolumeWatcher> {
    match locations.watch_volumes() {
        Ok(watcher) => Some(watcher),
        Err(e) => {
            log::warn!("removable volumes are not followed live: {e}");
            None
        }
    }
}

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
            // Last list's states are stale while bridge is away.
            location.connection = match status {
                BridgeStatus::Reconnecting => ConnectionState::Connecting,
                _ => r.connection.clone(),
            };
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

fn plan_title(plan: &Plan) -> String {
    plan.items
        .first()
        .and_then(|i| i.src.name().map(crate::vpath::display_name))
        .unwrap_or_default()
}
