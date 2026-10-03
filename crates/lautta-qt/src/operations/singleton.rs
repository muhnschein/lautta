// SPDX-License-Identifier: LGPL-2.1-or-later
//! The `Operations` singleton: copy/move with plan summaries, delete,
//! conflicts before the run, compress/extract and archives as locations.
//!
//! All methods take `&self` and keep their state in cells: QML may call back
//! from inside a signal handler while this object is still on the stack.

use super::{error_parts, parse_uri, parse_uris};
use crate::json::{from_json, to_json};
use crate::runtime::{core, handle, spawn_then};
use lautta_core::app::{Core, Started};
use lautta_core::app_operations::{parse_choice, ConflictEntry, PendingPlans, StartOptions};
use lautta_core::compress::ArchiveKind;
use lautta_core::ops::{OperationKind, Plan};
use lautta_core::transfer::{TransferEvent, TransferState};
use lautta_core::undo::UndoAction;
use lautta_core::{Error, ErrorKind, Result, Uri};
use qmetaobject::prelude::*;
use qmetaobject::{QPointer, QSingletonInit};
use std::cell::{Cell, RefCell};
use std::collections::HashMap;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::Arc;
use std::time::{Duration, Instant};

/// At most one progress signal per job in this interval.
const PROGRESS_INTERVAL: Duration = Duration::from_millis(250);

/// What starting an operation led to.
enum Launched {
    Transfer(i64),
    Summary(Box<Plan>, String),
}

#[derive(Default)]
struct State {
    /// Folder to open when the extract transfer with this id completes.
    open_after: HashMap<i64, String>,
    /// The same, for extract plans that still wait for their summary.
    open_after_plan: HashMap<u64, String>,
    jobs: HashMap<i64, Arc<AtomicBool>>,
    listening: bool,
}

#[derive(QObject, Default)]
pub struct Operations {
    base: qt_base_class!(trait QObject),
    jobsActive: qt_property!(i32; READ jobs_active NOTIFY jobs_changed),
    jobs_changed: qt_signal!(),

    copyTo: qt_method!(fn(&self, uris_json: QString, dest_uri: QString)),
    moveTo: qt_method!(fn(&self, uris_json: QString, dest_uri: QString)),
    startPlan: qt_method!(fn(&self, plan_id: i32)),
    startPlanWith: qt_method!(fn(&self, plan_id: i32, options_json: QString)),
    discardPlan: qt_method!(fn(&self, plan_id: i32)),
    conflictsJson: qt_method!(fn(&self, plan_id: i32) -> QString),
    resolveConflict: qt_method!(fn(&self, plan_id: i32, index: i32, choice: QString, apply_to_all: bool)),
    remove: qt_method!(fn(&self, uris_json: QString)),
    compress: qt_method!(fn(&self, uris_json: QString, dest_uri: QString, name: QString, kind: QString)),
    cancelJob: qt_method!(fn(&self, job_id: i32)),
    measure: qt_method!(fn(&self, uris_json: QString)),
    extract: qt_method!(fn(&self, archive_uri: QString, dest_uri: QString)),
    extractAndOpen: qt_method!(fn(&self, archive_uri: QString, dest_uri: QString)),
    inspectArchive: qt_method!(fn(&self, archive_uri: QString)),
    openArchive: qt_method!(fn(&self, uri: QString)),
    isArchive: qt_method!(fn(&self, uri: QString) -> bool),
    archiveSource: qt_method!(fn(&self, uri: QString) -> QString),
    displayPath: qt_method!(fn(&self, uri: QString) -> QString),
    probeShared: qt_method!(fn(&self, paths_json: QString) -> QString),
    destinationsJson: qt_method!(fn(&self) -> QString),

    needsSummary: qt_signal!(planId: i32, summaryJson: QString),
    started: qt_signal!(transferId: i64),
    failed: qt_signal!(kind: QString, message: QString),
    removed: qt_signal!(trashedCount: i32, transferId: i64),
    conflictResolved: qt_signal!(planId: i32, remaining: i32),
    jobStarted: qt_signal!(jobId: i32),
    jobProgress: qt_signal!(jobId: i32, done: i64, total: i64),
    jobFinished: qt_signal!(jobId: i32, ok: bool, resultUri: QString),
    measured: qt_signal!(infoJson: QString),
    archiveInspected: qt_signal!(uri: QString, infoJson: QString),
    archiveOpened: qt_signal!(uri: QString, rootUri: QString),
    extractFinished: qt_signal!(transferId: i64, ok: bool, destUri: QString),

    pending: Arc<PendingPlans>,
    next_job: Cell<i32>,
    jobs_count: Cell<i32>,
    state: RefCell<State>,
}

impl QSingletonInit for Operations {
    fn init(&mut self) {}
}

fn plan_id(id: u64) -> i32 {
    i32::try_from(id).unwrap_or(i32::MAX)
}

fn trash_ids(core: &Core) -> Vec<i64> {
    match core.undo_kind() {
        Some(UndoAction::Trash { ids }) => ids,
        _ => Vec::new(),
    }
}

async fn launch(core: &Core, started: Result<Started>) -> Result<Launched> {
    Ok(match started? {
        Started::Transfer(id) => Launched::Transfer(id),
        Started::NeedsSummary(plan) => {
            let summary = to_json(&core.plan_summary(&plan).await);
            Launched::Summary(plan, summary)
        }
    })
}

fn bad_arguments() -> Error {
    Error::new(ErrorKind::InvalidArgument, "bad arguments")
}

/// `file:///a%20b` or `/a b` as a path.
fn local_path(text: &str) -> std::path::PathBuf {
    let raw = text.strip_prefix("file://").unwrap_or(text);
    let bytes = raw.as_bytes();
    let mut out = Vec::with_capacity(bytes.len());
    let mut i = 0;
    while i < bytes.len() {
        let hex = |b: u8| char::from(b).to_digit(16);
        let escaped = match (
            bytes[i],
            bytes.get(i + 1).copied().and_then(hex),
            bytes.get(i + 2).copied().and_then(hex),
        ) {
            (b'%', Some(hi), Some(lo)) if text.starts_with("file://") => u8::try_from(hi * 16 + lo).ok(),
            _ => None,
        };
        match escaped {
            Some(b) => {
                out.push(b);
                i += 3;
            }
            None => {
                out.push(bytes[i]);
                i += 1;
            }
        }
    }
    <std::ffi::OsStr as std::os::unix::ffi::OsStrExt>::from_bytes(&out).into()
}

impl Operations {
    fn jobs_active(&self) -> i32 {
        self.jobs_count.get()
    }

    fn emit_failed(&self, e: &Error) {
        let (kind, message) = error_parts(e);
        self.failed(kind, message);
    }

    fn change_jobs(&self, delta: i32) {
        self.jobs_count.set((self.jobs_count.get() + delta).max(0));
        self.jobs_changed();
    }

    // ------------------------------------------------------ copy and move

    fn copyTo(&self, uris_json: QString, dest_uri: QString) {
        self.transfer_op(OperationKind::Copy, &uris_json, &dest_uri);
    }

    fn moveTo(&self, uris_json: QString, dest_uri: QString) {
        self.transfer_op(OperationKind::Move, &uris_json, &dest_uri);
    }

    fn transfer_op(&self, kind: OperationKind, uris_json: &QString, dest_uri: &QString) {
        let (Some(core), Some(sources), Some(dest)) = (core(), parse_uris(uris_json), parse_uri(dest_uri))
        else {
            return self.emit_failed(&bad_arguments());
        };
        let me = QPointer::from(self);
        spawn_then(
            async move { launch(&core, core.copy_or_move(kind, sources, dest).await).await },
            move |res| {
                if let Some(p) = me.as_pinned() {
                    p.borrow().arrived(res, None);
                }
            },
        );
    }

    /// A started or planned operation reached the GUI thread.
    fn arrived(&self, res: Result<Launched>, open_after: Option<String>) {
        match res {
            Ok(Launched::Transfer(id)) => {
                if let Some(dest) = open_after {
                    self.state.borrow_mut().open_after.insert(id, dest);
                    self.listen();
                }
                self.started(id);
            }
            Ok(Launched::Summary(plan, summary)) => {
                let id = self.pending.insert(*plan);
                if let Some(dest) = open_after {
                    self.state.borrow_mut().open_after_plan.insert(id, dest);
                }
                self.needsSummary(plan_id(id), QString::from(summary.as_str()));
            }
            Err(e) => self.emit_failed(&e),
        }
    }

    fn startPlan(&self, plan_id: i32) {
        self.start_pending(plan_id, None);
    }

    fn startPlanWith(&self, plan_id: i32, options_json: QString) {
        self.start_pending(plan_id, Some(options_json.to_string()));
    }

    fn start_pending(&self, id: i32, options: Option<String>) {
        let id = u64::try_from(id).unwrap_or(0);
        let (Some(core), Some(plan)) = (core(), self.pending.take(id)) else {
            return self.emit_failed(&Error::kind(ErrorKind::NotFound));
        };
        let open_after = self.state.borrow_mut().open_after_plan.remove(&id);
        let me = QPointer::from(self);
        spawn_then(
            async move {
                let started = match options {
                    Some(text) => {
                        let wanted = StartOptions::from_settings(&core.settings()).merged_with_json(&text);
                        core.start_plan_with(plan, wanted).await
                    }
                    None => core.start_plan(plan).await,
                };
                started.map(Launched::Transfer)
            },
            move |res| {
                if let Some(p) = me.as_pinned() {
                    p.borrow().arrived(res, open_after);
                }
            },
        );
    }

    fn discardPlan(&self, plan_id: i32) {
        let id = u64::try_from(plan_id).unwrap_or(0);
        self.pending.discard(id);
        self.state.borrow_mut().open_after_plan.remove(&id);
    }

    // ------------------------------------------------------ conflicts

    fn conflictsJson(&self, plan_id: i32) -> QString {
        let Some(core) = core() else {
            return QString::from("[]");
        };
        let id = u64::try_from(plan_id).unwrap_or(0);
        let list: Vec<ConflictEntry> = self
            .pending
            .with(id, |plan| core.unresolved_conflicts(plan))
            .unwrap_or_default();
        QString::from(to_json(&list).as_str())
    }

    fn resolveConflict(&self, plan_id: i32, index: i32, choice: QString, apply_to_all: bool) {
        let id = u64::try_from(plan_id).unwrap_or(0);
        let (Some(core), Some(choice), Ok(index)) =
            (core(), parse_choice(&choice.to_string()), usize::try_from(index))
        else {
            return self.emit_failed(&bad_arguments());
        };
        let Some(mut plan) = self.pending.take(id) else {
            return self.emit_failed(&Error::kind(ErrorKind::NotFound));
        };
        let me = QPointer::from(self);
        spawn_then(
            async move {
                let res = core
                    .resolve_plan_conflict(&mut plan, index, choice, apply_to_all)
                    .await;
                let remaining = core.unresolved_conflicts(&plan).len();
                (plan, res, remaining)
            },
            move |(plan, res, remaining)| {
                let Some(p) = me.as_pinned() else { return };
                let this = p.borrow();
                this.pending.put_back(id, plan);
                match res {
                    Ok(_) => this.conflictResolved(plan_id, i32::try_from(remaining).unwrap_or(i32::MAX)),
                    Err(e) => this.emit_failed(&e),
                }
            },
        );
    }

    // ------------------------------------------------------ delete

    fn remove(&self, uris_json: QString) {
        let (Some(core), Some(items)) = (core(), parse_uris(&uris_json)) else {
            return self.emit_failed(&bad_arguments());
        };
        let me = QPointer::from(self);
        spawn_then(
            async move {
                let before = trash_ids(&core);
                let res = core.delete(&items).await;
                let after = trash_ids(&core);
                let trashed = if after == before { 0 } else { after.len() };
                (res, trashed)
            },
            move |(res, trashed)| {
                let Some(p) = me.as_pinned() else { return };
                let this = p.borrow();
                match res {
                    Ok(transfer) => {
                        this.removed(i32::try_from(trashed).unwrap_or(i32::MAX), transfer.unwrap_or(-1))
                    }
                    Err(e) => this.emit_failed(&e),
                }
            },
        );
    }

    // ------------------------------------------------------ compress

    fn compress(&self, uris_json: QString, dest_uri: QString, name: QString, kind: QString) {
        let (Some(core), Some(sources), Some(dest), Some(kind)) = (
            core(),
            parse_uris(&uris_json),
            parse_uri(&dest_uri),
            ArchiveKind::parse(&kind.to_string()),
        ) else {
            return self.emit_failed(&bad_arguments());
        };
        let job = self.next_job.get() + 1;
        self.next_job.set(job);
        let cancel = Arc::new(AtomicBool::new(false));
        self.state
            .borrow_mut()
            .jobs
            .insert(i64::from(job), cancel.clone());
        self.change_jobs(1);
        self.jobStarted(job);
        let me = QPointer::from(self);
        let progress = self.progress_sink(job);
        let name = name.to_string();
        spawn_then(
            async move {
                core.compress_to(&sources, &dest, &name, kind, progress, cancel)
                    .await
            },
            move |res| {
                let Some(p) = me.as_pinned() else { return };
                let this = p.borrow();
                this.state.borrow_mut().jobs.remove(&i64::from(job));
                this.change_jobs(-1);
                match res {
                    Ok(uri) => this.jobFinished(job, true, QString::from(uri.to_string().as_str())),
                    Err(e) => {
                        this.jobFinished(job, false, QString::default());
                        if e.kind != ErrorKind::Canceled {
                            this.emit_failed(&e);
                        }
                    }
                }
            },
        );
    }

    /// Progress as signals on the GUI thread, at most a few times a second.
    fn progress_sink(&self, job: i32) -> lautta_core::provider::ProgressSink {
        let me = QPointer::from(self);
        let deliver = qmetaobject::queued_callback(move |(done, total): (i64, i64)| {
            if let Some(p) = me.as_pinned() {
                p.borrow().jobProgress(job, done, total);
            }
        });
        let started = Instant::now();
        let last = AtomicU64::new(0);
        Arc::new(move |done, total| {
            let now = u64::try_from(started.elapsed().as_millis()).unwrap_or(u64::MAX);
            let interval = u64::try_from(PROGRESS_INTERVAL.as_millis()).unwrap_or(250);
            if now.saturating_sub(last.load(Ordering::Relaxed)) < interval {
                return;
            }
            last.store(now, Ordering::Relaxed);
            deliver((
                i64::try_from(done).unwrap_or(i64::MAX),
                total.and_then(|t| i64::try_from(t).ok()).unwrap_or(-1),
            ));
        })
    }

    fn cancelJob(&self, job_id: i32) {
        if let Some(flag) = self.state.borrow().jobs.get(&i64::from(job_id)) {
            flag.store(true, Ordering::Relaxed);
        }
    }

    fn measure(&self, uris_json: QString) {
        let (Some(core), Some(sources)) = (core(), parse_uris(&uris_json)) else {
            return self.emit_failed(&bad_arguments());
        };
        let me = QPointer::from(self);
        spawn_then(async move { core.measure(&sources).await }, move |res| {
            let Some(p) = me.as_pinned() else { return };
            let this = p.borrow();
            match res {
                Ok(m) => this.measured(QString::from(to_json(&m).as_str())),
                Err(e) => this.emit_failed(&e),
            }
        });
    }

    // ------------------------------------------------------ archives

    fn extract(&self, archive_uri: QString, dest_uri: QString) {
        self.extract_op(&archive_uri, &dest_uri, false);
    }

    fn extractAndOpen(&self, archive_uri: QString, dest_uri: QString) {
        self.extract_op(&archive_uri, &dest_uri, true);
    }

    fn extract_op(&self, archive_uri: &QString, dest_uri: &QString, open: bool) {
        let (Some(core), Some(archive), Some(dest)) = (core(), parse_uri(archive_uri), parse_uri(dest_uri))
        else {
            return self.emit_failed(&bad_arguments());
        };
        let open_after = open.then(|| dest.to_string());
        let me = QPointer::from(self);
        spawn_then(
            async move { launch(&core, core.extract(&archive, &dest).await).await },
            move |res| {
                if let Some(p) = me.as_pinned() {
                    p.borrow().arrived(res, open_after);
                }
            },
        );
    }

    /// Listens to the engine once, to tell when an extract finished.
    fn listen(&self) {
        let Some(core) = core() else { return };
        if std::mem::replace(&mut self.state.borrow_mut().listening, true) {
            return;
        }
        let me = QPointer::from(self);
        let deliver = qmetaobject::queued_callback(move |(id, ok): (i64, bool)| {
            if let Some(p) = me.as_pinned() {
                p.borrow().transfer_finished(id, ok);
            }
        });
        let mut events = core.engine.subscribe();
        handle().spawn(async move {
            use tokio::sync::broadcast::error::RecvError;
            loop {
                match events.recv().await {
                    Ok(TransferEvent::Finished(id)) => {
                        let ok = core
                            .engine
                            .get(id)
                            .is_some_and(|s| s.state == TransferState::Completed);
                        deliver((id, ok));
                    }
                    Ok(_) | Err(RecvError::Lagged(_)) => {}
                    Err(RecvError::Closed) => break,
                }
            }
        });
    }

    fn transfer_finished(&self, id: i64, ok: bool) {
        let dest = self.state.borrow_mut().open_after.remove(&id);
        if let Some(dest) = dest {
            self.extractFinished(id, ok, QString::from(dest.as_str()));
        }
    }

    fn inspectArchive(&self, archive_uri: QString) {
        let (Some(core), Some(uri)) = (core(), parse_uri(&archive_uri)) else {
            return self.emit_failed(&bad_arguments());
        };
        let me = QPointer::from(self);
        spawn_then(async move { core.archive_info(&uri).await }, move |res| {
            let Some(p) = me.as_pinned() else { return };
            let this = p.borrow();
            match res {
                Ok(info) => this.archiveInspected(archive_uri, QString::from(to_json(&info).as_str())),
                Err(e) => this.emit_failed(&e),
            }
        });
    }

    fn openArchive(&self, uri: QString) {
        let (Some(core), Some(archive)) = (core(), parse_uri(&uri)) else {
            return self.emit_failed(&bad_arguments());
        };
        let me = QPointer::from(self);
        spawn_then(async move { core.open_archive(&archive).await }, move |res| {
            let Some(p) = me.as_pinned() else { return };
            let this = p.borrow();
            match res {
                Ok(location) => {
                    let root = Uri::root(location).to_string();
                    this.archiveOpened(uri, QString::from(root.as_str()));
                }
                Err(e) => this.emit_failed(&e),
            }
        });
    }

    /// Whether the URI lies inside an opened archive (LOC-5).
    fn isArchive(&self, uri: QString) -> bool {
        self.archive_location(&uri).is_some()
    }

    /// The archive file a location inside an archive was opened from.
    fn archiveSource(&self, uri: QString) -> QString {
        self.archive_location(&uri)
            .and_then(|l| l.url)
            .map(|u| QString::from(u.as_str()))
            .unwrap_or_default()
    }

    fn archive_location(&self, uri: &QString) -> Option<lautta_core::locations::Location> {
        let location = core()?.location(&parse_uri(uri)?.location)?;
        matches!(location.kind, lautta_core::locations::LocationKind::Archive).then_some(location)
    }

    // ------------------------------------------------------ helpers for pages

    fn displayPath(&self, uri: QString) -> QString {
        match (core(), parse_uri(&uri)) {
            (Some(core), Some(u)) => QString::from(core.display_path(&u).as_str()),
            _ => QString::default(),
        }
    }

    /// Which of the received files Lautta can read (INT-1); the paths may be
    /// `file://` URLs.
    fn probeShared(&self, paths_json: QString) -> QString {
        let paths: Vec<String> = from_json(&paths_json.to_string()).unwrap_or_default();
        let paths: Vec<std::path::PathBuf> = paths.iter().map(|p| local_path(p)).collect();
        let files = core().map(|c| c.probe_shared(&paths)).unwrap_or_default();
        QString::from(to_json(&files).as_str())
    }

    fn destinationsJson(&self) -> QString {
        let list = core().map(|c| c.destinations()).unwrap_or_default();
        QString::from(to_json(&list).as_str())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn plan_ids_fit_signals() {
        assert_eq!(plan_id(3), 3);
        assert_eq!(plan_id(u64::MAX), i32::MAX);
    }

    #[test]
    fn local_paths_from_urls_and_plain_paths() {
        assert_eq!(
            local_path("file:///home/a%20b/c.jpg"),
            std::path::PathBuf::from("/home/a b/c.jpg")
        );
        assert_eq!(local_path("/home/x%20y"), std::path::PathBuf::from("/home/x%20y"));
        assert_eq!(local_path("file:///100%"), std::path::PathBuf::from("/100%"));
        assert_eq!(local_path("file:///caf%C3%A9"), std::path::PathBuf::from("/café"));
    }
}
