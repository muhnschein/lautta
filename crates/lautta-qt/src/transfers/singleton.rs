// SPDX-License-Identifier: LGPL-2.1-or-later
//! The `Transfers` singleton (doc/QML-API.md): queue summary and control,
//! cover data, keep-alive state, start-up restore (XFR-11) and the watching
//! of working copies while the app runs (EDT-1..3).

use super::edited::conflict_text;
use super::events;
use crate::json::to_json;
use crate::runtime::{core, handle, spawn_then};
use lautta_core::app::Started;
use lautta_core::app_transfers::{
    copy_for_path, sync_watches, Aggregate, ProgressBook, StartReport, WriteBack,
};
use lautta_core::ops::OperationKind;
use lautta_core::transfer::{TransferEvent, TransferState};
use lautta_core::watch::{FileEvent, FileWatcher, FILE_DEBOUNCE};
use lautta_core::workcopy::Purpose;
use lautta_core::{Error, Result, Uri};
use qmetaobject::prelude::*;
use qmetaobject::{QPointer, QSingletonInit};
use std::collections::HashSet;
use std::path::PathBuf;
use std::time::Duration;

/// How often the set of watched working copies is brought up to date, so
/// copies made by other pages are watched without them telling us (EDT-1).
const WATCH_SYNC: Duration = Duration::from_secs(5);

#[derive(QObject, Default)]
pub struct Transfers {
    base: qt_base_class!(trait QObject),
    /// Queued, scanning or running transfers.
    activeCount: qt_property!(i32; NOTIFY changed),
    /// Everything not finished: active, waiting and paused.
    pendingCount: qt_property!(i32; NOTIFY changed),
    pausedCount: qt_property!(i32; NOTIFY changed),
    waitingCount: qt_property!(i32; NOTIFY changed),
    /// Transfers that wait for a conflict answer.
    questionCount: qt_property!(i32; NOTIFY changed),
    /// Drives `KeepAlive` (XFR-5).
    busy: qt_property!(bool; NOTIFY changed),
    bytesDone: qt_property!(i64; NOTIFY changed),
    bytesTotal: qt_property!(i64; NOTIFY changed),
    /// Bytes per second over the active transfers.
    rate: qt_property!(i64; NOTIFY changed),
    /// Seconds left, -1 when unknown.
    eta: qt_property!(i64; NOTIFY changed),
    /// Transfers that came back paused at start (XFR-11), until resumed or
    /// dismissed.
    pendingAtStart: qt_property!(i32; NOTIFY changed),
    /// The app is closing with work pending (XFR-6).
    closing: qt_property!(bool; NOTIFY changed),
    changed: qt_signal!(),

    finished: qt_signal!(id: i64, ok: bool, failures: i32),
    needsAnswer: qt_signal!(id: i64, item: i32, conflictJson: QString),
    /// A control call failed (`kind` is an ErrorKind name).
    failed: qt_signal!(kind: QString, message: QString),
    /// A queued transfer got its id (`queueCopy`).
    queued: qt_signal!(id: i64),
    /// An edited file was uploaded after its change was noticed.
    editUploaded: qt_signal!(copyId: i64, name: QString),
    /// The remote changed meanwhile; the user must choose (EDT-2).
    editConflict: qt_signal!(copyId: i64, name: QString, conflictJson: QString),
    /// Answer to `resolveEditConflict`: `result` is `uploaded`, `savedCopy`
    /// (`detail` is the copy's URI) or `discarded`.
    editResolved: qt_signal!(copyId: i64, result: QString, detail: QString),
    /// Someone (the cover) asks to show the Transfers page.
    showRequested: qt_signal!(),

    start: qt_method!(fn(&mut self)),
    setBridgeReachable: qt_method!(fn(&mut self, up: bool)),
    pauseAll: qt_method!(fn(&mut self)),
    resumeAll: qt_method!(fn(&mut self)),
    pause: qt_method!(fn(&self, id: i64) -> bool),
    resume: qt_method!(fn(&mut self, id: i64) -> bool),
    cancel: qt_method!(fn(&self, id: i64) -> bool),
    retryFailed: qt_method!(fn(&self, id: i64) -> bool),
    moveUp: qt_method!(fn(&self, id: i64) -> bool),
    moveDown: qt_method!(fn(&self, id: i64) -> bool),
    moveToTop: qt_method!(fn(&self, id: i64) -> bool),
    answer: qt_method!(fn(&self, id: i64, item: i32, choice: QString, apply_to_all: bool) -> bool),
    questionsJson: qt_method!(fn(&self, id: i64) -> QString),
    clearHistory: qt_method!(fn(&self) -> i32),
    dismissRestored: qt_method!(fn(&mut self)),
    noteClosing: qt_method!(fn(&mut self)),
    requestShow: qt_method!(fn(&self)),
    queueCopy: qt_method!(fn(&self, uris_json: QString, dest: QString)),
    watchWorkingCopies: qt_method!(fn(&self)),
    resolveEditConflict: qt_method!(fn(&self, copy_id: i64, choice: QString)),
    summaryJson: qt_method!(fn(&self, id: i64) -> QString),

    book: ProgressBook,
    notified: HashSet<i64>,
    started: bool,
    bridge_reachable: Option<bool>,
    watcher: Option<FileWatcher>,
}

impl QSingletonInit for Transfers {
    fn init(&mut self) {
        events::ensure_forwarder();
        let me = QPointer::from(&*self);
        events::listen(move |ev| match me.as_pinned() {
            Some(p) => {
                p.borrow_mut().on_event(ev);
                true
            }
            None => false,
        });
        self.recompute();
        // The window loads the persisted settings when its creation is done;
        // starting one event-loop turn later sees them (XFR-11).
        let me = QPointer::from(&*self);
        qmetaobject::single_shot(Duration::from_millis(0), move || {
            if let Some(p) = me.as_pinned() {
                p.borrow_mut().start();
            }
        });
    }
}

fn to_i64(v: u64) -> i64 {
    i64::try_from(v).unwrap_or(i64::MAX)
}

impl Transfers {
    fn on_event(&mut self, ev: &TransferEvent) {
        match ev {
            TransferEvent::Progress {
                id,
                bytes_done,
                rate,
                eta_secs,
            } => self.book.update(*id, *bytes_done, *rate, *eta_secs),
            TransferEvent::Finished(id) => self.notify_finished(*id),
            TransferEvent::NeedsAnswer { id, item } => {
                self.notify_question(*id, i32::try_from(*item).unwrap_or(i32::MAX))
            }
            _ => {}
        }
        self.recompute();
    }

    /// Re-reads the queue and updates the summary properties; `changed` is
    /// emitted once when anything differs.
    fn recompute(&mut self) {
        let Some(core) = core() else { return };
        let list = core.engine.list();
        let ids: HashSet<i64> = list.iter().map(|s| s.id).collect();
        self.book.retain(&ids);
        self.notified.retain(|id| ids.contains(id));
        let agg = self.book.aggregate(&list);
        let questions = list
            .iter()
            .filter(|s| s.state == TransferState::Waiting(lautta_core::transfer::WaitReason::Question))
            .count();
        let busy = core.engine.is_busy();
        if self.apply_summary(&agg, questions, busy) {
            self.changed();
        }
    }

    fn apply_summary(&mut self, a: &Aggregate, questions: usize, busy: bool) -> bool {
        let next = (
            a.active as i32,
            a.unfinished as i32,
            a.paused as i32,
            a.waiting as i32,
            questions as i32,
            busy,
            to_i64(a.bytes_done),
            to_i64(a.bytes_total),
            to_i64(a.rate),
            a.eta_secs.map_or(-1, to_i64),
        );
        let now = (
            self.activeCount,
            self.pendingCount,
            self.pausedCount,
            self.waitingCount,
            self.questionCount,
            self.busy,
            self.bytesDone,
            self.bytesTotal,
            self.rate,
            self.eta,
        );
        if now == next {
            return false;
        }
        (
            self.activeCount,
            self.pendingCount,
            self.pausedCount,
            self.waitingCount,
            self.questionCount,
            self.busy,
            self.bytesDone,
            self.bytesTotal,
            self.rate,
            self.eta,
        ) = next;
        true
    }

    /// Completed and failed transfers are announced once (INT-2); canceled
    /// ones are the user's own doing.
    fn notify_finished(&mut self, id: i64) {
        let Some(s) = core().and_then(|c| c.engine.get(id)) else {
            return;
        };
        if s.state == TransferState::Canceled || !self.notified.insert(id) {
            return;
        }
        let ok = s.state == TransferState::Completed && s.items_failed == 0;
        self.finished(id, ok, i32::try_from(s.items_failed).unwrap_or(i32::MAX));
    }

    fn notify_question(&self, id: i64, item: i32) {
        let Some(core) = core() else { return };
        let Ok(list) = core.question_list(id) else {
            return;
        };
        if let Some(q) = list.iter().find(|q| q["item"] == item) {
            self.needsAnswer(id, item, QString::from(to_json(q).as_str()));
        }
    }

    fn report(&self, res: Result<()>) -> bool {
        match res {
            Ok(()) => true,
            Err(e) => {
                self.failed(QString::from(e.kind.name()), QString::from(e.message.as_str()));
                false
            }
        }
    }

    /// Restores the queue (XFR-11) once: the engine loads with auto-resume
    /// when the setting is on and the bridge is reachable.
    fn start(&mut self) {
        if self.started {
            return;
        }
        let Some(core) = core() else { return };
        self.started = true;
        let auto = core.settings().auto_resume && self.bridge_reachable.unwrap_or(true);
        let me = QPointer::from(&*self);
        spawn_then(async move { core.start_transfers(auto).await }, move |res| {
            if let Some(p) = me.as_pinned() {
                p.borrow_mut().after_start(res);
            }
        });
    }

    fn after_start(&mut self, res: Result<StartReport>) {
        match res {
            Ok(report) => {
                self.pendingAtStart = i32::try_from(report.paused).unwrap_or(i32::MAX);
                self.changed();
            }
            Err(e) => {
                self.failed(QString::from(e.kind.name()), QString::from(e.message.as_str()));
            }
        }
        self.recompute();
        self.start_watching();
        events::reload();
    }

    /// Browse tells whether the bridge is reachable: the engine parks bridge
    /// transfers while it is not (NVB-12) and `start` reads it for
    /// auto-resume.
    fn setBridgeReachable(&mut self, up: bool) {
        self.bridge_reachable = Some(up);
        if let Some(core) = core() {
            core.engine.bridge_available(up);
        }
    }

    fn pauseAll(&mut self) {
        if let Some(core) = core() {
            core.engine.pause_all();
        }
    }

    fn resumeAll(&mut self) {
        if let Some(core) = core() {
            core.engine.resume_all();
        }
        self.dismissRestored();
    }

    fn pause(&self, id: i64) -> bool {
        self.report(core().map_or(Ok(()), |c| c.engine.pause(id)))
    }

    fn resume(&mut self, id: i64) -> bool {
        let ok = self.report(core().map_or(Ok(()), |c| c.engine.resume(id)));
        if ok && self.pendingAtStart > 0 {
            self.pendingAtStart -= 1;
            self.changed();
        }
        ok
    }

    fn cancel(&self, id: i64) -> bool {
        self.report(core().map_or(Ok(()), |c| c.engine.cancel(id)))
    }

    fn retryFailed(&self, id: i64) -> bool {
        self.report(core().map_or(Ok(()), |c| c.engine.retry_failed(id)))
    }

    fn moveUp(&self, id: i64) -> bool {
        core().is_some_and(|c| c.engine.move_up(id))
    }

    fn moveDown(&self, id: i64) -> bool {
        core().is_some_and(|c| c.engine.move_down(id))
    }

    fn moveToTop(&self, id: i64) -> bool {
        core().is_some_and(|c| c.engine.move_to_top(id))
    }

    fn answer(&self, id: i64, item: i32, choice: QString, apply_to_all: bool) -> bool {
        let Some(core) = core() else { return false };
        let Ok(item) = u32::try_from(item) else {
            return self.report(Err(Error::new(
                lautta_core::ErrorKind::InvalidArgument,
                "bad item",
            )));
        };
        self.report(core.answer_question(id, item, &choice.to_string(), apply_to_all))
    }

    /// Open questions of a transfer, a JSON array of conflict objects (the
    /// same shape `needsAnswer` carries).
    fn questionsJson(&self, id: i64) -> QString {
        let list = core().and_then(|c| c.question_list(id).ok()).unwrap_or_default();
        QString::from(to_json(&list).as_str())
    }

    fn clearHistory(&self) -> i32 {
        let n = core().map_or(0, |c| c.clear_history());
        events::reload();
        i32::try_from(n).unwrap_or(i32::MAX)
    }

    /// The restored-transfers banner was answered or dismissed.
    fn dismissRestored(&mut self) {
        if self.pendingAtStart != 0 {
            self.pendingAtStart = 0;
            self.changed();
        }
    }

    /// The app is about to close: the cover and the notification say what
    /// happens to pending work (XFR-6).
    fn noteClosing(&mut self) {
        if !self.closing {
            self.closing = true;
            self.changed();
        }
    }

    fn requestShow(&self) {
        self.showRequested();
    }

    /// Queues a plain copy of `uris_json` into the folder `dest`, asking
    /// nothing (large plans start at once). The browse and operations areas
    /// have their own entry points; this one serves tests and the cover.
    fn queueCopy(&self, uris_json: QString, dest: QString) {
        let Some(core) = core() else { return };
        let uris: Vec<Uri> = crate::json::from_json::<Vec<String>>(&uris_json.to_string())
            .unwrap_or_default()
            .iter()
            .filter_map(|s| Uri::parse(s).ok())
            .collect();
        let Ok(dest) = Uri::parse(&dest.to_string()) else {
            self.report(Err(Error::new(
                lautta_core::ErrorKind::InvalidArgument,
                "bad destination",
            )));
            return;
        };
        let me = QPointer::from(self);
        spawn_then(
            async move {
                match core.copy_or_move(OperationKind::Copy, uris, dest).await? {
                    Started::Transfer(id) => Ok(id),
                    Started::NeedsSummary(plan) => core.start_plan(*plan).await,
                }
            },
            move |res| {
                if let Some(p) = me.as_pinned() {
                    match res {
                        Ok(id) => p.borrow().queued(id),
                        Err(e) => {
                            p.borrow().report(Err(e));
                        }
                    }
                }
            },
        );
    }

    /// One transfer's summary as JSON (title, kind, counters, options), or
    /// an empty string when it is gone.
    fn summaryJson(&self, id: i64) -> QString {
        let text = core().and_then(|c| super::items::summary_json(&c, id, &self.book));
        QString::from(text.unwrap_or_default().as_str())
    }

    // ------------------------------------------------- working copies

    /// Answers an edit conflict (EDT-2); the dialog that asked is gone by
    /// the time the answer is known, so the result comes back here.
    fn resolveEditConflict(&self, copy_id: i64, choice: QString) {
        let Some(core) = core() else { return };
        let Some(choice) = lautta_core::app_transfers::parse_edit_choice(&choice.to_string()) else {
            self.report(Err(Error::new(
                lautta_core::ErrorKind::InvalidArgument,
                "unknown choice",
            )));
            return;
        };
        let me = QPointer::from(self);
        spawn_then(
            async move { core.resolve_edit_conflict(copy_id, choice).await },
            move |res| {
                events::reload();
                let Some(p) = me.as_pinned() else { return };
                let p = p.borrow();
                match res {
                    Ok(r) => {
                        let (what, detail) = match r {
                            lautta_core::workcopy::Resolution::Uploaded(_) => ("uploaded", String::new()),
                            lautta_core::workcopy::Resolution::SavedCopy(u) => ("savedCopy", u.to_string()),
                            lautta_core::workcopy::Resolution::Discarded => ("discarded", String::new()),
                        };
                        p.editResolved(copy_id, QString::from(what), QString::from(detail.as_str()));
                    }
                    Err(e) => {
                        p.report(Err(e));
                    }
                }
            },
        );
    }

    /// Starts the file watcher for working copies (EDT-1/2) and keeps it in
    /// step with the copies.
    fn start_watching(&mut self) {
        if self.watcher.is_some() {
            return;
        }
        let (watcher, mut rx) = match FileWatcher::new(FILE_DEBOUNCE) {
            Ok(w) => w,
            Err(e) => {
                log::warn!("cannot watch working copies: {e}");
                return;
            }
        };
        self.watcher = Some(watcher);
        let me = QPointer::from(&*self);
        let to_gui = qmetaobject::queued_callback(move |path: PathBuf| {
            if let Some(p) = me.as_pinned() {
                p.borrow().on_written(path);
            }
        });
        handle().spawn(async move {
            while let Some(FileEvent::Written(path)) = rx.recv().await {
                to_gui(path);
            }
        });
        self.watchWorkingCopies();
        self.schedule_watch_sync();
    }

    fn schedule_watch_sync(&self) {
        let me = QPointer::from(self);
        qmetaobject::single_shot(WATCH_SYNC, move || {
            if let Some(p) = me.as_pinned() {
                let p = p.borrow();
                p.watchWorkingCopies();
                p.schedule_watch_sync();
            }
        });
    }

    fn watchWorkingCopies(&self) {
        let Some(core) = core() else { return };
        let me = QPointer::from(self);
        spawn_then(
            async move { core.working_copies.list(Some(Purpose::Edit)).await },
            move |res| {
                if let (Some(p), Ok(copies)) = (me.as_pinned(), res) {
                    if let Some(w) = &p.borrow().watcher {
                        sync_watches(w, &copies);
                    }
                }
            },
        );
    }

    /// A working copy was written (close-after-write, 2 s debounce): upload
    /// it or ask (EDT-2).
    fn on_written(&self, path: PathBuf) {
        let Some(core) = core() else { return };
        let me = QPointer::from(self);
        spawn_then(
            async move {
                let copies = core.working_copies.list(Some(Purpose::Edit)).await?;
                let Some(id) = copy_for_path(&copies, &path) else {
                    return Ok(None);
                };
                let name = copies
                    .iter()
                    .find(|c| c.id == id)
                    .map(|c| core.working_copy_name(c))
                    .unwrap_or_default();
                let out = core.write_back(id).await?;
                let json = match &out {
                    WriteBack::Conflict(c) => conflict_text(&core, c),
                    _ => String::new(),
                };
                Ok::<_, Error>(Some((id, name, out, json)))
            },
            move |res| {
                events::reload();
                let Some(p) = me.as_pinned() else { return };
                let p = p.borrow();
                match res {
                    Ok(Some((id, name, WriteBack::Uploaded(_), _))) => {
                        p.editUploaded(id, QString::from(name.as_str()))
                    }
                    Ok(Some((id, name, WriteBack::Conflict(_), json))) => {
                        p.editConflict(id, QString::from(name.as_str()), QString::from(json.as_str()))
                    }
                    Ok(_) => {}
                    Err(e) => {
                        p.report(Err(e));
                    }
                }
            },
        );
    }
}
