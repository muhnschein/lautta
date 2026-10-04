// SPDX-License-Identifier: LGPL-2.1-or-later
//! The location registry (SPEC §5.3, §8): everything the user can browse,
//! with the provider behind each location.
//!
//! * user folders (LOC-1) and Android storage (LOC-2) are found by looking
//!   at the home folder; each is shown only if it exists and is readable;
//! * removable volumes (LOC-3) come from [`crate::volumes`];
//! * netvfs accounts and ad-hoc servers (LOC-4) are pushed in with
//!   [`LocationRegistry::set_remote`];
//! * archives (LOC-5) are added and removed with `register`/`unregister`.
//!
//! The registry is cheap to clone (shared state) and implements
//! [`ProviderResolver`]. Interested parties follow changes through
//! [`LocationRegistry::subscribe`].

use crate::entry::cap;
use crate::error::{Error, ErrorKind, Result};
use crate::paths::AppPaths;
use crate::provider::local::{lexical_normalize, new_io_semaphore, LocalProvider};
use crate::provider::{Provider, ProviderResolver};
use crate::sys;
use crate::uri::{LocationId, Uri};
use crate::volumes::{self, Volume, VolumeEvent, VolumeWatcher};
use crate::vpath::VPath;
use percent_encoding::{percent_encode, AsciiSet, CONTROLS};
use std::ffi::OsStr;
use std::os::unix::ffi::OsStrExt;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex, MutexGuard};
use tokio::sync::{watch, Semaphore};

/// `(folder name, location id)` of the user folders (LOC-1), in display order.
pub const USER_FOLDERS: [(&str, &str); 7] = [
    ("Documents", "user-documents"),
    ("Downloads", "user-downloads"),
    ("Pictures", "user-pictures"),
    ("Music", "user-music"),
    ("Videos", "user-videos"),
    ("Public", "user-public"),
    ("Playlists", "user-playlists"),
];

/// The folders under `~/android_storage` that the sandbox exposes (LOC-2).
pub const ANDROID_FOLDERS: [(&str, &str); 7] = [
    ("Documents", "android-documents"),
    ("Download", "android-download"),
    ("Pictures", "android-pictures"),
    ("DCIM", "android-dcim"),
    ("Music", "android-music"),
    ("Podcasts", "android-podcasts"),
    ("Movies", "android-movies"),
];

const ANDROID_DIR: &str = "android_storage";

/// Bytes escaped when a path is appended to a displayed address.
const ADDRESS_SET: &AsciiSet = &CONTROLS
    .add(b' ')
    .add(b'"')
    .add(b'#')
    .add(b'%')
    .add(b'<')
    .add(b'>')
    .add(b'?')
    .add(b'`')
    .add(b'{')
    .add(b'}');

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum LocationKind {
    UserFolder,
    Android,
    Volume,
    /// A netvfs account; `provider` is the netvfs provider name (sftp, smb, ...).
    Server {
        provider: String,
    },
    AdHoc,
    Archive,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum LocationStatus {
    #[default]
    Ready,
    Connecting,
    Offline,
    NeedsAttention,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Location {
    pub id: LocationId,
    pub kind: LocationKind,
    pub name: String,
    /// Absolute folder for local locations.
    pub local_root: Option<PathBuf>,
    /// Address shown to the user for remote locations (LOC-7). May contain a
    /// user name but never a password; [`LocationRegistry::display_address`]
    /// strips anything secret again.
    pub url: Option<String>,
    /// Why the location needs the user (for example "Sign in again").
    pub attention: Option<String>,
    pub status: LocationStatus,
}

impl Location {
    pub fn local(
        id: impl Into<LocationId>,
        kind: LocationKind,
        name: impl Into<String>,
        root: PathBuf,
    ) -> Location {
        Location {
            id: id.into(),
            kind,
            name: name.into(),
            local_root: Some(root),
            url: None,
            attention: None,
            status: LocationStatus::Ready,
        }
    }

    pub fn remote(
        id: impl Into<LocationId>,
        kind: LocationKind,
        name: impl Into<String>,
        url: Option<String>,
    ) -> Location {
        Location {
            id: id.into(),
            kind,
            name: name.into(),
            local_root: None,
            url,
            attention: None,
            status: LocationStatus::Ready,
        }
    }

    pub fn is_local(&self) -> bool {
        self.local_root.is_some()
    }
}

#[derive(Clone)]
struct Slot {
    location: Location,
    provider: Arc<dyn Provider>,
}

#[derive(Default)]
struct State {
    user: Vec<Slot>,
    android: Vec<Slot>,
    volumes: Vec<Slot>,
    remote: Vec<Slot>,
    extra: Vec<Slot>,
}

impl State {
    fn all(&self) -> impl Iterator<Item = &Slot> {
        self.user
            .iter()
            .chain(&self.android)
            .chain(&self.volumes)
            .chain(&self.remote)
            .chain(&self.extra)
    }

    fn find_mut(&mut self, id: &str) -> Option<&mut Slot> {
        self.user
            .iter_mut()
            .chain(self.android.iter_mut())
            .chain(self.volumes.iter_mut())
            .chain(self.remote.iter_mut())
            .chain(self.extra.iter_mut())
            .find(|s| s.location.id == id)
    }
}

struct Inner {
    paths: AppPaths,
    media_root: PathBuf,
    io: Arc<Semaphore>,
    state: Mutex<State>,
    changed: watch::Sender<u64>,
}

#[derive(Clone)]
pub struct LocationRegistry {
    inner: Arc<Inner>,
}

impl LocationRegistry {
    /// Scans the user folders, Android storage and `/run/media/$USER` once.
    pub fn new(paths: AppPaths) -> LocationRegistry {
        let media_root = volumes::media_root(&paths);
        LocationRegistry::with_media_root(paths, media_root)
    }

    /// Like [`LocationRegistry::new`] with an explicit removable-media root (tests).
    pub fn with_media_root(paths: AppPaths, media_root: PathBuf) -> LocationRegistry {
        let (changed, _) = watch::channel(0);
        let reg = LocationRegistry {
            inner: Arc::new(Inner {
                paths,
                media_root,
                io: new_io_semaphore(),
                state: Mutex::new(State::default()),
                changed,
            }),
        };
        reg.refresh();
        reg
    }

    fn state(&self) -> MutexGuard<'_, State> {
        match self.inner.state.lock() {
            Ok(g) => g,
            Err(p) => p.into_inner(),
        }
    }

    fn notify(&self) {
        self.inner.changed.send_modify(|generation| *generation += 1);
    }

    /// A receiver whose value increases on every change of the location
    /// list, statuses included.
    pub fn subscribe(&self) -> watch::Receiver<u64> {
        self.inner.changed.subscribe()
    }

    /// Rescans the local folders and volumes. True when something changed.
    pub fn refresh(&self) -> bool {
        let user = self.scan_folders(&self.inner.paths.home, &USER_FOLDERS, LocationKind::UserFolder);
        let android_root = self.inner.paths.home.join(ANDROID_DIR);
        let android = self.scan_folders(&android_root, &ANDROID_FOLDERS, LocationKind::Android);
        let vols = volumes::scan(&self.inner.media_root);
        let mut changed = self.replace_slots(|s| &mut s.user, user);
        changed |= self.replace_slots(|s| &mut s.android, android);
        changed |= self.apply_volumes_inner(vols);
        if changed {
            self.notify();
        }
        changed
    }

    /// The known folders below `base` that exist and can be listed.
    fn scan_folders(&self, base: &Path, table: &[(&str, &str)], kind: LocationKind) -> Vec<Location> {
        table
            .iter()
            .filter_map(|(dir, id)| {
                let root = base.join(dir);
                (std::fs::metadata(&root).map(|m| m.is_dir()).unwrap_or(false)
                    && std::fs::read_dir(&root).is_ok())
                .then(|| Location::local(*id, kind.clone(), *dir, root))
            })
            .collect()
    }

    /// Provider for a local folder: sharing the I/O semaphore, and with
    /// *Recently deleted* when the folder is on the home filesystem (OPS-8).
    fn local_provider(&self, root: &Path, trash_possible: bool) -> Arc<dyn Provider> {
        let mut provider = LocalProvider::new(root.to_path_buf()).with_semaphore(self.inner.io.clone());
        if trash_possible && self.on_home_filesystem(root) {
            provider = provider.with_capability(cap::TRASH);
        }
        Arc::new(provider)
    }

    fn on_home_filesystem(&self, root: &Path) -> bool {
        sys::same_device(root, &self.inner.paths.home).unwrap_or(false)
    }

    /// Swaps in the new locations for one group, keeping unchanged slots
    /// (same id, name and root) with their provider and status. True when
    /// the list differs.
    fn replace_slots(&self, group: fn(&mut State) -> &mut Vec<Slot>, found: Vec<Location>) -> bool {
        let trash_possible = !matches!(found.first().map(|l| &l.kind), Some(LocationKind::Volume));
        let old: Vec<Slot> = group(&mut self.state()).clone();
        let same =
            |a: &Location, b: &Location| a.id == b.id && a.name == b.name && a.local_root == b.local_root;
        let new: Vec<Slot> = found
            .into_iter()
            .filter_map(|loc| {
                if let Some(slot) = old.iter().find(|s| same(&s.location, &loc)) {
                    return Some(slot.clone());
                }
                let provider = self.local_provider(loc.local_root.as_deref()?, trash_possible);
                Some(Slot {
                    location: loc,
                    provider,
                })
            })
            .collect();
        let changed =
            old.len() != new.len() || old.iter().zip(&new).any(|(o, n)| !same(&o.location, &n.location));
        *group(&mut self.state()) = new;
        changed
    }

    fn apply_volumes_inner(&self, vols: Vec<Volume>) -> bool {
        let locations = vols
            .into_iter()
            .map(|v| Location::local(v.location_id(), LocationKind::Volume, v.name, v.mount_path))
            .collect();
        self.replace_slots(|s| &mut s.volumes, locations)
    }

    /// Re-reads `/run/media/$USER`.
    pub fn refresh_volumes(&self) -> bool {
        let changed = self.apply_volumes_inner(volumes::scan(&self.inner.media_root));
        if changed {
            self.notify();
        }
        changed
    }

    /// Applies one live volume change.
    pub fn apply_volume_event(&self, event: &VolumeEvent) -> bool {
        let mut current: Vec<Volume> = {
            let state = self.state();
            state
                .volumes
                .iter()
                .filter_map(|s| {
                    let root = s.location.local_root.clone()?;
                    Some(Volume {
                        id: s
                            .location
                            .id
                            .strip_prefix("vol-")
                            .unwrap_or(&s.location.id)
                            .to_owned(),
                        name: s.location.name.clone(),
                        mount_path: root,
                    })
                })
                .collect()
        };
        match event {
            VolumeEvent::Appeared(v) => {
                current.retain(|c| c.id != v.id);
                current.push(v.clone());
            }
            VolumeEvent::Disappeared(v) => current.retain(|c| c.id != v.id),
        }
        let changed = self.apply_volumes_inner(current);
        if changed {
            self.notify();
        }
        changed
    }

    /// Starts following the media root; changes are applied from a task on
    /// the current tokio runtime. Dropping the returned watcher stops it.
    pub fn watch_volumes(&self) -> Result<VolumeWatcher> {
        let runtime = tokio::runtime::Handle::try_current()
            .map_err(|_| Error::new(ErrorKind::Internal, "volume watching needs a tokio runtime"))?;
        let (watcher, mut rx) =
            VolumeWatcher::start(self.inner.media_root.clone(), volumes::VOLUME_DEBOUNCE)?;
        // Volumes mounted between the constructor and now.
        self.refresh_volumes();
        let registry = self.clone();
        runtime.spawn(async move {
            while let Some(event) = rx.recv().await {
                registry.apply_volume_event(&event);
            }
        });
        Ok(watcher)
    }

    /// Replaces all remote locations (netvfs accounts, ad-hoc servers).
    pub fn set_remote(&self, remote: Vec<(Location, Arc<dyn Provider>)>) {
        let slots = remote
            .into_iter()
            .map(|(location, provider)| Slot { location, provider })
            .collect();
        self.state().remote = slots;
        self.notify();
    }

    /// Adds or replaces one location (archives, LOC-5).
    pub fn register(&self, location: Location, provider: Arc<dyn Provider>) {
        {
            let mut state = self.state();
            state.extra.retain(|s| s.location.id != location.id);
            state.extra.push(Slot { location, provider });
        }
        self.notify();
    }

    /// Removes a location added with [`LocationRegistry::register`].
    pub fn unregister(&self, id: &str) -> bool {
        let removed = {
            let mut state = self.state();
            let before = state.extra.len();
            state.extra.retain(|s| s.location.id != id);
            state.extra.len() != before
        };
        if removed {
            self.notify();
        }
        removed
    }

    /// Sets the status and the attention text of any location (reconnecting,
    /// "Sign in again", ...). False when the id is unknown or nothing changed.
    pub fn set_status(&self, id: &str, status: LocationStatus, attention: Option<String>) -> bool {
        let changed = {
            let mut state = self.state();
            match state.find_mut(id) {
                Some(slot) if slot.location.status != status || slot.location.attention != attention => {
                    slot.location.status = status;
                    slot.location.attention = attention;
                    true
                }
                _ => false,
            }
        };
        if changed {
            self.notify();
        }
        changed
    }

    /// All locations in display order: user folders, Android, volumes,
    /// remote, then registered ones.
    pub fn locations(&self) -> Vec<Location> {
        self.state().all().map(|s| s.location.clone()).collect()
    }

    pub fn get(&self, id: &str) -> Option<Location> {
        self.state()
            .all()
            .find(|s| s.location.id == id)
            .map(|s| s.location.clone())
    }

    /// The address to show or copy (LOC-7): `file://` and the absolute path
    /// for local locations, the netvfs URL plus the path for remote ones,
    /// without credentials, query or fragment.
    pub fn display_address(&self, uri: &Uri) -> String {
        let Some(loc) = self.get(&uri.location) else {
            return uri.to_string();
        };
        if let Some(full) = local_path_of(&loc, uri) {
            return format!(
                "file://{}",
                percent_encode(full.as_os_str().as_bytes(), ADDRESS_SET)
            );
        }
        match loc.url.as_deref().map(strip_secrets) {
            Some(base) if uri.path.is_root() => base,
            Some(base) => format!(
                "{}/{}",
                base.trim_end_matches('/'),
                percent_encode(uri.path.as_bytes(), ADDRESS_SET)
            ),
            None => uri.to_string(),
        }
    }

    /// The local path behind `uri`, `None` for remote locations and archives.
    pub fn to_local_path(&self, uri: &Uri) -> Option<PathBuf> {
        local_path_of(&self.get(&uri.location)?, uri)
    }

    /// The URI of an absolute local path: the location with the longest
    /// matching root. `.` and `..` are resolved lexically first.
    pub fn uri_for_local_path(&self, path: &Path) -> Option<Uri> {
        if !path.is_absolute() {
            return None;
        }
        let path = lexical_normalize(path);
        let best = {
            let state = self.state();
            state
                .all()
                .filter_map(|s| {
                    let root = lexical_normalize(s.location.local_root.as_deref()?);
                    path.starts_with(&root)
                        .then(|| (root.components().count(), s.location.id.clone(), root))
                })
                .max_by_key(|(depth, _, _)| *depth)
        };
        let (_, id, root) = best?;
        let rel = path.strip_prefix(&root).ok()?;
        Some(Uri::new(id, VPath::parse(rel.as_os_str().as_bytes()).ok()?))
    }
}

/// The absolute path of `uri` inside a local location.
fn local_path_of(loc: &Location, uri: &Uri) -> Option<PathBuf> {
    let root = loc.local_root.as_ref()?;
    Some(if uri.path.is_root() {
        root.clone()
    } else {
        root.join(OsStr::from_bytes(uri.path.as_bytes()))
    })
}

impl ProviderResolver for LocationRegistry {
    fn provider(&self, location: &str) -> Result<Arc<dyn Provider>> {
        self.state()
            .all()
            .find(|s| s.location.id == location)
            .map(|s| s.provider.clone())
            .ok_or_else(|| Error::new(ErrorKind::NotFound, format!("unknown location {location}")))
    }
}

/// Removes user name passwords, query and fragment from a URL.
fn strip_secrets(url: &str) -> String {
    let cut = url.find(['?', '#']).unwrap_or(url.len());
    let url = &url[..cut];
    let Some(scheme_end) = url.find("://") else {
        return url.to_owned();
    };
    let (scheme, rest) = url.split_at(scheme_end + 3);
    let auth_end = rest.find('/').unwrap_or(rest.len());
    let (authority, path) = rest.split_at(auth_end);
    match authority.rfind('@') {
        Some(at) => {
            let user = authority[..at].split(':').next().unwrap_or("");
            format!("{scheme}{user}@{}{path}", &authority[at + 1..])
        }
        None => url.to_owned(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::provider::memory::MemoryProvider;
    use crate::provider::{list_all, Lane};
    use std::time::Duration;

    struct Env {
        _dir: tempfile::TempDir,
        home: PathBuf,
        media: PathBuf,
        reg: LocationRegistry,
    }

    fn env_with(dirs: &[&str]) -> Env {
        let dir = tempfile::tempdir().unwrap();
        let home = dir.path().join("home");
        let media = dir.path().join("media");
        std::fs::create_dir_all(&media).unwrap();
        for d in dirs {
            std::fs::create_dir_all(home.join(d)).unwrap();
        }
        std::fs::create_dir_all(&home).unwrap();
        let reg = LocationRegistry::with_media_root(AppPaths::new(&home), media.clone());
        Env {
            _dir: dir,
            home,
            media,
            reg,
        }
    }

    fn ids(reg: &LocationRegistry) -> Vec<String> {
        reg.locations().into_iter().map(|l| l.id).collect()
    }

    fn mem() -> Arc<dyn Provider> {
        Arc::new(MemoryProvider::default())
    }

    #[test]
    fn only_existing_user_folders_are_listed_in_order() {
        let e = env_with(&["Music", "Documents", "Pictures"]);
        std::fs::write(e.home.join("Public"), "a file, not a folder").unwrap();
        assert_eq!(ids(&e.reg), ["user-documents", "user-pictures", "user-music"]);
        let docs = e.reg.get("user-documents").unwrap();
        assert_eq!(docs.kind, LocationKind::UserFolder);
        assert_eq!(docs.name, "Documents");
        assert_eq!(docs.local_root, Some(e.home.join("Documents")));
        assert_eq!(docs.status, LocationStatus::Ready);
        assert!(e.reg.get("user-videos").is_none());
    }

    #[test]
    fn no_home_or_root_location_exists() {
        let e = env_with(&["Documents"]);
        assert!(e
            .reg
            .locations()
            .iter()
            .all(|l| l.local_root.as_deref() != Some(e.home.as_path())));
        assert!(e.reg.get("user-home").is_none());
    }

    #[test]
    fn unreadable_folders_are_hidden() {
        use std::os::unix::fs::PermissionsExt;
        if sys::effective_ids().0 == 0 {
            return; // root can read everything
        }
        let e = env_with(&["Documents", "Music"]);
        std::fs::set_permissions(e.home.join("Music"), std::fs::Permissions::from_mode(0o000)).unwrap();
        e.reg.refresh();
        std::fs::set_permissions(e.home.join("Music"), std::fs::Permissions::from_mode(0o755)).unwrap();
        assert_eq!(ids(&e.reg), ["user-documents"]);
    }

    #[test]
    fn android_storage_folders() {
        let e = env_with(&[
            "android_storage/DCIM",
            "android_storage/Download",
            "android_storage/Movies",
            "android_storage/Other",
        ]);
        assert_eq!(
            ids(&e.reg),
            ["android-download", "android-dcim", "android-movies"]
        );
        let dcim = e.reg.get("android-dcim").unwrap();
        assert_eq!(dcim.kind, LocationKind::Android);
        assert_eq!(dcim.local_root, Some(e.home.join("android_storage/DCIM")));
    }

    #[test]
    fn volumes_appear_with_stable_ids() {
        let e = env_with(&[]);
        std::fs::create_dir(e.media.join("SD CARD")).unwrap();
        std::fs::create_dir(e.media.join("0A1B-2C3D")).unwrap();
        assert!(e.reg.refresh());
        assert_eq!(ids(&e.reg), ["vol-0A1B-2C3D", "vol-SD_20CARD"]);
        let sd = e.reg.get("vol-SD_20CARD").unwrap();
        assert_eq!(sd.kind, LocationKind::Volume);
        assert_eq!(sd.name, "SD CARD");
        assert_eq!(sd.local_root, Some(e.media.join("SD CARD")));
        std::fs::remove_dir(e.media.join("0A1B-2C3D")).unwrap();
        assert!(e.reg.refresh_volumes());
        assert_eq!(ids(&e.reg), ["vol-SD_20CARD"]);
    }

    #[test]
    fn changes_are_signalled_only_when_something_changed() {
        let e = env_with(&["Documents"]);
        let mut rx = e.reg.subscribe();
        assert!(!rx.has_changed().unwrap());
        assert!(!e.reg.refresh());
        assert!(!rx.has_changed().unwrap(), "nothing changed, nothing signalled");
        std::fs::create_dir(e.home.join("Music")).unwrap();
        assert!(e.reg.refresh());
        assert!(rx.has_changed().unwrap());
        let seen = *rx.borrow_and_update();
        std::fs::remove_dir(e.home.join("Music")).unwrap();
        assert!(e.reg.refresh());
        assert!(*rx.borrow_and_update() > seen);
        assert_eq!(ids(&e.reg), ["user-documents"]);
    }

    #[test]
    fn volume_events_update_the_list_incrementally() {
        let e = env_with(&[]);
        let v = Volume {
            id: "CARD".into(),
            name: "CARD".into(),
            mount_path: e.media.join("CARD"),
        };
        std::fs::create_dir(&v.mount_path).unwrap();
        let rx = e.reg.subscribe();
        assert!(e.reg.apply_volume_event(&VolumeEvent::Appeared(v.clone())));
        assert_eq!(ids(&e.reg), ["vol-CARD"]);
        assert!(rx.has_changed().unwrap());
        assert!(
            !e.reg.apply_volume_event(&VolumeEvent::Appeared(v.clone())),
            "duplicate"
        );
        assert!(e.reg.apply_volume_event(&VolumeEvent::Disappeared(v.clone())));
        assert!(ids(&e.reg).is_empty());
        assert!(
            !e.reg.apply_volume_event(&VolumeEvent::Disappeared(v)),
            "already gone"
        );
    }

    #[tokio::test]
    async fn live_volume_watching_updates_the_registry() {
        let e = env_with(&[]);
        let mut rx = e.reg.subscribe();
        let _watcher = e.reg.watch_volumes().unwrap();
        std::fs::create_dir(e.media.join("USBSTICK")).unwrap();
        tokio::time::timeout(Duration::from_secs(5), rx.changed())
            .await
            .unwrap()
            .unwrap();
        assert_eq!(ids(&e.reg), ["vol-USBSTICK"]);
        std::fs::remove_dir(e.media.join("USBSTICK")).unwrap();
        tokio::time::timeout(Duration::from_secs(5), rx.changed())
            .await
            .unwrap()
            .unwrap();
        assert!(ids(&e.reg).is_empty());
    }

    #[test]
    fn watching_without_a_runtime_is_an_error() {
        let e = env_with(&[]);
        assert_eq!(e.reg.watch_volumes().err().unwrap().kind, ErrorKind::Internal);
    }

    #[tokio::test]
    async fn resolver_returns_working_local_providers() {
        let e = env_with(&["Documents"]);
        std::fs::write(e.home.join("Documents/note.txt"), "hi").unwrap();
        let p = e.reg.provider("user-documents").unwrap();
        let all = list_all(p.as_ref(), &VPath::root(), Lane::Interactive)
            .await
            .unwrap();
        assert_eq!(all.len(), 1);
        assert_eq!(all[0].name, b"note.txt");
        assert_eq!(e.reg.provider("nope").err().unwrap().kind, ErrorKind::NotFound);
    }

    #[test]
    fn providers_survive_a_refresh_unchanged() {
        let e = env_with(&["Documents"]);
        let before = e.reg.provider("user-documents").unwrap();
        std::fs::create_dir(e.home.join("Music")).unwrap();
        e.reg.refresh();
        let after = e.reg.provider("user-documents").unwrap();
        assert!(Arc::ptr_eq(&before, &after));
    }

    #[test]
    fn trash_is_offered_on_the_home_filesystem_only() {
        let e = env_with(&["Documents"]);
        std::fs::create_dir(e.media.join("CARD")).unwrap();
        e.reg.refresh();
        let docs = e.reg.provider("user-documents").unwrap().capabilities();
        assert!(docs.has(cap::TRASH));
        assert!(docs.has(cap::WRITE));
        let card = e.reg.provider("vol-CARD").unwrap().capabilities();
        assert!(
            !card.has(cap::TRASH),
            "removable media delete permanently (OPS-8)"
        );
    }

    #[test]
    fn remote_locations_are_replaced_as_a_set() {
        let e = env_with(&["Documents"]);
        let rx = e.reg.subscribe();
        let nas = Location::remote(
            "nv-1",
            LocationKind::Server {
                provider: "sftp".into(),
            },
            "NAS",
            Some("sftp://me@nas.local".into()),
        );
        let adhoc = Location::remote("nv-adhoc-2", LocationKind::AdHoc, "Quick", None);
        e.reg.set_remote(vec![(nas, mem()), (adhoc, mem())]);
        assert!(rx.has_changed().unwrap());
        assert_eq!(ids(&e.reg), ["user-documents", "nv-1", "nv-adhoc-2"]);
        assert!(e.reg.provider("nv-1").is_ok());
        assert!(!e.reg.get("nv-1").unwrap().is_local());
        e.reg.set_remote(Vec::new());
        assert_eq!(ids(&e.reg), ["user-documents"]);
        assert!(e.reg.provider("nv-1").is_err());
    }

    #[test]
    fn archives_register_and_unregister() {
        let e = env_with(&[]);
        let mut rx = e.reg.subscribe();
        let loc = Location::remote("arc-ab12", LocationKind::Archive, "photos.zip", None);
        e.reg.register(loc.clone(), mem());
        assert!(rx.has_changed().unwrap());
        assert_eq!(ids(&e.reg), ["arc-ab12"]);
        e.reg.register(
            Location {
                name: "renamed.zip".into(),
                ..loc
            },
            mem(),
        );
        assert_eq!(e.reg.locations().len(), 1, "same id replaces");
        assert_eq!(e.reg.get("arc-ab12").unwrap().name, "renamed.zip");
        rx.borrow_and_update();
        assert!(e.reg.unregister("arc-ab12"));
        assert!(rx.has_changed().unwrap());
        assert!(!e.reg.unregister("arc-ab12"));
        assert!(e.reg.provider("arc-ab12").is_err());
    }

    #[test]
    fn status_and_attention_updates() {
        let e = env_with(&[]);
        let nas = Location::remote(
            "nv-1",
            LocationKind::Server {
                provider: "smb".into(),
            },
            "NAS",
            None,
        );
        e.reg.set_remote(vec![(nas, mem())]);
        let mut rx = e.reg.subscribe();
        assert!(e.reg.set_status(
            "nv-1",
            LocationStatus::NeedsAttention,
            Some("Sign in again".into())
        ));
        assert!(rx.has_changed().unwrap());
        let l = e.reg.get("nv-1").unwrap();
        assert_eq!(l.status, LocationStatus::NeedsAttention);
        assert_eq!(l.attention.as_deref(), Some("Sign in again"));
        rx.borrow_and_update();
        assert!(!e.reg.set_status(
            "nv-1",
            LocationStatus::NeedsAttention,
            Some("Sign in again".into())
        ));
        assert!(!rx.has_changed().unwrap());
        assert!(!e.reg.set_status("missing", LocationStatus::Offline, None));
        assert!(e.reg.set_status("nv-1", LocationStatus::Ready, None));
        assert_eq!(e.reg.get("nv-1").unwrap().attention, None);
    }

    #[test]
    fn status_survives_a_local_refresh() {
        let e = env_with(&["Documents"]);
        assert!(e
            .reg
            .set_status("user-documents", LocationStatus::Offline, Some("why".into())));
        let mut rx = e.reg.subscribe();
        assert!(!e.reg.refresh(), "a status is not a folder change");
        assert!(!rx.has_changed().unwrap());
        std::fs::create_dir(e.home.join("Music")).unwrap();
        assert!(e.reg.refresh());
        let docs = e.reg.get("user-documents").unwrap();
        assert_eq!(docs.status, LocationStatus::Offline);
        assert_eq!(docs.attention.as_deref(), Some("why"));
        rx.borrow_and_update();
    }

    // ---- addresses and paths ----------------------------------------------

    #[test]
    fn local_addresses_are_file_urls() {
        let e = env_with(&["Documents"]);
        let u = Uri::parse("lautta://user-documents/My%20Files/caf%C3%A9%20%231.txt").unwrap();
        assert_eq!(
            e.reg.display_address(&u),
            format!(
                "file://{}/Documents/My%20Files/caf%C3%A9%20%231.txt",
                e.home.display()
            )
        );
        assert_eq!(
            e.reg.display_address(&Uri::root("user-documents")),
            format!("file://{}/Documents", e.home.display())
        );
    }

    #[test]
    fn non_utf8_local_paths_are_escaped_byte_for_byte() {
        let e = env_with(&["Documents"]);
        let u = Uri::new("user-documents", VPath::parse(b"caf\xe9").unwrap());
        assert!(e.reg.display_address(&u).ends_with("/Documents/caf%E9"));
    }

    #[test]
    fn remote_addresses_never_carry_secrets() {
        let e = env_with(&[]);
        let loc = |id: &str, url: &str| {
            (
                Location::remote(
                    id,
                    LocationKind::Server { provider: "x".into() },
                    id,
                    Some(url.into()),
                ),
                mem(),
            )
        };
        e.reg.set_remote(vec![
            loc("a", "sftp://me:hunter2@nas.local:2222/share?token=abc#frag"),
            loc("b", "smb://nas/media/"),
            loc("c", "https://user@dav.example.com"),
        ]);
        let at = |id: &str, path: &str| {
            e.reg
                .display_address(&Uri::new(id, VPath::parse(path.as_bytes()).unwrap()))
        };
        assert_eq!(at("a", ""), "sftp://me@nas.local:2222/share");
        assert_eq!(at("a", "x/y z.txt"), "sftp://me@nas.local:2222/share/x/y%20z.txt");
        assert_eq!(at("b", "Movies/a.mkv"), "smb://nas/media/Movies/a.mkv");
        assert_eq!(at("c", "d"), "https://user@dav.example.com/d");
        for id in ["a", "b", "c"] {
            let shown = at(id, "p");
            assert!(!shown.contains("hunter2") && !shown.contains("token") && !shown.contains("frag"));
        }
    }

    #[test]
    fn unknown_or_url_less_locations_fall_back_to_the_internal_uri() {
        let e = env_with(&[]);
        let u = Uri::parse("lautta://ghost/a/b").unwrap();
        assert_eq!(e.reg.display_address(&u), "lautta://ghost/a/b");
        e.reg.register(
            Location::remote("arc-1", LocationKind::Archive, "a.zip", None),
            mem(),
        );
        let u = Uri::parse("lautta://arc-1/inner/f").unwrap();
        assert_eq!(e.reg.display_address(&u), "lautta://arc-1/inner/f");
    }

    #[test]
    fn secrets_are_stripped_from_urls() {
        assert_eq!(strip_secrets("ftp://u:p@h/x"), "ftp://u@h/x");
        assert_eq!(strip_secrets("ftp://:p@h"), "ftp://@h");
        assert_eq!(strip_secrets("ftp://h/x?a=b"), "ftp://h/x");
        assert_eq!(strip_secrets("ftp://h/a@b"), "ftp://h/a@b");
        assert_eq!(strip_secrets("not a url"), "not a url");
    }

    #[test]
    fn uri_and_path_round_trip() {
        let e = env_with(&["Documents", "Music"]);
        let u = Uri::parse("lautta://user-music/Album/01%20Song.flac").unwrap();
        let p = e.reg.to_local_path(&u).unwrap();
        assert_eq!(p, e.home.join("Music/Album/01 Song.flac"));
        assert_eq!(e.reg.uri_for_local_path(&p), Some(u));
        assert_eq!(
            e.reg.to_local_path(&Uri::root("user-music")).unwrap(),
            e.home.join("Music")
        );
        assert_eq!(
            e.reg.uri_for_local_path(&e.home.join("Music")),
            Some(Uri::root("user-music"))
        );
    }

    #[test]
    fn longest_root_wins_and_outsiders_are_rejected() {
        let e = env_with(&["Documents", "Music"]);
        // a prefix of another name must not match: Documents vs Documents2
        std::fs::create_dir(e.home.join("Documents2")).unwrap();
        assert_eq!(e.reg.uri_for_local_path(&e.home.join("Documents2/x")), None);
        let nested = e.home.join("Documents/Shared");
        std::fs::create_dir(&nested).unwrap();
        e.reg.register(
            Location::local("vol-nested", LocationKind::Volume, "Nested", nested.clone()),
            mem(),
        );
        let u = e.reg.uri_for_local_path(&nested.join("deep/f")).unwrap();
        assert_eq!(u.location, "vol-nested");
        assert_eq!(u.path.as_bytes(), b"deep/f");
        let u = e.reg.uri_for_local_path(&e.home.join("Documents/other")).unwrap();
        assert_eq!(u.location, "user-documents");
        assert_eq!(e.reg.uri_for_local_path(Path::new("/etc/passwd")), None);
        assert_eq!(
            e.reg.uri_for_local_path(Path::new("Documents/x")),
            None,
            "relative"
        );
    }

    #[test]
    fn dot_dot_cannot_smuggle_a_path_into_another_location() {
        let e = env_with(&["Documents", "Music"]);
        let sneaky = e.home.join("Documents/../Music/song");
        let u = e.reg.uri_for_local_path(&sneaky).unwrap();
        assert_eq!(u.location, "user-music");
        assert_eq!(u.path.as_bytes(), b"song");
        let escape = e.home.join("Documents/../../elsewhere");
        assert_eq!(e.reg.uri_for_local_path(&escape), None);
    }

    #[test]
    fn non_utf8_paths_round_trip() {
        let e = env_with(&["Documents"]);
        let p = e.home.join(OsStr::from_bytes(b"Documents/caf\xe9/\xff"));
        let u = e.reg.uri_for_local_path(&p).unwrap();
        assert_eq!(u.path.as_bytes(), b"caf\xe9/\xff");
        assert_eq!(e.reg.to_local_path(&u).unwrap(), p);
        let reparsed = Uri::parse(&u.to_string()).unwrap();
        assert_eq!(reparsed, u);
    }

    #[test]
    fn remote_and_unknown_locations_have_no_local_path() {
        let e = env_with(&[]);
        e.reg.set_remote(vec![(
            Location::remote(
                "nv-1",
                LocationKind::Server {
                    provider: "sftp".into(),
                },
                "N",
                None,
            ),
            mem(),
        )]);
        assert_eq!(e.reg.to_local_path(&Uri::root("nv-1")), None);
        assert_eq!(e.reg.to_local_path(&Uri::root("zzz")), None);
    }
}
