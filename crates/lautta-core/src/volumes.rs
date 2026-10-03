// SPDX-License-Identifier: LGPL-2.1-or-later
//! Removable volumes (SPEC LOC-3): SD cards and USB drives mounted below
//! `/run/media/$USER/<label-or-uuid>`.
//!
//! SPEC LOC-3 also names UDisks2 signals as a source. That D-Bus path is out
//! of scope here: the folder scan plus an inotify watch on the media root
//! sees exactly the same mounts and unmounts (udisks creates and removes the
//! mount point directories), needs no D-Bus dependency in the core, and is
//! trivial to test. A UDisks2 listener can later feed [`VolumeEvent`]s into
//! the same registry call without changing anything else.

use crate::error::Result;
use crate::paths::AppPaths;
use crate::watch::{lock, map_notify_error, raw_watcher, spawn_worker, StopOnDrop};
use notify::{EventKind, RecommendedWatcher, RecursiveMode, Watcher};
use std::ffi::OsStr;
use std::os::unix::ffi::OsStrExt;
use std::path::{Path, PathBuf};
use std::sync::atomic::AtomicBool;
use std::sync::{Arc, Mutex};
use std::time::Duration;
use tokio::sync::mpsc::{unbounded_channel, UnboundedReceiver, UnboundedSender};

/// Quiet time after a mount change before the media root is rescanned
/// (udisks creates the directory a moment before the mount completes).
pub const VOLUME_DEBOUNCE: Duration = Duration::from_millis(300);

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Volume {
    /// The mount directory name, made safe for a location id.
    pub id: String,
    /// Shown to the user: the mount directory name (the volume label, or the
    /// UUID when it has none).
    pub name: String,
    pub mount_path: PathBuf,
}

impl Volume {
    /// `vol-<uuid-or-dirname>` (LOC-7).
    pub fn location_id(&self) -> String {
        format!("vol-{}", self.id)
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum VolumeEvent {
    Appeared(Volume),
    Disappeared(Volume),
}

/// Keeps ASCII letters, digits and `-.:_`; every other byte becomes `_xx`
/// (hex), so any directory name, including non-UTF-8 ones, gives a valid
/// `lautta://` location id.
pub fn sanitize_id(name: &OsStr) -> String {
    let mut out = String::with_capacity(name.len());
    for &b in name.as_bytes() {
        if b.is_ascii_alphanumeric() || matches!(b, b'-' | b'.' | b':' | b'_') {
            out.push(char::from(b));
        } else {
            out.push_str(&format!("_{b:02x}"));
        }
    }
    out
}

/// The media root of the current user: `/run/media/$USER`, with the home
/// folder's name standing in when `$USER` is unset.
pub fn media_root(paths: &AppPaths) -> PathBuf {
    let user = std::env::var("USER")
        .ok()
        .filter(|u| !u.is_empty())
        .unwrap_or_else(|| {
            paths
                .home
                .file_name()
                .map_or_else(|| "defaultuser".to_owned(), |n| n.to_string_lossy().into_owned())
        });
    paths.media_root(&user)
}

/// Lists the mounted volumes below `root`: readable folders, not hidden,
/// sorted by name. A missing root means no volumes.
pub fn scan(root: &Path) -> Vec<Volume> {
    let Ok(rd) = std::fs::read_dir(root) else {
        return Vec::new();
    };
    let mut out: Vec<Volume> = rd
        .flatten()
        .filter_map(|entry| {
            let name = entry.file_name();
            let path = entry.path();
            // follow links: some setups bind-mount through a symlink
            let is_dir = std::fs::metadata(&path).map(|m| m.is_dir()).unwrap_or(false);
            let usable = name.as_bytes().first() != Some(&b'.') && is_dir && std::fs::read_dir(&path).is_ok();
            usable.then(|| Volume {
                id: sanitize_id(&name),
                name: name.to_string_lossy().into_owned(),
                mount_path: path,
            })
        })
        .collect();
    out.sort_by(|a, b| a.name.cmp(&b.name).then_with(|| a.id.cmp(&b.id)));
    out
}

/// Changes between two scans: removals first, then additions.
pub fn diff(old: &[Volume], new: &[Volume]) -> Vec<VolumeEvent> {
    let gone = old
        .iter()
        .filter(|o| !new.iter().any(|n| n.id == o.id))
        .cloned()
        .map(VolumeEvent::Disappeared);
    let added = new
        .iter()
        .filter(|n| !old.iter().any(|o| o.id == n.id))
        .cloned()
        .map(VolumeEvent::Appeared);
    gone.chain(added).collect()
}

/// The deepest existing folder at or above `path`.
fn nearest_existing(path: &Path) -> PathBuf {
    path.ancestors()
        .find(|p| p.is_dir())
        .map_or_else(|| PathBuf::from("/"), Path::to_path_buf)
}

/// Rescans on mount changes and reports appearing and disappearing volumes.
pub struct VolumeWatcher {
    known: Arc<Mutex<Vec<Volume>>>,
    _watcher: Arc<Mutex<RecommendedWatcher>>,
    _stop: StopOnDrop,
}

/// State of the worker: which folder is currently watched.
struct Rescanner {
    root: PathBuf,
    known: Arc<Mutex<Vec<Volume>>>,
    tx: UnboundedSender<VolumeEvent>,
    watcher: Arc<Mutex<RecommendedWatcher>>,
    watching: PathBuf,
}

impl Rescanner {
    /// The root may be created or removed at any time; keep the watch on it,
    /// or on its nearest existing ancestor while it is missing.
    fn retarget(&mut self) {
        let want = nearest_existing(&self.root);
        if want == self.watching {
            return;
        }
        let mut w = lock(&self.watcher);
        let _ = w.unwatch(&self.watching);
        if w.watch(&want, RecursiveMode::NonRecursive).is_ok() {
            self.watching = want;
        }
    }

    /// Returns false when nobody listens any more.
    fn rescan(&mut self) -> bool {
        self.retarget();
        let now = scan(&self.root);
        let events = {
            let mut known = lock(&self.known);
            let events = diff(&known, &now);
            *known = now;
            events
        };
        events.into_iter().all(|e| self.tx.send(e).is_ok())
    }
}

impl VolumeWatcher {
    /// Starts watching `root` (which need not exist yet). Events arrive on
    /// the receiver; the volumes present now are in [`VolumeWatcher::current`].
    pub fn start(
        root: PathBuf,
        debounce: Duration,
    ) -> Result<(VolumeWatcher, UnboundedReceiver<VolumeEvent>)> {
        let (mut watcher, raw) = raw_watcher()?;
        let watching = nearest_existing(&root);
        watcher
            .watch(&watching, RecursiveMode::NonRecursive)
            .map_err(map_notify_error)?;
        let watcher = Arc::new(Mutex::new(watcher));
        let known = Arc::new(Mutex::new(scan(&root)));
        let (tx, rx) = unbounded_channel();
        let stop = Arc::new(AtomicBool::new(false));
        let mut rescanner = Rescanner {
            root,
            known: known.clone(),
            tx,
            watcher: watcher.clone(),
            watching,
        };
        spawn_worker(
            raw,
            debounce,
            stop.clone(),
            |ev| {
                if matches!(ev.kind, EventKind::Access(_)) {
                    Vec::new()
                } else {
                    vec![()]
                }
            },
            move |()| rescanner.rescan(),
        );
        Ok((
            VolumeWatcher {
                known,
                _watcher: watcher,
                _stop: StopOnDrop(stop),
            },
            rx,
        ))
    }

    /// The volumes as of the last scan.
    pub fn current(&self) -> Vec<Volume> {
        lock(&self.known).clone()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const SHORT: Duration = Duration::from_millis(60);

    async fn next(rx: &mut UnboundedReceiver<VolumeEvent>) -> Option<VolumeEvent> {
        tokio::time::timeout(Duration::from_secs(5), rx.recv())
            .await
            .ok()
            .flatten()
    }

    fn vol(id: &str) -> Volume {
        Volume {
            id: id.into(),
            name: id.into(),
            mount_path: PathBuf::from("/m").join(id),
        }
    }

    #[test]
    fn scan_lists_readable_non_hidden_folders_sorted() {
        let d = tempfile::tempdir().unwrap();
        std::fs::create_dir(d.path().join("SDCARD")).unwrap();
        std::fs::create_dir(d.path().join("0123-4567")).unwrap();
        std::fs::create_dir(d.path().join(".hidden")).unwrap();
        std::fs::write(d.path().join("not-a-volume"), "x").unwrap();
        std::os::unix::fs::symlink(d.path().join("SDCARD"), d.path().join("linked")).unwrap();
        std::os::unix::fs::symlink("/nonexistent", d.path().join("dangling")).unwrap();
        let vols = scan(d.path());
        let names: Vec<&str> = vols.iter().map(|v| v.name.as_str()).collect();
        assert_eq!(names, ["0123-4567", "SDCARD", "linked"]);
        assert_eq!(vols[1].mount_path, d.path().join("SDCARD"));
        assert_eq!(vols[1].location_id(), "vol-SDCARD");
    }

    #[test]
    fn scan_of_a_missing_root_is_empty() {
        let d = tempfile::tempdir().unwrap();
        assert!(scan(&d.path().join("nope")).is_empty());
    }

    #[test]
    fn ids_are_safe_for_uris_even_for_odd_names() {
        assert_eq!(sanitize_id(OsStr::new("SD-CARD_1.x:y")), "SD-CARD_1.x:y");
        assert_eq!(sanitize_id(OsStr::new("My Card")), "My_20Card");
        assert_eq!(sanitize_id(OsStr::from_bytes(b"a/b\xff")), "a_2fb_ff");
        assert_eq!(sanitize_id(OsStr::new("\u{e9}")), "_c3_a9");
        for name in ["My Card", "caf\u{e9} #1", "100%"] {
            let v = Volume {
                id: sanitize_id(OsStr::new(name)),
                name: name.into(),
                mount_path: PathBuf::new(),
            };
            let u = crate::uri::Uri::parse(&format!("lautta://{}/", v.location_id())).unwrap();
            assert_eq!(u.location, v.location_id());
        }
    }

    #[test]
    fn non_utf8_directory_names_get_valid_ids() {
        let d = tempfile::tempdir().unwrap();
        std::fs::create_dir(d.path().join(OsStr::from_bytes(b"caf\xe9"))).unwrap();
        let vols = scan(d.path());
        assert_eq!(vols.len(), 1);
        assert_eq!(vols[0].id, "caf_e9");
        assert!(vols[0].name.contains('\u{fffd}'));
        assert!(vols[0].mount_path.ends_with(OsStr::from_bytes(b"caf\xe9")));
    }

    #[test]
    fn diff_reports_removals_then_additions() {
        let old = [vol("a"), vol("b")];
        let new = [vol("b"), vol("c")];
        assert_eq!(
            diff(&old, &new),
            [
                VolumeEvent::Disappeared(vol("a")),
                VolumeEvent::Appeared(vol("c"))
            ]
        );
        assert!(diff(&old, &old).is_empty());
        assert!(diff(&[], &[]).is_empty());
    }

    #[test]
    fn media_root_uses_the_user_name() {
        let paths = AppPaths::new("/home/someone");
        let root = media_root(&paths);
        let user = std::env::var("USER").ok().filter(|u| !u.is_empty());
        assert_eq!(
            root,
            Path::new("/run/media").join(user.unwrap_or_else(|| "someone".into()))
        );
    }

    #[test]
    fn nearest_existing_walks_up() {
        let d = tempfile::tempdir().unwrap();
        assert_eq!(nearest_existing(&d.path().join("a/b/c")), d.path());
        assert_eq!(nearest_existing(d.path()), d.path());
    }

    #[tokio::test]
    async fn mounts_and_unmounts_are_reported() {
        let d = tempfile::tempdir().unwrap();
        let root = d.path().join("media");
        std::fs::create_dir(&root).unwrap();
        std::fs::create_dir(root.join("PRESENT")).unwrap();
        let (w, mut rx) = VolumeWatcher::start(root.clone(), SHORT).unwrap();
        assert_eq!(
            w.current().iter().map(|v| v.name.as_str()).collect::<Vec<_>>(),
            ["PRESENT"]
        );
        std::fs::create_dir(root.join("CARD")).unwrap();
        let ev = next(&mut rx).await.unwrap();
        assert_eq!(
            ev,
            VolumeEvent::Appeared(Volume {
                id: "CARD".into(),
                name: "CARD".into(),
                mount_path: root.join("CARD"),
            })
        );
        assert_eq!(w.current().len(), 2);
        std::fs::remove_dir(root.join("PRESENT")).unwrap();
        match next(&mut rx).await.unwrap() {
            VolumeEvent::Disappeared(v) => assert_eq!(v.id, "PRESENT"),
            other => panic!("unexpected {other:?}"),
        }
        assert_eq!(w.current().len(), 1);
    }

    #[tokio::test]
    async fn watching_starts_before_the_media_root_exists() {
        let d = tempfile::tempdir().unwrap();
        let root = d.path().join("run/media/user");
        let (w, mut rx) = VolumeWatcher::start(root.clone(), SHORT).unwrap();
        assert!(w.current().is_empty());
        std::fs::create_dir_all(root.join("SD")).unwrap();
        let mut seen = None;
        while let Some(ev) = next(&mut rx).await {
            if matches!(&ev, VolumeEvent::Appeared(v) if v.id == "SD") {
                seen = Some(ev);
                break;
            }
        }
        assert!(seen.is_some(), "volume below a late-created root must be found");
        // after the root exists the watch moved onto it: later mounts are seen
        std::fs::create_dir(root.join("USB")).unwrap();
        let ev = next(&mut rx).await.unwrap();
        assert!(matches!(ev, VolumeEvent::Appeared(v) if v.id == "USB"));
    }

    #[tokio::test]
    async fn removing_the_whole_root_reports_every_volume_gone() {
        let d = tempfile::tempdir().unwrap();
        let root = d.path().join("media");
        std::fs::create_dir_all(root.join("CARD")).unwrap();
        let (w, mut rx) = VolumeWatcher::start(root.clone(), SHORT).unwrap();
        std::fs::remove_dir_all(&root).unwrap();
        let mut gone = false;
        while let Some(ev) = next(&mut rx).await {
            if matches!(&ev, VolumeEvent::Disappeared(v) if v.id == "CARD") {
                gone = true;
                break;
            }
        }
        assert!(gone);
        assert!(w.current().is_empty());
        // and it comes back
        std::fs::create_dir_all(root.join("CARD2")).unwrap();
        let mut back = false;
        while let Some(ev) = next(&mut rx).await {
            if matches!(&ev, VolumeEvent::Appeared(v) if v.id == "CARD2") {
                back = true;
                break;
            }
        }
        assert!(back);
    }
}
