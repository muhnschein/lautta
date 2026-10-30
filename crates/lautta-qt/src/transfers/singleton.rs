// SPDX-License-Identifier: LGPL-2.1-or-later

use super::events;
use crate::json::to_json;
use crate::runtime::{core, spawn_then};
use lautta_core::activity::{ActivityBar, BarMode, BarView, Task};
use lautta_core::app::Started;
use lautta_core::app_transfers::{Aggregate, ProgressBook, StartReport};
use lautta_core::ops::OperationKind;
use lautta_core::transfer::{Clock, SystemClock, TransferEvent, TransferState};
use lautta_core::{Error, Result, Uri};
use qmetaobject::prelude::*;
use qmetaobject::{QPointer, QSingletonInit};
use std::collections::HashSet;
use std::time::Duration;

#[derive(QObject, Default)]
pub struct Transfers {
    base: qt_base_class!(trait QObject),
    activeCount: qt_property!(i32; NOTIFY changed),
    pendingCount: qt_property!(i32; NOTIFY changed),
    pausedCount: qt_property!(i32; NOTIFY changed),
    waitingCount: qt_property!(i32; NOTIFY changed),
    questionCount: qt_property!(i32; NOTIFY changed),
    busy: qt_property!(bool; NOTIFY changed),
    bytesDone: qt_property!(i64; NOTIFY changed),
    bytesTotal: qt_property!(i64; NOTIFY changed),
    rate: qt_property!(i64; NOTIFY changed),
    /// -1 when unknown.
    eta: qt_property!(i64; NOTIFY changed),
    /// Came back paused at start, until resumed or dismissed.
    pendingAtStart: qt_property!(i32; NOTIFY changed),
    closing: qt_property!(bool; NOTIFY changed),
    /// A `BarMode` name, the transfer it names (0 when hidden), count of unfinished transfers.
    barMode: qt_property!(QString; NOTIFY barChanged),
    barTransferId: qt_property!(i64; NOTIFY barChanged),
    barUnfinished: qt_property!(i32; NOTIFY barChanged),
    changed: qt_signal!(),
    barChanged: qt_signal!(),

    finished: qt_signal!(id: i64, ok: bool, failures: i32),
    needsAnswer: qt_signal!(id: i64, item: i32, conflictJson: QString),
    /// `kind` is an ErrorKind name.
    failed: qt_signal!(kind: QString, message: QString),
    queued: qt_signal!(id: i64),
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
    dismissBar: qt_method!(fn(&mut self)),
    barSeen: qt_method!(fn(&mut self)),

    book: ProgressBook,
    bar: ActivityBar,
    flash_timer: bool,
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
        self.barMode = QString::from(BarMode::Hidden.name());
        self.recompute();
        // Window loads persisted settings when its creation is done: start one event-loop turn
        // later.
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
        let tasks: Vec<Task> = list.iter().map(Task::from).collect();
        self.refresh_bar(&tasks);
    }

    fn refresh_bar(&mut self, tasks: &[Task]) {
        let now = SystemClock.now_ms();
        let view = self.bar.view(tasks, now);
        if self.apply_bar(view) {
            self.barChanged();
        }
        if let Some(ms) = self.bar.next_change_in(now) {
            self.schedule_flash_end(ms);
        }
    }

    fn apply_bar(&mut self, v: BarView) -> bool {
        let next = (
            QString::from(v.mode.name()),
            v.lead.unwrap_or(0),
            i32::try_from(v.unfinished).unwrap_or(i32::MAX),
        );
        if (self.barMode.clone(), self.barTransferId, self.barUnfinished) == next {
            return false;
        }
        (self.barMode, self.barTransferId, self.barUnfinished) = next;
        true
    }

    fn schedule_flash_end(&mut self, ms: i64) {
        if self.flash_timer {
            return;
        }
        self.flash_timer = true;
        let me = QPointer::from(&*self);
        let wait = Duration::from_millis(u64::try_from(ms).unwrap_or(0) + 50);
        qmetaobject::single_shot(wait, move || {
            if let Some(p) = me.as_pinned() {
                let mut t = p.borrow_mut();
                t.flash_timer = false;
                t.recompute();
            }
        });
    }

    fn tasks() -> Vec<Task> {
        core().map_or_else(Vec::new, |c| c.engine.list().iter().map(Task::from).collect())
    }

    fn dismissBar(&mut self) {
        let tasks = Self::tasks();
        self.bar.dismiss(&tasks);
        self.refresh_bar(&tasks);
    }

    fn barSeen(&mut self) {
        self.bar.seen();
        self.refresh_bar(&Self::tasks());
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

    fn notify_finished(&mut self, id: i64) {
        let Some(s) = core().and_then(|c| c.engine.get(id)) else {
            return;
        };
        self.bar.finished(id, s.state, SystemClock.now_ms());
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

    /// Engine loads with auto-resume when the setting is on and the bridge is reachable.
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

    fn questionsJson(&self, id: i64) -> QString {
        let list = core().and_then(|c| c.question_list(id).ok()).unwrap_or_default();
        QString::from(to_json(&list).as_str())
    }

    fn clearHistory(&self) -> i32 {
        let n = core().map_or(0, |c| c.clear_history());
        events::reload();
        i32::try_from(n).unwrap_or(i32::MAX)
    }

    fn dismissRestored(&mut self) {
        if self.pendingAtStart != 0 {
            self.pendingAtStart = 0;
            self.changed();
        }
    }

    fn noteClosing(&mut self) {
        if !self.closing {
            self.closing = true;
            self.changed();
        }
    }

    fn requestShow(&self) {
        self.showRequested();
    }

    /// Asks nothing (large plans start at once). Serves tests and the cover; browse and operations
    /// have their own entry points.
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

    fn summaryJson(&self, id: i64) -> QString {
        let text = core().and_then(|c| super::items::summary_json(&c, id, &self.book));
        QString::from(text.unwrap_or_default().as_str())
    }
}
