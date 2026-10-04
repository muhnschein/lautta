// SPDX-License-Identifier: LGPL-2.1-or-later
//! The transfer model (XFR-1): states with an explicit transition table,
//! items, and the options that travel with a transfer.

use crate::entry::Kind;
use crate::error::{Error, ErrorKind, Result};
use crate::ops::{Conflict, ConflictChoice, OperationKind, Plan, PlanItem};
use crate::uri::Uri;
use serde::{Deserialize, Serialize};

pub type TransferId = i64;

/// Why a transfer is waiting (XFR-1, XFR-7, NVB-12).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum WaitReason {
    Bridge,
    Network,
    /// A conflict needs an answer (OPS-2).
    Question,
    /// A removable volume vanished: "Insert the card to continue".
    Volume,
}

impl WaitReason {
    pub const ALL: [WaitReason; 4] = [
        WaitReason::Bridge,
        WaitReason::Network,
        WaitReason::Question,
        WaitReason::Volume,
    ];

    pub fn name(self) -> &'static str {
        match self {
            WaitReason::Bridge => "bridge",
            WaitReason::Network => "network",
            WaitReason::Question => "question",
            WaitReason::Volume => "volume",
        }
    }

    pub fn from_name(s: &str) -> Option<WaitReason> {
        WaitReason::ALL.into_iter().find(|r| r.name() == s)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum TransferState {
    Queued,
    Scanning,
    Running,
    Paused,
    Waiting(WaitReason),
    /// Ended with at least one failed item or a fatal error. "Completed with
    /// N failures" is this state with `items_done > 0` (XFR-9).
    Failed,
    Completed,
    Canceled,
}

impl TransferState {
    /// Every state, for exhaustive tests.
    pub const ALL: [TransferState; 11] = [
        TransferState::Queued,
        TransferState::Scanning,
        TransferState::Running,
        TransferState::Paused,
        TransferState::Waiting(WaitReason::Bridge),
        TransferState::Waiting(WaitReason::Network),
        TransferState::Waiting(WaitReason::Question),
        TransferState::Waiting(WaitReason::Volume),
        TransferState::Failed,
        TransferState::Completed,
        TransferState::Canceled,
    ];

    /// Completed, failed or canceled: the transfer is history.
    pub fn is_finished(self) -> bool {
        matches!(
            self,
            TransferState::Failed | TransferState::Completed | TransferState::Canceled
        )
    }

    pub fn is_waiting(self) -> bool {
        matches!(self, TransferState::Waiting(_))
    }

    /// Eligible for the scheduler to start items.
    pub fn is_runnable(self) -> bool {
        matches!(
            self,
            TransferState::Queued | TransferState::Scanning | TransferState::Running
        )
    }

    /// The transition table. `Failed` may only go back to `Queued` (retry);
    /// `Completed` and `Canceled` are terminal.
    pub fn can_transition(self, to: TransferState) -> bool {
        use TransferState::*;
        match (self, to) {
            (Queued, Scanning | Running | Paused | Waiting(_) | Canceled) => true,
            (Scanning, Running | Paused | Waiting(_) | Failed | Completed | Canceled) => true,
            (Running, Paused | Waiting(_) | Failed | Completed | Canceled) => true,
            (Paused, Queued | Running | Canceled) => true,
            (Waiting(from), Waiting(next)) => from != next,
            (Waiting(_), Queued | Running | Paused | Failed | Canceled) => true,
            (Failed, Queued) => true,
            _ => false,
        }
    }

    /// Database encoding: state name and waiting reason.
    pub fn to_db(self) -> (&'static str, Option<&'static str>) {
        match self {
            TransferState::Queued => ("queued", None),
            TransferState::Scanning => ("scanning", None),
            TransferState::Running => ("running", None),
            TransferState::Paused => ("paused", None),
            TransferState::Waiting(r) => ("waiting", Some(r.name())),
            TransferState::Failed => ("failed", None),
            TransferState::Completed => ("completed", None),
            TransferState::Canceled => ("canceled", None),
        }
    }

    pub fn from_db(state: &str, reason: Option<&str>) -> Result<TransferState> {
        Ok(match state {
            "queued" => TransferState::Queued,
            "scanning" => TransferState::Scanning,
            "running" => TransferState::Running,
            "paused" => TransferState::Paused,
            "waiting" => TransferState::Waiting(
                reason
                    .and_then(WaitReason::from_name)
                    .unwrap_or(WaitReason::Question),
            ),
            "failed" => TransferState::Failed,
            "completed" => TransferState::Completed,
            "canceled" => TransferState::Canceled,
            other => {
                return Err(Error::new(
                    ErrorKind::Internal,
                    format!("unknown transfer state {other}"),
                ))
            }
        })
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum ItemState {
    Pending,
    Running,
    Done,
    Skipped,
    Failed,
    /// A conflict needs an answer.
    NeedsAnswer,
}

impl ItemState {
    pub fn name(self) -> &'static str {
        match self {
            ItemState::Pending => "pending",
            ItemState::Running => "running",
            ItemState::Done => "done",
            ItemState::Skipped => "skipped",
            ItemState::Failed => "failed",
            ItemState::NeedsAnswer => "needs-answer",
        }
    }

    pub fn from_name(s: &str) -> ItemState {
        [
            ItemState::Pending,
            ItemState::Running,
            ItemState::Done,
            ItemState::Skipped,
            ItemState::Failed,
            ItemState::NeedsAnswer,
        ]
        .into_iter()
        .find(|i| i.name() == s)
        .unwrap_or(ItemState::Pending)
    }

    /// Needs no more work.
    pub fn is_settled(self) -> bool {
        matches!(self, ItemState::Done | ItemState::Skipped | ItemState::Failed)
    }
}

/// Options that travel with a transfer (stored in the `options` column).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct TransferOptions {
    /// "Verify with checksums" (XFR-4); always on for moves between locations.
    pub verify_checksums: bool,
    /// OPS-5: default on.
    pub preserve_mtime: bool,
    /// OPS-5: only when both sides support modes.
    pub preserve_mode: bool,
    /// "Apply to all remaining" answer (OPS-2).
    pub resolve_all: Option<ConflictChoice>,
    /// Deletes go through the caller's [`super::Trasher`] (OPS-8).
    pub trash: bool,
    /// Write-backs run before ordinary transfers (EDT-2).
    pub high_priority: bool,
}

impl Default for TransferOptions {
    fn default() -> Self {
        TransferOptions {
            verify_checksums: false,
            preserve_mtime: true,
            preserve_mode: false,
            resolve_all: None,
            trash: false,
            high_priority: false,
        }
    }
}

/// Per-item data that has no column of its own; stored as JSON in
/// `transfer_items.conflict`.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct ItemExtra {
    pub mtime_ms: Option<i64>,
    pub mode: Option<u32>,
    pub link_target: Option<Vec<u8>>,
    pub conflict: Option<Conflict>,
    pub proposed_name: Option<Vec<u8>>,
    pub resolution: Option<ConflictChoice>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TransferItem {
    pub seq: u32,
    pub plan: PlanItem,
    pub state: ItemState,
    /// Bytes known to be at the destination temp file (XFR-12).
    pub committed: u64,
    pub temp_name: Option<Vec<u8>>,
    pub attempts: u32,
    pub error: Option<String>,
}

impl TransferItem {
    pub fn size(&self) -> u64 {
        self.plan.size.unwrap_or(0)
    }

    pub fn extra(&self) -> ItemExtra {
        ItemExtra {
            mtime_ms: self.plan.mtime_ms,
            mode: self.plan.mode,
            link_target: self.plan.link_target.clone(),
            conflict: self.plan.conflict.clone(),
            proposed_name: self.plan.proposed_name.clone(),
            resolution: self.plan.resolution,
        }
    }
}

pub fn kind_name(k: Kind) -> &'static str {
    match k {
        Kind::Unknown => "unknown",
        Kind::File => "file",
        Kind::Dir => "dir",
        Kind::Symlink => "symlink",
        Kind::Special => "special",
    }
}

pub fn kind_from_name(s: &str) -> Kind {
    [Kind::File, Kind::Dir, Kind::Symlink, Kind::Special]
        .into_iter()
        .find(|k| kind_name(*k) == s)
        .unwrap_or(Kind::Unknown)
}

pub fn operation_name(k: OperationKind) -> &'static str {
    match k {
        OperationKind::Copy => "copy",
        OperationKind::Move => "move",
        OperationKind::Delete => "delete",
        OperationKind::Compress => "compress",
        OperationKind::Extract => "extract",
        OperationKind::Sync => "sync",
        OperationKind::WriteBack => "writeback",
    }
}

pub fn operation_from_name(s: &str) -> OperationKind {
    [
        OperationKind::Move,
        OperationKind::Delete,
        OperationKind::Compress,
        OperationKind::Extract,
        OperationKind::Sync,
        OperationKind::WriteBack,
    ]
    .into_iter()
    .find(|k| operation_name(*k) == s)
    .unwrap_or(OperationKind::Copy)
}

/// What the UI lists: a transfer without its items (INT-2, INT-3, §15.4).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TransferSummary {
    pub id: TransferId,
    pub kind: OperationKind,
    pub title: String,
    pub state: TransferState,
    pub dest: Uri,
    pub position: i64,
    pub created_ms: i64,
    pub finished_ms: Option<i64>,
    pub bytes_total: u64,
    pub bytes_done: u64,
    pub items_total: u64,
    pub items_done: u64,
    pub items_failed: u64,
    pub error: Option<String>,
    pub options: TransferOptions,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Transfer {
    pub id: TransferId,
    pub kind: OperationKind,
    pub title: String,
    pub state: TransferState,
    pub dest: Uri,
    pub position: i64,
    pub created_ms: i64,
    pub finished_ms: Option<i64>,
    pub bytes_total: u64,
    pub bytes_done: u64,
    pub items_total: u64,
    pub items_done: u64,
    pub items_failed: u64,
    pub error: Option<String>,
    pub options: TransferOptions,
    pub items: Vec<TransferItem>,
}

impl Transfer {
    /// Builds a queued transfer from a plan (XFR-1: one plan, executed).
    /// Deletes are ordered children-first unless they go to the trash, where
    /// the roots go first and take their subtrees with them (OPS-8).
    pub fn from_plan(
        id: TransferId,
        plan: Plan,
        title: &str,
        options: TransferOptions,
        now_ms: i64,
    ) -> Transfer {
        let mut plan_items = plan.items;
        if plan.kind == OperationKind::Delete {
            plan_items.sort_by_key(|i| {
                let depth = i.src.path.depth();
                if options.trash {
                    depth as i64
                } else {
                    -(depth as i64)
                }
            });
        }
        let items = plan_items
            .into_iter()
            .enumerate()
            .map(|(n, p)| TransferItem {
                seq: u32::try_from(n).unwrap_or(u32::MAX),
                plan: p,
                state: ItemState::Pending,
                committed: 0,
                temp_name: None,
                attempts: 0,
                error: None,
            })
            .collect();
        let mut t = Transfer {
            id,
            kind: plan.kind,
            title: if title.is_empty() {
                default_title(plan.kind, plan.totals.files + plan.totals.dirs)
            } else {
                title.to_owned()
            },
            state: TransferState::Queued,
            dest: plan.destination,
            position: 0,
            created_ms: now_ms,
            finished_ms: None,
            bytes_total: 0,
            bytes_done: 0,
            items_total: 0,
            items_done: 0,
            items_failed: 0,
            error: None,
            options,
            items,
        };
        t.recount();
        t
    }

    /// Moves to `to` if the table allows it.
    pub fn transition(&mut self, to: TransferState) -> Result<()> {
        if !self.state.can_transition(to) {
            return Err(Error::new(
                ErrorKind::InvalidArgument,
                format!("transfer cannot go from {:?} to {:?}", self.state, to),
            ));
        }
        self.state = to;
        Ok(())
    }

    /// Recomputes counters from the items. Skipped items leave the byte total
    /// so progress still reaches 100 %.
    pub fn recount(&mut self) {
        self.items_total = self.items.len() as u64;
        self.items_done = self
            .items
            .iter()
            .filter(|i| matches!(i.state, ItemState::Done | ItemState::Skipped))
            .count() as u64;
        self.items_failed = self.items.iter().filter(|i| i.state == ItemState::Failed).count() as u64;
        self.bytes_total = self
            .items
            .iter()
            .filter(|i| i.state != ItemState::Skipped)
            .map(TransferItem::size)
            .sum();
        self.bytes_done = self
            .items
            .iter()
            .filter(|i| i.state == ItemState::Done)
            .map(TransferItem::size)
            .sum();
    }

    /// Changes one item's state and keeps the counters in step without a
    /// full recount (plans can hold 100 000 items).
    pub fn set_item_state(&mut self, seq: u32, to: ItemState) {
        let Some(item) = self.items.get_mut(seq as usize) else {
            return;
        };
        let (from, size) = (item.state, item.size());
        item.state = to;
        let done = |s: ItemState| matches!(s, ItemState::Done | ItemState::Skipped);
        self.items_done = (self.items_done + u64::from(done(to))).saturating_sub(u64::from(done(from)));
        self.items_failed = (self.items_failed + u64::from(to == ItemState::Failed))
            .saturating_sub(u64::from(from == ItemState::Failed));
        if to == ItemState::Skipped {
            self.bytes_total = self.bytes_total.saturating_sub(size);
        } else if from == ItemState::Skipped {
            self.bytes_total += size;
        }
        if to == ItemState::Done {
            self.bytes_done += size;
        } else if from == ItemState::Done {
            self.bytes_done = self.bytes_done.saturating_sub(size);
        }
    }

    pub fn summary(&self) -> TransferSummary {
        TransferSummary {
            id: self.id,
            kind: self.kind,
            title: self.title.clone(),
            state: self.state,
            dest: self.dest.clone(),
            position: self.position,
            created_ms: self.created_ms,
            finished_ms: self.finished_ms,
            bytes_total: self.bytes_total,
            bytes_done: self.bytes_done,
            items_total: self.items_total,
            items_done: self.items_done,
            items_failed: self.items_failed,
            error: self.error.clone(),
            options: self.options.clone(),
        }
    }

    pub fn dest_location(&self) -> &str {
        &self.dest.location
    }

    /// Items waiting for a user decision.
    pub fn questions(&self) -> impl Iterator<Item = &TransferItem> {
        self.items.iter().filter(|i| i.state == ItemState::NeedsAnswer)
    }
}

fn default_title(kind: OperationKind, count: u64) -> String {
    let verb = match kind {
        OperationKind::Copy => "Copy",
        OperationKind::Move => "Move",
        OperationKind::Delete => "Delete",
        OperationKind::Compress => "Compress",
        OperationKind::Extract => "Extract",
        OperationKind::Sync => "Sync",
        OperationKind::WriteBack => "Save",
    };
    if count == 1 {
        format!("{verb} 1 item")
    } else {
        format!("{verb} {count} items")
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::transfer::testutil::{plan, plan_item};

    /// The complete expected table, written independently of the code.
    fn allowed(from: TransferState, to: TransferState) -> bool {
        use TransferState::*;
        let waiting = |s: TransferState| matches!(s, Waiting(_));
        match from {
            Queued => matches!(to, Scanning | Running | Paused | Canceled) || waiting(to),
            Scanning => matches!(to, Running | Paused | Failed | Completed | Canceled) || waiting(to),
            Running => matches!(to, Paused | Failed | Completed | Canceled) || waiting(to),
            Paused => matches!(to, Queued | Running | Canceled),
            Waiting(r) => {
                matches!(to, Queued | Running | Paused | Failed | Canceled)
                    || matches!(to, Waiting(n) if n != r)
            }
            Failed => to == Queued,
            Completed | Canceled => false,
        }
    }

    #[test]
    fn transition_table_is_exhaustive() {
        for from in TransferState::ALL {
            for to in TransferState::ALL {
                assert_eq!(from.can_transition(to), allowed(from, to), "{from:?} -> {to:?}");
            }
        }
    }

    #[test]
    fn transition_mutates_only_when_allowed() {
        let mut t = Transfer::from_plan(
            1,
            plan(OperationKind::Copy, vec![]),
            "t",
            TransferOptions::default(),
            0,
        );
        t.transition(TransferState::Running).unwrap();
        let err = t.transition(TransferState::Queued).unwrap_err();
        assert_eq!(err.kind, ErrorKind::InvalidArgument);
        assert_eq!(t.state, TransferState::Running);
        t.transition(TransferState::Completed).unwrap();
        assert!(t.transition(TransferState::Running).is_err());
    }

    #[test]
    fn finished_runnable_waiting_flags() {
        assert!(TransferState::Failed.is_finished());
        assert!(TransferState::Completed.is_finished());
        assert!(TransferState::Canceled.is_finished());
        assert!(!TransferState::Paused.is_finished());
        assert!(TransferState::Waiting(WaitReason::Volume).is_waiting());
        assert!(!TransferState::Paused.is_waiting() && !TransferState::Queued.is_waiting());
        assert!(TransferState::ALL.iter().filter(|s| s.is_waiting()).count() == WaitReason::ALL.len());
        assert!(TransferState::Queued.is_runnable());
        assert!(TransferState::Running.is_runnable());
        assert!(!TransferState::Paused.is_runnable());
        assert!(!TransferState::Waiting(WaitReason::Bridge).is_runnable());
    }

    #[test]
    fn db_encoding_round_trips() {
        for s in TransferState::ALL {
            let (name, reason) = s.to_db();
            assert_eq!(TransferState::from_db(name, reason).unwrap(), s);
        }
        assert!(TransferState::from_db("bogus", None).is_err());
        for i in [
            ItemState::Pending,
            ItemState::Running,
            ItemState::Done,
            ItemState::Skipped,
            ItemState::Failed,
            ItemState::NeedsAnswer,
        ] {
            assert_eq!(ItemState::from_name(i.name()), i);
        }
        for k in [
            OperationKind::Copy,
            OperationKind::Move,
            OperationKind::Delete,
            OperationKind::Compress,
            OperationKind::Extract,
            OperationKind::Sync,
            OperationKind::WriteBack,
        ] {
            assert_eq!(operation_from_name(operation_name(k)), k);
        }
        for k in [Kind::File, Kind::Dir, Kind::Symlink, Kind::Special, Kind::Unknown] {
            assert_eq!(kind_from_name(kind_name(k)), k);
        }
    }

    #[test]
    fn from_plan_counts_and_titles() {
        let items = vec![
            plan_item("x", "x", Kind::File, 10),
            plan_item("d", "d", Kind::Dir, 0),
            plan_item("d/y", "d/y", Kind::File, 5),
        ];
        let mut t = Transfer::from_plan(
            7,
            plan(OperationKind::Copy, items),
            "",
            TransferOptions::default(),
            42,
        );
        assert_eq!((t.items_total, t.bytes_total), (3, 15));
        assert_eq!(t.title, "Copy 0 items");
        assert_eq!(t.created_ms, 42);
        assert_eq!(t.dest_location(), "b");
        t.items[0].state = ItemState::Done;
        t.items[2].state = ItemState::Skipped;
        t.items[1].state = ItemState::Failed;
        t.recount();
        assert_eq!(
            (t.items_done, t.items_failed, t.bytes_total, t.bytes_done),
            (2, 1, 10, 10)
        );
        let s = t.summary();
        assert_eq!((s.id, s.items_done, s.bytes_done), (7, 2, 10));
        assert_eq!(default_title(OperationKind::Delete, 1), "Delete 1 item");
    }

    #[test]
    fn default_title_counts_files_and_folders() {
        let mut p = plan(OperationKind::Move, Vec::new());
        p.totals.files = 4;
        p.totals.dirs = 3;
        let t = Transfer::from_plan(1, p, "", TransferOptions::default(), 0);
        assert_eq!(t.title, "Move 7 items");
    }

    #[test]
    fn settled_items_need_no_more_work() {
        for s in [ItemState::Done, ItemState::Skipped, ItemState::Failed] {
            assert!(s.is_settled(), "{s:?}");
        }
        for s in [ItemState::Pending, ItemState::Running, ItemState::NeedsAnswer] {
            assert!(!s.is_settled(), "{s:?}");
        }
    }

    #[test]
    fn incremental_counters_match_a_recount() {
        let items = vec![
            plan_item("a", "a", Kind::File, 10),
            plan_item("b", "b", Kind::File, 20),
            plan_item("c", "c", Kind::File, 30),
        ];
        let mut t = Transfer::from_plan(
            1,
            plan(OperationKind::Copy, items),
            "t",
            TransferOptions::default(),
            0,
        );
        let steps = [
            (0, ItemState::Running),
            (0, ItemState::Done),
            (1, ItemState::Skipped),
            (2, ItemState::Failed),
            (2, ItemState::Pending),
            (2, ItemState::Done),
            (0, ItemState::Pending),
            (1, ItemState::Pending),
            (1, ItemState::Failed),
        ];
        // (items_done, items_failed, bytes_total, bytes_done) after each step.
        let expected = [
            (0, 0, 60, 0),
            (1, 0, 60, 10),
            (2, 0, 40, 10),
            (2, 1, 40, 10),
            (2, 0, 40, 10),
            (3, 0, 40, 40),
            (2, 0, 40, 30),
            (1, 0, 60, 30),
            (1, 1, 60, 30),
        ];
        for ((seq, to), want) in steps.into_iter().zip(expected) {
            t.set_item_state(seq, to);
            assert_eq!(t.items[seq as usize].state, to);
            let got = (t.items_done, t.items_failed, t.bytes_total, t.bytes_done);
            assert_eq!(got, want, "after {seq} -> {to:?}");
            let mut fresh = t.clone();
            fresh.recount();
            assert_eq!(t, fresh, "after {seq} -> {to:?}");
        }
        t.set_item_state(99, ItemState::Done);
    }

    #[test]
    fn deletes_are_ordered_by_depth() {
        let items = vec![
            plan_item("d", "d", Kind::Dir, 0),
            plan_item("d/e/f", "d/e/f", Kind::File, 1),
            plan_item("d/e", "d/e", Kind::Dir, 0),
        ];
        let names =
            |t: &Transfer| -> Vec<String> { t.items.iter().map(|i| i.plan.src.path.display()).collect() };
        let t = Transfer::from_plan(
            1,
            plan(OperationKind::Delete, items.clone()),
            "t",
            TransferOptions::default(),
            0,
        );
        assert_eq!(names(&t), vec!["d/e/f", "d/e", "d"]);
        let trash = TransferOptions {
            trash: true,
            ..TransferOptions::default()
        };
        let t = Transfer::from_plan(1, plan(OperationKind::Delete, items), "t", trash, 0);
        assert_eq!(names(&t), vec!["d", "d/e", "d/e/f"]);
    }

    #[test]
    fn options_defaults_survive_partial_json() {
        let o: TransferOptions = serde_json::from_str("{\"verify_checksums\":true}").unwrap();
        assert!(o.verify_checksums);
        assert!(o.preserve_mtime);
        let o: TransferOptions = serde_json::from_str("{}").unwrap();
        assert_eq!(o, TransferOptions::default());
    }
}
