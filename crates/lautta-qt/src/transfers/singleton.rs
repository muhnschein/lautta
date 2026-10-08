// SPDX-License-Identifier: LGPL-2.1-or-later
//! The `Transfers` singleton (doc/QML-API.md): queue summary and control,
//! cover data, keep-alive state and start-up restore (XFR-11).

use super::events;
use crate::json::to_json;
use crate::runtime::{core, spawn_then};
use lautta_core::app::Started;
use lautta_core::app_transfers::{Aggregate, ProgressBook, StartReport};
use lautta_core::ops::OperationKind;
use lautta_core::transfer::{TransferEvent, TransferState};
use lautta_core::{Error, Result, Uri};
use qmetaobject::prelude::*;
use qmetaobject::{QPointer, QSingletonInit};
use std::collections::HashSet;
use std::time::Duration;

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
    /// Someone (the cover) asks to show the Transfers page.
    showRequested: qt_signal!(),

    start: qt_method!(fn(&mut self)),
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
    summaryJson: qt_method!(fn(&self, id: i64) -> QString),

    book: ProgressBook,
    notified: HashSet<i64>,
    started: bool,
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
        let auto = core.settings().auto_resume && core.bridge_reachable();
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
        events::reload();
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
}
