// SPDX-License-Identifier: LGPL-2.1-or-later
//! inotify watching with debouncing.
//!
//! [`DirWatcher`] watches local folders (not recursively) while open in the browser and sends one
//! [`WatchEvent::Changed`] per folder to each of its [`DirWatch`]es after changes were quiet for
//! the debounce time.
//!
//! Raw `notify` events are handled on a small worker thread owning the debounce state; consumers
//! get events through unbounded tokio channels. The thread stops when the watcher is dropped.

use crate::error::{Error, ErrorKind, Result};
use notify::event::{AccessKind, AccessMode};
use notify::{Event, EventKind, RecommendedWatcher, RecursiveMode, Watcher};
use std::collections::HashMap;
use std::hash::Hash;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::mpsc::{self, RecvTimeoutError};
use std::sync::{Arc, Mutex, MutexGuard};
use std::time::{Duration, Instant};
use tokio::sync::mpsc::{unbounded_channel, UnboundedReceiver, UnboundedSender};

pub const DIR_DEBOUNCE: Duration = Duration::from_millis(250);

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum WatchEvent {
    Changed(PathBuf),
}

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

    pub(crate) fn next_deadline(&self) -> Option<Instant> {
        self.pending.values().map(|last| *last + self.delay).min()
    }

    #[cfg(test)]
    fn is_empty(&self) -> bool {
        self.pending.is_empty()
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

/// Stops when the raw channel closes (its watcher dropped) or `emit` returns false (consumer
/// gone). Blocks while nothing is pending: no wakeups when idle. `classify` maps a raw event to
/// the keys it touches; `emit` delivers a due key.
pub(crate) fn spawn_worker<K, C, E>(
    raw: mpsc::Receiver<notify::Result<Event>>,
    delay: Duration,
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
            loop {
                let got = match debounce.next_deadline() {
                    None => raw.recv().map_err(|_| RecvTimeoutError::Disconnected),
                    Some(d) => raw.recv_timeout(d.saturating_duration_since(Instant::now())),
                };
                match got {
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
                for key in debounce.take_due(Instant::now()) {
                    if !emit(key) {
                        return;
                    }
                }
            }
        });
    if let Err(e) = spawned {
        log::error!("cannot start the watch thread: {e}");
    }
}

fn changes_listing(kind: &EventKind) -> bool {
    match kind {
        EventKind::Create(_) | EventKind::Remove(_) | EventKind::Modify(_) | EventKind::Any => true,
        EventKind::Access(AccessKind::Close(AccessMode::Write)) => true,
        EventKind::Access(_) | EventKind::Other => false,
    }
}

type Subscribers = HashMap<PathBuf, Vec<(u64, UnboundedSender<WatchEvent>)>>;

/// One inotify instance and one thread for all folders of the process: the kernel allows few
/// instances per user (128 by default), shared by every app.
#[derive(Clone)]
pub struct DirWatcher {
    shared: Arc<Shared>,
}

struct Shared {
    watcher: Mutex<RecommendedWatcher>,
    subs: Arc<Mutex<Subscribers>>,
    next_id: AtomicU64,
}

/// Follows one folder until dropped.
pub struct DirWatch {
    shared: Arc<Shared>,
    dir: PathBuf,
    id: u64,
}

impl DirWatcher {
    /// Changes arrive after `debounce` of quiet.
    pub fn new(debounce: Duration) -> Result<DirWatcher> {
        let (watcher, raw) = raw_watcher()?;
        let subs: Arc<Mutex<Subscribers>> = Arc::default();
        let classify_subs = subs.clone();
        let emit_subs = subs.clone();
        spawn_worker(
            raw,
            debounce,
            move |ev| dirs_touched(&lock(&classify_subs), ev),
            move |dir: PathBuf| {
                if let Some(list) = lock(&emit_subs).get(&dir) {
                    for (_, tx) in list {
                        let _ = tx.send(WatchEvent::Changed(dir.clone()));
                    }
                }
                true
            },
        );
        Ok(DirWatcher {
            shared: Arc::new(Shared {
                watcher: Mutex::new(watcher),
                subs,
                next_id: AtomicU64::new(0),
            }),
        })
    }

    /// Changes of `dir` arrive on the receiver while the [`DirWatch`] lives. `NotFound` if it
    /// does not exist.
    pub fn watch(&self, dir: &Path) -> Result<(DirWatch, UnboundedReceiver<WatchEvent>)> {
        let mut subs = lock(&self.shared.subs);
        if !subs.contains_key(dir) {
            lock(&self.shared.watcher)
                .watch(dir, RecursiveMode::NonRecursive)
                .map_err(map_notify_error)?;
        }
        let id = self.shared.next_id.fetch_add(1, Ordering::Relaxed);
        let (tx, rx) = unbounded_channel();
        subs.entry(dir.to_path_buf()).or_default().push((id, tx));
        let watch = DirWatch {
            shared: self.shared.clone(),
            dir: dir.to_path_buf(),
            id,
        };
        Ok((watch, rx))
    }

    pub fn watched(&self) -> Vec<PathBuf> {
        let mut v: Vec<PathBuf> = lock(&self.shared.subs).keys().cloned().collect();
        v.sort();
        v
    }
}

impl Drop for DirWatch {
    fn drop(&mut self) {
        let mut subs = lock(&self.shared.subs);
        let Some(list) = subs.get_mut(&self.dir) else {
            return;
        };
        list.retain(|(id, _)| *id != self.id);
        if !list.is_empty() {
            return;
        }
        subs.remove(&self.dir);
        // Fails once the folder, and with it the kernel watch, is gone: nothing left to undo.
        let _ = lock(&self.shared.watcher).unwatch(&self.dir);
    }
}

fn dirs_touched<V>(watched: &HashMap<PathBuf, V>, ev: &Event) -> Vec<PathBuf> {
    if ev.need_rescan() {
        // The kernel queue overflowed: assume everything changed.
        return watched.keys().cloned().collect();
    }
    if !changes_listing(&ev.kind) {
        return Vec::new();
    }
    let mut out: Vec<PathBuf> = Vec::new();
    for path in &ev.paths {
        let dir = path
            .parent()
            .filter(|p| watched.contains_key(*p))
            .or_else(|| watched.get_key_value(path).map(|(k, _)| k.as_path()));
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

    /// Channel ends; a channel that stays open but quiet is not closed.
    async fn closed<T>(rx: &mut UnboundedReceiver<T>) -> bool {
        let got = tokio::time::timeout(Duration::from_secs(30), rx.recv()).await;
        matches!(got, Ok(None))
    }

    async fn silent<T>(rx: &mut UnboundedReceiver<T>, ms: u64) -> bool {
        tokio::time::timeout(Duration::from_millis(ms), rx.recv())
            .await
            .is_err()
    }

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

    fn ev(kind: EventKind, paths: &[&str]) -> Event {
        let mut e = Event::new(kind);
        for p in paths {
            e = e.add_path(PathBuf::from(p));
        }
        e
    }

    #[test]
    fn dir_events_map_to_the_watched_parent() {
        let watched: HashMap<PathBuf, ()> = [(PathBuf::from("/w/a"), ()), (PathBuf::from("/w/b"), ())].into();
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
        assert_eq!(
            touched(ev(EventKind::Remove(RemoveKind::Folder), &["/w/a"])),
            [PathBuf::from("/w/a")]
        );
        let both = touched(ev(
            EventKind::Modify(ModifyKind::Name(RenameMode::Both)),
            &["/w/a/x", "/w/b/x"],
        ));
        assert_eq!(both.len(), 2);
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
        let watched: HashMap<PathBuf, ()> = [(PathBuf::from("/w/a"), ()), (PathBuf::from("/w/b"), ())].into();
        let e = Event::new(EventKind::Other).set_flag(notify::event::Flag::Rescan);
        let mut got = dirs_touched(&watched, &e);
        got.sort();
        assert_eq!(got, [PathBuf::from("/w/a"), PathBuf::from("/w/b")]);
    }

    /// One inotify event (`mkdir`): unlike a file write (create, modify, close), a stalled
    /// runner cannot split it into two debounced reports.
    fn one_change(path: &Path) {
        std::fs::create_dir(path).unwrap();
    }

    fn changed(dir: &tempfile::TempDir) -> Option<WatchEvent> {
        Some(WatchEvent::Changed(dir.path().to_path_buf()))
    }

    #[tokio::test]
    async fn dir_watcher_reports_a_burst_once() {
        let d = tempfile::tempdir().unwrap();
        // Long quiet time: a stalled writer on a loaded runner must not split the burst.
        let w = DirWatcher::new(Duration::from_secs(2)).unwrap();
        let (_watch, mut rx) = w.watch(d.path()).unwrap();
        for i in 0..10 {
            std::fs::write(d.path().join(format!("f{i}")), "x").unwrap();
        }
        assert_eq!(next(&mut rx, 30).await, changed(&d));
        assert!(silent(&mut rx, 500).await, "the burst must collapse to one event");
    }

    /// Fresh folder and watcher per change: a late event of one change can't stand in for another.
    async fn sees(act: impl FnOnce(&Path)) {
        let d = tempfile::tempdir().unwrap();
        std::fs::write(d.path().join("old"), "x").unwrap();
        let w = DirWatcher::new(SHORT).unwrap();
        let (_watch, mut rx) = w.watch(d.path()).unwrap();
        act(d.path());
        assert_eq!(next(&mut rx, 30).await, changed(&d));
    }

    #[tokio::test]
    async fn dir_watcher_sees_create_remove_rename_and_write() {
        sees(|d| std::fs::write(d.join("new"), "x").unwrap()).await;
        sees(|d| std::fs::rename(d.join("old"), d.join("renamed")).unwrap()).await;
        sees(|d| std::fs::remove_file(d.join("old")).unwrap()).await;
        sees(|d| {
            let mut f = std::fs::OpenOptions::new()
                .append(true)
                .open(d.join("old"))
                .unwrap();
            f.write_all(b"more").unwrap();
        })
        .await;
    }

    #[tokio::test]
    async fn dir_watcher_keeps_folders_apart_and_is_not_recursive() {
        let a = tempfile::tempdir().unwrap();
        let b = tempfile::tempdir().unwrap();
        std::fs::create_dir(a.path().join("sub")).unwrap();
        let w = DirWatcher::new(SHORT).unwrap();
        let (_wa, mut rx_a) = w.watch(a.path()).unwrap();
        let (_wb, mut rx_b) = w.watch(b.path()).unwrap();
        let mut both = vec![a.path().to_path_buf(), b.path().to_path_buf()];
        both.sort();
        assert_eq!(w.watched(), both);
        std::fs::write(b.path().join("x"), "x").unwrap();
        assert_eq!(next(&mut rx_b, 30).await, changed(&b));
        assert!(silent(&mut rx_a, 300).await, "only the changed folder hears it");
        std::fs::write(a.path().join("sub/deep"), "x").unwrap();
        assert!(silent(&mut rx_a, 400).await, "grandchildren are not watched");
    }

    #[tokio::test]
    async fn watches_of_one_folder_share_it_until_the_last_is_dropped() {
        let d = tempfile::tempdir().unwrap();
        let w = DirWatcher::new(SHORT).unwrap();
        let (first, mut rx1) = w.watch(d.path()).unwrap();
        let (second, mut rx2) = w.watch(d.path()).unwrap();
        assert_eq!(w.watched(), [d.path().to_path_buf()]);
        one_change(&d.path().join("x"));
        assert_eq!(next(&mut rx1, 30).await, changed(&d));
        assert_eq!(next(&mut rx2, 30).await, changed(&d));
        drop(first);
        assert!(closed(&mut rx1).await, "dropped watch closes its channel");
        assert_eq!(w.watched(), [d.path().to_path_buf()]);
        one_change(&d.path().join("y"));
        assert_eq!(next(&mut rx2, 30).await, changed(&d));
        drop(second);
        assert!(w.watched().is_empty());
        assert!(closed(&mut rx2).await);
    }

    #[tokio::test]
    async fn folder_can_be_watched_again_after_its_last_watch_is_dropped() {
        let d = tempfile::tempdir().unwrap();
        let w = DirWatcher::new(SHORT).unwrap();
        drop(w.watch(d.path()).unwrap());
        let (_watch, mut rx) = w.watch(d.path()).unwrap();
        std::fs::write(d.path().join("x"), "x").unwrap();
        assert_eq!(next(&mut rx, 30).await, changed(&d));
    }

    #[tokio::test]
    async fn dropped_receiver_does_not_stop_other_watches() {
        let a = tempfile::tempdir().unwrap();
        let w = DirWatcher::new(SHORT).unwrap();
        let (_gone, rx_gone) = w.watch(a.path()).unwrap();
        drop(rx_gone);
        let (_kept, mut rx) = w.watch(a.path()).unwrap();
        one_change(&a.path().join("x"));
        assert_eq!(next(&mut rx, 30).await, changed(&a));
        one_change(&a.path().join("y"));
        assert_eq!(next(&mut rx, 30).await, changed(&a));
    }

    #[tokio::test]
    async fn watch_outlives_its_watcher_handle() {
        let d = tempfile::tempdir().unwrap();
        let w = DirWatcher::new(SHORT).unwrap();
        let (_watch, mut rx) = w.watch(d.path()).unwrap();
        drop(w);
        std::fs::write(d.path().join("x"), "x").unwrap();
        assert_eq!(next(&mut rx, 30).await, changed(&d));
    }

    #[tokio::test]
    async fn removed_folder_reports_and_drops_cleanly() {
        let parent = tempfile::tempdir().unwrap();
        let dir = parent.path().join("gone");
        std::fs::create_dir(&dir).unwrap();
        let w = DirWatcher::new(SHORT).unwrap();
        let (watch, mut rx) = w.watch(&dir).unwrap();
        std::fs::remove_dir(&dir).unwrap();
        assert_eq!(next(&mut rx, 30).await, Some(WatchEvent::Changed(dir.clone())));
        drop(watch);
        assert!(w.watched().is_empty());
    }

    #[tokio::test]
    async fn watching_a_missing_folder_is_not_found() {
        let d = tempfile::tempdir().unwrap();
        let w = DirWatcher::new(SHORT).unwrap();
        let err = w.watch(&d.path().join("missing")).err().unwrap();
        assert_eq!(err.kind, ErrorKind::NotFound);
        assert!(w.watched().is_empty());
    }

    #[tokio::test]
    async fn dir_watcher_handles_non_utf8_names() {
        use std::os::unix::ffi::OsStrExt;
        let d = tempfile::tempdir().unwrap();
        let w = DirWatcher::new(SHORT).unwrap();
        let (_watch, mut rx) = w.watch(d.path()).unwrap();
        std::fs::write(d.path().join(std::ffi::OsStr::from_bytes(b"\xff\xfe")), "x").unwrap();
        assert!(next(&mut rx, 30).await.is_some());
    }

    #[test]
    fn the_worker_ends_once_its_consumer_is_gone() {
        struct Ended(mpsc::Sender<()>);
        impl Drop for Ended {
            fn drop(&mut self) {
                let _ = self.0.send(());
            }
        }
        let (raw_tx, raw) = mpsc::channel();
        let (ended_tx, ended) = mpsc::channel();
        let guard = Ended(ended_tx);
        spawn_worker(
            raw,
            Duration::ZERO,
            |_: &Event| vec![()],
            move |()| {
                let _ = &guard;
                false
            },
        );
        raw_tx.send(Ok(Event::new(EventKind::Any))).unwrap();
        ended
            .recv_timeout(Duration::from_secs(30))
            .expect("worker thread returns once emit reports no consumer");
        drop(raw_tx);
    }

    #[test]
    fn the_worker_ends_once_its_watcher_is_gone() {
        let (ended_tx, ended) = mpsc::channel::<()>();
        let (raw_tx, raw) = mpsc::channel::<notify::Result<Event>>();
        spawn_worker(
            raw,
            Duration::ZERO,
            |_: &Event| Vec::<()>::new(),
            move |()| {
                let _ = &ended_tx;
                true
            },
        );
        drop(raw_tx);
        assert_eq!(
            ended.recv_timeout(Duration::from_secs(30)),
            Err(mpsc::RecvTimeoutError::Disconnected),
            "worker thread returns once the raw channel closes"
        );
    }
}
