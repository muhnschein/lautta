// SPDX-License-Identifier: LGPL-2.1-or-later
//! inotify watching with debouncing (SPEC BRW-6).
//!
//! [`DirWatcher`] watches a set of local folders (not recursively) while
//! they are open in the browser and sends one [`WatchEvent::Changed`] per
//! folder after changes have been quiet for the debounce time.
//!
//! Raw `notify` events are handled on a small worker thread that owns the
//! debounce state; consumers get events through an unbounded tokio channel.
//! The watcher stops its thread when dropped.

use crate::error::{Error, ErrorKind, Result};
use notify::event::{AccessKind, AccessMode};
use notify::{Event, EventKind, RecommendedWatcher, RecursiveMode, Watcher};
use std::collections::{HashMap, HashSet};
use std::hash::Hash;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc::{self, RecvTimeoutError};
use std::sync::{Arc, Mutex, MutexGuard};
use std::time::{Duration, Instant};
use tokio::sync::mpsc::{unbounded_channel, UnboundedReceiver};

/// Quiet time before a folder change is reported.
pub const DIR_DEBOUNCE: Duration = Duration::from_millis(250);
/// How often the worker checks whether it should stop.
const STOP_POLL: Duration = Duration::from_millis(100);

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum WatchEvent {
    /// Something in this folder was created, removed, renamed or modified.
    Changed(PathBuf),
}

/// Collapses bursts: a key is due once no new event for it arrived for `delay`.
pub(crate) struct Debouncer<K> {
    delay: Duration,
    pending: HashMap<K, Instant>,
}

impl<K: Eq + Hash + Clone> Debouncer<K> {
    pub(crate) fn new(delay: Duration) -> Debouncer<K> {
        Debouncer {
            delay,
            pending: HashMap::new(),
        }
    }

    pub(crate) fn push(&mut self, key: K, now: Instant) {
        self.pending.insert(key, now);
    }

    /// Removes and returns the keys whose quiet period has passed.
    pub(crate) fn take_due(&mut self, now: Instant) -> Vec<K> {
        let delay = self.delay;
        let due: Vec<K> = self
            .pending
            .iter()
            .filter(|(_, last)| now.saturating_duration_since(**last) >= delay)
            .map(|(k, _)| k.clone())
            .collect();
        for k in &due {
            self.pending.remove(k);
        }
        due
    }

    /// When the next key becomes due.
    pub(crate) fn next_deadline(&self) -> Option<Instant> {
        self.pending.values().map(|last| *last + self.delay).min()
    }

    pub(crate) fn is_empty(&self) -> bool {
        self.pending.is_empty()
    }
}

/// Tells the worker thread to stop when its owner is dropped.
pub(crate) struct StopOnDrop(pub(crate) Arc<AtomicBool>);

impl Drop for StopOnDrop {
    fn drop(&mut self) {
        self.0.store(true, Ordering::Relaxed);
    }
}

pub(crate) fn map_notify_error(e: notify::Error) -> Error {
    match e.kind {
        notify::ErrorKind::PathNotFound => Error::new(ErrorKind::NotFound, "path to watch does not exist"),
        notify::ErrorKind::Io(io) => Error::from_io(&io),
        notify::ErrorKind::MaxFilesWatch => Error::new(ErrorKind::Io, "inotify watch limit reached"),
        other => Error::new(ErrorKind::Io, format!("watch failed: {other:?}")),
    }
}

/// A `notify` watcher whose raw events arrive on a std channel.
pub(crate) fn raw_watcher() -> Result<(RecommendedWatcher, mpsc::Receiver<notify::Result<Event>>)> {
    let (tx, rx) = mpsc::channel();
    let watcher = notify::recommended_watcher(move |res| {
        // The worker may already be gone during shutdown.
        let _ = tx.send(res);
    })
    .map_err(map_notify_error)?;
    Ok((watcher, rx))
}

pub(crate) fn lock<T>(m: &Mutex<T>) -> MutexGuard<'_, T> {
    match m.lock() {
        Ok(g) => g,
        Err(p) => p.into_inner(),
    }
}

/// Runs the debounce loop on its own thread until `stop` is set, the raw
/// channel closes, or `emit` returns false (the consumer is gone).
/// `classify` maps a raw event to the keys it touches; `emit` delivers a
/// due key.
pub(crate) fn spawn_worker<K, C, E>(
    raw: mpsc::Receiver<notify::Result<Event>>,
    delay: Duration,
    stop: Arc<AtomicBool>,
    classify: C,
    mut emit: E,
) where
    K: Eq + Hash + Clone + Send + 'static,
    C: Fn(&Event) -> Vec<K> + Send + 'static,
    E: FnMut(K) -> bool + Send + 'static,
{
    let spawned = std::thread::Builder::new()
        .name("lautta-watch".into())
        .spawn(move || {
            let mut debounce = Debouncer::new(delay);
            while !stop.load(Ordering::Relaxed) {
                let wait = debounce.next_deadline().map_or(STOP_POLL, |d| {
                    d.saturating_duration_since(Instant::now()).min(STOP_POLL)
                });
                match raw.recv_timeout(wait) {
                    Ok(Ok(event)) => {
                        let now = Instant::now();
                        for key in classify(&event) {
                            debounce.push(key, now);
                        }
                    }
                    Ok(Err(e)) => log::warn!("file watch error: {e}"),
                    Err(RecvTimeoutError::Timeout) => {}
                    Err(RecvTimeoutError::Disconnected) => break,
                }
                if !debounce.is_empty() {
                    for key in debounce.take_due(Instant::now()) {
                        if !emit(key) {
                            return;
                        }
                    }
                }
            }
        });
    if let Err(e) = spawned {
        log::error!("cannot start the watch thread: {e}");
    }
}

/// Events that change what a folder listing shows.
fn changes_listing(kind: &EventKind) -> bool {
    match kind {
        EventKind::Create(_) | EventKind::Remove(_) | EventKind::Modify(_) | EventKind::Any => true,
        EventKind::Access(AccessKind::Close(AccessMode::Write)) => true,
        EventKind::Access(_) | EventKind::Other => false,
    }
}

/// Watches folders for changes (BRW-6).
pub struct DirWatcher {
    watcher: Mutex<RecommendedWatcher>,
    watched: Arc<Mutex<HashSet<PathBuf>>>,
    _stop: StopOnDrop,
}

impl DirWatcher {
    /// Creates a watcher; changes arrive on the returned receiver after
    /// `debounce` of quiet.
    pub fn new(debounce: Duration) -> Result<(DirWatcher, UnboundedReceiver<WatchEvent>)> {
        let (watcher, raw) = raw_watcher()?;
        let (tx, rx) = unbounded_channel();
        let watched: Arc<Mutex<HashSet<PathBuf>>> = Arc::default();
        let stop = Arc::new(AtomicBool::new(false));
        let shared = watched.clone();
        spawn_worker(
            raw,
            debounce,
            stop.clone(),
            move |ev| dirs_touched(&lock(&shared), ev),
            move |dir: PathBuf| tx.send(WatchEvent::Changed(dir)).is_ok(),
        );
        Ok((
            DirWatcher {
                watcher: Mutex::new(watcher),
                watched,
                _stop: StopOnDrop(stop),
            },
            rx,
        ))
    }

    /// Starts watching `dir` (no-op when already watched). `NotFound` if it
    /// does not exist.
    pub fn watch(&self, dir: &Path) -> Result<()> {
        let mut set = lock(&self.watched);
        if set.contains(dir) {
            return Ok(());
        }
        lock(&self.watcher)
            .watch(dir, RecursiveMode::NonRecursive)
            .map_err(map_notify_error)?;
        set.insert(dir.to_path_buf());
        Ok(())
    }

    /// Stops watching `dir`; unknown folders are ignored.
    pub fn unwatch(&self, dir: &Path) -> Result<()> {
        if !lock(&self.watched).remove(dir) {
            return Ok(());
        }
        match lock(&self.watcher).unwatch(dir) {
            // The folder (and with it the kernel watch) may be gone already.
            Err(e) if matches!(e.kind, notify::ErrorKind::WatchNotFound) => Ok(()),
            other => other.map_err(map_notify_error),
        }
    }

    pub fn watched(&self) -> Vec<PathBuf> {
        let mut v: Vec<PathBuf> = lock(&self.watched).iter().cloned().collect();
        v.sort();
        v
    }
}

/// Which watched folders a raw event concerns.
fn dirs_touched(watched: &HashSet<PathBuf>, ev: &Event) -> Vec<PathBuf> {
    if ev.need_rescan() {
        // The kernel queue overflowed: assume everything changed.
        return watched.iter().cloned().collect();
    }
    if !changes_listing(&ev.kind) {
        return Vec::new();
    }
    let mut out: Vec<PathBuf> = Vec::new();
    for path in &ev.paths {
        let dir = path
            .parent()
            .filter(|p| watched.contains(*p))
            .or_else(|| watched.get(path).map(PathBuf::as_path));
        if let Some(dir) = dir {
            if !out.iter().any(|d| d == dir) {
                out.push(dir.to_path_buf());
            }
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use notify::event::{CreateKind, DataChange, ModifyKind, RemoveKind, RenameMode};
    use std::io::Write;

    const SHORT: Duration = Duration::from_millis(80);

    async fn next<T>(rx: &mut UnboundedReceiver<T>, secs: u64) -> Option<T> {
        tokio::time::timeout(Duration::from_secs(secs), rx.recv())
            .await
            .ok()
            .flatten()
    }

    async fn silent<T>(rx: &mut UnboundedReceiver<T>, ms: u64) -> bool {
        tokio::time::timeout(Duration::from_millis(ms), rx.recv())
            .await
            .is_err()
    }

    // ---- debouncer (deterministic) ---------------------------------------

    #[test]
    fn debouncer_waits_for_quiet_and_restarts_on_new_events() {
        let t0 = Instant::now();
        let mut d = Debouncer::new(Duration::from_millis(100));
        assert!(d.is_empty() && d.next_deadline().is_none());
        d.push("a", t0);
        assert!(d.take_due(t0 + Duration::from_millis(99)).is_empty());
        d.push("a", t0 + Duration::from_millis(90));
        assert!(
            d.take_due(t0 + Duration::from_millis(150)).is_empty(),
            "burst extends the wait"
        );
        assert_eq!(d.next_deadline(), Some(t0 + Duration::from_millis(190)));
        assert_eq!(d.take_due(t0 + Duration::from_millis(190)), ["a"]);
        assert!(d.is_empty());
        assert!(
            d.take_due(t0 + Duration::from_secs(5)).is_empty(),
            "reported once"
        );
    }

    #[test]
    fn debouncer_keys_are_independent() {
        let t0 = Instant::now();
        let mut d = Debouncer::new(Duration::from_millis(100));
        d.push(1, t0);
        d.push(2, t0 + Duration::from_millis(60));
        assert_eq!(d.take_due(t0 + Duration::from_millis(110)), [1]);
        assert_eq!(d.next_deadline(), Some(t0 + Duration::from_millis(160)));
        assert_eq!(d.take_due(t0 + Duration::from_millis(160)), [2]);
    }

    // ---- classification (deterministic) ----------------------------------

    fn ev(kind: EventKind, paths: &[&str]) -> Event {
        let mut e = Event::new(kind);
        for p in paths {
            e = e.add_path(PathBuf::from(p));
        }
        e
    }

    #[test]
    fn dir_events_map_to_the_watched_parent() {
        let watched: HashSet<PathBuf> = [PathBuf::from("/w/a"), PathBuf::from("/w/b")].into();
        let touched = |e: Event| dirs_touched(&watched, &e);
        assert_eq!(
            touched(ev(EventKind::Create(CreateKind::File), &["/w/a/x"])),
            [PathBuf::from("/w/a")]
        );
        assert_eq!(
            touched(ev(EventKind::Remove(RemoveKind::File), &["/w/b/x"])),
            [PathBuf::from("/w/b")]
        );
        assert_eq!(
            touched(ev(
                EventKind::Modify(ModifyKind::Data(DataChange::Any)),
                &["/w/a/x"]
            )),
            [PathBuf::from("/w/a")]
        );
        assert_eq!(
            touched(ev(
                EventKind::Access(AccessKind::Close(AccessMode::Write)),
                &["/w/a/x"]
            )),
            [PathBuf::from("/w/a")]
        );
        // the watched folder itself (deleted or renamed)
        assert_eq!(
            touched(ev(EventKind::Remove(RemoveKind::Folder), &["/w/a"])),
            [PathBuf::from("/w/a")]
        );
        // moves between two watched folders touch both, once each
        let both = touched(ev(
            EventKind::Modify(ModifyKind::Name(RenameMode::Both)),
            &["/w/a/x", "/w/b/x"],
        ));
        assert_eq!(both.len(), 2);
        // not watched, grandchildren, reads and opens
        assert!(touched(ev(EventKind::Create(CreateKind::File), &["/w/c/x"])).is_empty());
        assert!(touched(ev(EventKind::Create(CreateKind::File), &["/w/a/sub/x"])).is_empty());
        assert!(touched(ev(EventKind::Access(AccessKind::Read), &["/w/a/x"])).is_empty());
        assert!(touched(ev(
            EventKind::Access(AccessKind::Close(AccessMode::Read)),
            &["/w/a/x"]
        ))
        .is_empty());
    }

    #[test]
    fn overflow_marks_every_folder_changed() {
        let watched: HashSet<PathBuf> = [PathBuf::from("/w/a"), PathBuf::from("/w/b")].into();
        let e = Event::new(EventKind::Other).set_flag(notify::event::Flag::Rescan);
        let mut got = dirs_touched(&watched, &e);
        got.sort();
        assert_eq!(got, [PathBuf::from("/w/a"), PathBuf::from("/w/b")]);
    }

    // ---- real inotify ----------------------------------------------------

    #[tokio::test]
    async fn dir_watcher_reports_a_burst_once() {
        let d = tempfile::tempdir().unwrap();
        let (w, mut rx) = DirWatcher::new(Duration::from_millis(200)).unwrap();
        w.watch(d.path()).unwrap();
        for i in 0..10 {
            std::fs::write(d.path().join(format!("f{i}")), "x").unwrap();
        }
        let first = next(&mut rx, 5).await.unwrap();
        assert_eq!(first, WatchEvent::Changed(d.path().to_path_buf()));
        assert!(silent(&mut rx, 500).await, "the burst must collapse to one event");
    }

    #[tokio::test]
    async fn dir_watcher_sees_create_remove_rename_and_write() {
        let d = tempfile::tempdir().unwrap();
        std::fs::write(d.path().join("old"), "x").unwrap();
        let (w, mut rx) = DirWatcher::new(SHORT).unwrap();
        w.watch(d.path()).unwrap();
        w.watch(d.path()).unwrap();
        let expect = WatchEvent::Changed(d.path().to_path_buf());
        std::fs::write(d.path().join("new"), "x").unwrap();
        assert_eq!(next(&mut rx, 5).await, Some(expect.clone()));
        std::fs::rename(d.path().join("old"), d.path().join("renamed")).unwrap();
        assert_eq!(next(&mut rx, 5).await, Some(expect.clone()));
        std::fs::remove_file(d.path().join("new")).unwrap();
        assert_eq!(next(&mut rx, 5).await, Some(expect.clone()));
        let mut f = std::fs::OpenOptions::new()
            .append(true)
            .open(d.path().join("renamed"))
            .unwrap();
        f.write_all(b"more").unwrap();
        drop(f);
        assert_eq!(next(&mut rx, 5).await, Some(expect));
    }

    #[tokio::test]
    async fn dir_watcher_keeps_folders_apart_and_is_not_recursive() {
        let a = tempfile::tempdir().unwrap();
        let b = tempfile::tempdir().unwrap();
        std::fs::create_dir(a.path().join("sub")).unwrap();
        let (w, mut rx) = DirWatcher::new(SHORT).unwrap();
        w.watch(a.path()).unwrap();
        w.watch(b.path()).unwrap();
        assert_eq!(w.watched().len(), 2);
        std::fs::write(b.path().join("x"), "x").unwrap();
        assert_eq!(
            next(&mut rx, 5).await,
            Some(WatchEvent::Changed(b.path().to_path_buf()))
        );
        assert!(silent(&mut rx, 300).await);
        std::fs::write(a.path().join("sub/deep"), "x").unwrap();
        assert!(silent(&mut rx, 400).await, "grandchildren are not watched");
    }

    #[tokio::test]
    async fn unwatch_stops_events() {
        let d = tempfile::tempdir().unwrap();
        let (w, mut rx) = DirWatcher::new(SHORT).unwrap();
        w.watch(d.path()).unwrap();
        w.unwatch(d.path()).unwrap();
        w.unwatch(d.path()).unwrap();
        assert!(w.watched().is_empty());
        std::fs::write(d.path().join("x"), "x").unwrap();
        assert!(silent(&mut rx, 400).await);
    }

    #[tokio::test]
    async fn watching_a_missing_folder_is_not_found() {
        let d = tempfile::tempdir().unwrap();
        let (w, _rx) = DirWatcher::new(SHORT).unwrap();
        let err = w.watch(&d.path().join("missing")).unwrap_err();
        assert_eq!(err.kind, ErrorKind::NotFound);
        assert!(w.watched().is_empty());
    }

    #[tokio::test]
    async fn dir_watcher_handles_non_utf8_names() {
        use std::os::unix::ffi::OsStrExt;
        let d = tempfile::tempdir().unwrap();
        let (w, mut rx) = DirWatcher::new(SHORT).unwrap();
        w.watch(d.path()).unwrap();
        std::fs::write(d.path().join(std::ffi::OsStr::from_bytes(b"\xff\xfe")), "x").unwrap();
        assert!(next(&mut rx, 5).await.is_some());
    }

    #[tokio::test]
    async fn dropping_the_receiver_ends_the_worker_quietly() {
        let d = tempfile::tempdir().unwrap();
        let (w, rx) = DirWatcher::new(SHORT).unwrap();
        w.watch(d.path()).unwrap();
        drop(rx);
        std::fs::write(d.path().join("x"), "x").unwrap();
        tokio::time::sleep(Duration::from_millis(300)).await;
        std::fs::write(d.path().join("y"), "x").unwrap();
    }
}
