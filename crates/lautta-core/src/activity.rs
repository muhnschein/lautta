// SPDX-License-Identifier: LGPL-2.1-or-later
//! Activity bar state. Qt-free; `Transfers` feeds it queue and finish events.

use crate::transfer::{TransferId, TransferState, TransferSummary, WaitReason};

pub const FLASH_MS: i64 = 4000;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BarMode {
    Hidden,
    Question,
    Running,
    Waiting,
    Paused,
    Failed,
    Done,
}

impl BarMode {
    pub fn name(self) -> &'static str {
        match self {
            BarMode::Hidden => "hidden",
            BarMode::Question => "question",
            BarMode::Running => "running",
            BarMode::Waiting => "waiting",
            BarMode::Paused => "paused",
            BarMode::Failed => "failed",
            BarMode::Done => "done",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Task {
    pub id: TransferId,
    pub state: TransferState,
    pub position: i64,
}

impl From<&TransferSummary> for Task {
    fn from(s: &TransferSummary) -> Self {
        Task {
            id: s.id,
            state: s.state,
            position: s.position,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct BarView {
    pub mode: BarMode,
    pub lead: Option<TransferId>,
    pub unfinished: usize,
}

impl BarView {
    const HIDDEN: BarView = BarView {
        mode: BarMode::Hidden,
        lead: None,
        unfinished: 0,
    };
}

#[derive(Debug, Default)]
pub struct ActivityBar {
    failed: Option<TransferId>,
    done: Option<(TransferId, i64)>,
    hidden: Option<Vec<(TransferId, TransferState)>>,
}

fn urgency(state: TransferState) -> u8 {
    match state {
        TransferState::Waiting(WaitReason::Question) => 0,
        TransferState::Running | TransferState::Scanning => 1,
        TransferState::Queued => 2,
        TransferState::Waiting(_) => 3,
        _ => 4,
    }
}

fn mode_of(state: TransferState) -> BarMode {
    match state {
        TransferState::Waiting(WaitReason::Question) => BarMode::Question,
        TransferState::Waiting(_) => BarMode::Waiting,
        TransferState::Paused => BarMode::Paused,
        _ => BarMode::Running,
    }
}

fn snapshot(unfinished: &[Task]) -> Vec<(TransferId, TransferState)> {
    let mut s: Vec<_> = unfinished.iter().map(|t| (t.id, t.state)).collect();
    s.sort_by_key(|(id, _)| *id);
    s
}

impl ActivityBar {
    pub fn finished(&mut self, id: TransferId, state: TransferState, now_ms: i64) {
        match state {
            TransferState::Completed => self.done = Some((id, now_ms)),
            TransferState::Failed => self.failed = Some(id),
            _ => return,
        }
        self.hidden = None;
    }

    pub fn dismiss(&mut self, tasks: &[Task]) {
        let unfinished: Vec<Task> = tasks.iter().copied().filter(|t| !t.state.is_finished()).collect();
        self.hidden = Some(snapshot(&unfinished));
        self.seen();
    }

    pub fn seen(&mut self) {
        self.failed = None;
        self.done = None;
    }

    pub fn view(&mut self, tasks: &[Task], now_ms: i64) -> BarView {
        let unfinished: Vec<Task> = tasks.iter().copied().filter(|t| !t.state.is_finished()).collect();
        let now = snapshot(&unfinished);
        match &self.hidden {
            Some(h) if *h == now => return BarView::HIDDEN,
            Some(_) => self.hidden = None,
            None => {}
        }
        if let Some(lead) = unfinished
            .iter()
            .min_by_key(|t| (urgency(t.state), t.position, t.id))
        {
            return BarView {
                mode: mode_of(lead.state),
                lead: Some(lead.id),
                unfinished: unfinished.len(),
            };
        }
        let listed = |id: TransferId| tasks.iter().any(|t| t.id == id);
        if let Some(id) = self.failed.filter(|id| listed(*id)) {
            return BarView {
                mode: BarMode::Failed,
                lead: Some(id),
                unfinished: 0,
            };
        }
        match self.done {
            Some((id, at)) if listed(id) && now_ms - at < FLASH_MS => BarView {
                mode: BarMode::Done,
                lead: Some(id),
                unfinished: 0,
            },
            _ => BarView::HIDDEN,
        }
    }

    pub fn next_change_in(&self, now_ms: i64) -> Option<i64> {
        self.done
            .map(|(_, at)| at + FLASH_MS - now_ms)
            .filter(|ms| *ms > 0)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use TransferState::*;

    fn t(id: TransferId, state: TransferState, position: i64) -> Task {
        Task { id, state, position }
    }

    fn view(bar: &mut ActivityBar, tasks: &[Task], now: i64) -> (BarMode, Option<TransferId>, usize) {
        let v = bar.view(tasks, now);
        (v.mode, v.lead, v.unfinished)
    }

    #[test]
    fn nothing_to_show_is_hidden() {
        let mut bar = ActivityBar::default();
        assert_eq!(view(&mut bar, &[], 0), (BarMode::Hidden, None, 0));
        assert_eq!(
            view(&mut bar, &[t(1, Completed, 0), t(2, Canceled, 1)], 0),
            (BarMode::Hidden, None, 0)
        );
    }

    #[test]
    fn mode_names() {
        let names: Vec<&str> = [
            BarMode::Hidden,
            BarMode::Question,
            BarMode::Running,
            BarMode::Waiting,
            BarMode::Paused,
            BarMode::Failed,
            BarMode::Done,
        ]
        .iter()
        .map(|m| m.name())
        .collect();
        assert_eq!(
            names,
            ["hidden", "question", "running", "waiting", "paused", "failed", "done"]
        );
    }

    #[test]
    fn each_unfinished_state_has_its_mode() {
        let cases = [
            (Queued, BarMode::Running),
            (Scanning, BarMode::Running),
            (Running, BarMode::Running),
            (Paused, BarMode::Paused),
            (Waiting(WaitReason::Network), BarMode::Waiting),
            (Waiting(WaitReason::Bridge), BarMode::Waiting),
            (Waiting(WaitReason::Volume), BarMode::Waiting),
            (Waiting(WaitReason::Question), BarMode::Question),
        ];
        for (state, mode) in cases {
            let mut bar = ActivityBar::default();
            assert_eq!(
                view(&mut bar, &[t(7, state, 0)], 0),
                (mode, Some(7), 1),
                "{state:?}"
            );
        }
    }

    #[test]
    fn a_question_leads_then_running_then_queue_order() {
        let mut bar = ActivityBar::default();
        let tasks = [
            t(1, Paused, 0),
            t(2, Waiting(WaitReason::Network), 1),
            t(3, Queued, 2),
            t(4, Running, 5),
            t(5, Running, 3),
            t(6, Waiting(WaitReason::Question), 9),
            t(9, Completed, 0),
        ];
        assert_eq!(view(&mut bar, &tasks, 0), (BarMode::Question, Some(6), 6));
        assert_eq!(view(&mut bar, &tasks[..5], 0), (BarMode::Running, Some(5), 5));
        assert_eq!(view(&mut bar, &tasks[..3], 0), (BarMode::Running, Some(3), 3));
        assert_eq!(view(&mut bar, &tasks[..2], 0), (BarMode::Waiting, Some(2), 2));
        assert_eq!(
            view(&mut bar, &[t(8, Scanning, 4), t(4, Running, 4)], 0),
            (BarMode::Running, Some(4), 2)
        );
    }

    #[test]
    fn success_flashes_then_hides() {
        let mut bar = ActivityBar::default();
        let tasks = [t(3, Completed, 0)];
        bar.finished(3, Completed, 1000);
        assert_eq!(view(&mut bar, &tasks, 1000), (BarMode::Done, Some(3), 0));
        assert_eq!(bar.next_change_in(1000), Some(FLASH_MS));
        assert_eq!(
            view(&mut bar, &tasks, 1000 + FLASH_MS - 1),
            (BarMode::Done, Some(3), 0)
        );
        assert_eq!(bar.next_change_in(1000 + FLASH_MS - 1), Some(1));
        assert_eq!(
            view(&mut bar, &tasks, 1000 + FLASH_MS),
            (BarMode::Hidden, None, 0)
        );
        assert_eq!(bar.next_change_in(1000 + FLASH_MS), None);
    }

    #[test]
    fn failure_stays_until_seen() {
        let mut bar = ActivityBar::default();
        let tasks = [t(4, Failed, 0)];
        bar.finished(4, Failed, 0);
        assert_eq!(
            view(&mut bar, &tasks, 10 * FLASH_MS),
            (BarMode::Failed, Some(4), 0)
        );
        assert_eq!(bar.next_change_in(0), None);
        bar.seen();
        assert_eq!(view(&mut bar, &tasks, 0), (BarMode::Hidden, None, 0));
    }

    #[test]
    fn a_later_success_does_not_hide_a_failure() {
        let mut bar = ActivityBar::default();
        bar.finished(1, Failed, 0);
        bar.finished(2, Completed, 10);
        assert_eq!(
            view(&mut bar, &[t(1, Failed, 0), t(2, Completed, 1)], 20),
            (BarMode::Failed, Some(1), 0)
        );
    }

    #[test]
    fn running_work_comes_before_an_outcome() {
        let mut bar = ActivityBar::default();
        bar.finished(1, Failed, 0);
        let tasks = [t(1, Failed, 0), t(2, Running, 1)];
        assert_eq!(view(&mut bar, &tasks, 0), (BarMode::Running, Some(2), 1));
        assert_eq!(
            view(&mut bar, &[t(1, Failed, 0), t(2, Completed, 1)], 0),
            (BarMode::Failed, Some(1), 0)
        );
    }

    #[test]
    fn canceled_shows_nothing() {
        let mut bar = ActivityBar::default();
        bar.finished(5, Canceled, 0);
        assert_eq!(
            view(&mut bar, &[t(5, Canceled, 0)], 0),
            (BarMode::Hidden, None, 0)
        );
    }

    #[test]
    fn cleared_history_takes_the_outcome_away() {
        let mut bar = ActivityBar::default();
        bar.finished(1, Failed, 0);
        bar.finished(2, Completed, 0);
        assert_eq!(view(&mut bar, &[], 0), (BarMode::Hidden, None, 0));
        assert_eq!(
            view(&mut bar, &[t(2, Completed, 0)], 0),
            (BarMode::Done, Some(2), 0)
        );
    }

    #[test]
    fn swipe_hides_until_the_tasks_change() {
        let mut bar = ActivityBar::default();
        let running = [t(1, Running, 0), t(2, Queued, 1)];
        bar.dismiss(&running);
        assert_eq!(view(&mut bar, &running, 0), (BarMode::Hidden, None, 0));
        assert_eq!(view(&mut bar, &running, 500), (BarMode::Hidden, None, 0));
        let more = [t(1, Running, 0), t(2, Queued, 1), t(3, Queued, 2)];
        assert_eq!(view(&mut bar, &more, 0), (BarMode::Running, Some(1), 3));
        assert_eq!(view(&mut bar, &running, 0), (BarMode::Running, Some(1), 2));
    }

    #[test]
    fn swipe_hides_until_a_state_changes() {
        let mut bar = ActivityBar::default();
        bar.dismiss(&[t(1, Running, 0)]);
        assert_eq!(
            view(&mut bar, &[t(1, Waiting(WaitReason::Question), 0)], 0),
            (BarMode::Question, Some(1), 1)
        );
    }

    #[test]
    fn swiping_an_outcome_counts_as_seen() {
        let mut bar = ActivityBar::default();
        let tasks = [t(1, Failed, 0)];
        bar.finished(1, Failed, 0);
        bar.dismiss(&tasks);
        assert_eq!(view(&mut bar, &tasks, 0), (BarMode::Hidden, None, 0));
        assert_eq!(
            view(&mut bar, &[t(1, Failed, 0), t(2, Running, 1)], 0),
            (BarMode::Running, Some(2), 1)
        );
        bar.finished(2, Completed, 0);
        assert_eq!(
            view(&mut bar, &[t(1, Failed, 0), t(2, Completed, 1)], 0),
            (BarMode::Done, Some(2), 0)
        );
    }

    #[test]
    fn an_end_after_a_swipe_shows_without_a_view_between() {
        let mut bar = ActivityBar::default();
        bar.dismiss(&[]);
        bar.finished(1, Failed, 0);
        assert_eq!(
            view(&mut bar, &[t(1, Failed, 0)], 0),
            (BarMode::Failed, Some(1), 0)
        );
    }

    #[test]
    fn task_from_summary() {
        use crate::ops::OperationKind;
        use crate::transfer::TransferOptions;
        use crate::uri::Uri;
        let s = TransferSummary {
            id: 9,
            kind: OperationKind::Delete,
            title: "x".into(),
            state: Paused,
            dest: Uri::root("a"),
            position: 4,
            created_ms: 0,
            finished_ms: None,
            bytes_total: 0,
            bytes_done: 0,
            items_total: 1,
            items_done: 0,
            items_failed: 0,
            error: None,
            options: TransferOptions::default(),
        };
        assert_eq!(Task::from(&s), t(9, Paused, 4));
    }
}
