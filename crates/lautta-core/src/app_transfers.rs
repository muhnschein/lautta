// SPDX-License-Identifier: LGPL-2.1-or-later
//! User-level actions of the transfers area on [`Core`](crate::app::Core):
//! the grouped transfer list with live progress (XFR-8, §15.4) and start-up
//! restore (XFR-11). Everything here is Qt-free; the Qt layer turns it into
//! models.

use crate::app::Core;
use crate::error::{Error, ErrorKind, Result};
use crate::locations::LocationRegistry;
use crate::ops::{Conflict, ConflictChoice, OperationKind};
use crate::transfer::model::operation_name;
use crate::transfer::{TransferId, TransferState, TransferSummary, WaitReason};
use crate::uri::Uri;
use std::collections::{HashMap, HashSet};

/// The groups of the Transfers page (§15.4), in display order.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum TransferGroup {
    Active,
    /// Waiting for the user, the network, the bridge or a card.
    Waiting,
    Paused,
    History,
}

impl TransferGroup {
    pub const ALL: [TransferGroup; 4] = [
        TransferGroup::Active,
        TransferGroup::Waiting,
        TransferGroup::Paused,
        TransferGroup::History,
    ];

    pub fn name(self) -> &'static str {
        match self {
            TransferGroup::Active => "active",
            TransferGroup::Waiting => "waiting",
            TransferGroup::Paused => "paused",
            TransferGroup::History => "history",
        }
    }
}

pub fn group_of(state: TransferState) -> TransferGroup {
    match state {
        TransferState::Queued | TransferState::Scanning | TransferState::Running => TransferGroup::Active,
        TransferState::Waiting(_) => TransferGroup::Waiting,
        TransferState::Paused => TransferGroup::Paused,
        TransferState::Failed | TransferState::Completed | TransferState::Canceled => TransferGroup::History,
    }
}

/// Engineering name of a transfer state for the UI (`state` role).
pub fn state_name(state: TransferState) -> &'static str {
    state.to_db().0
}

/// Engineering name of the waiting reason, empty when not waiting.
pub fn wait_reason_name(state: TransferState) -> &'static str {
    match state {
        TransferState::Waiting(r) => WaitReason::name(r),
        _ => "",
    }
}

/// Which way the bytes go, for the row icon.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Direction {
    Upload,
    Download,
    /// Within the device.
    Local,
    /// Between remote locations.
    Remote,
    Delete,
}

impl Direction {
    pub fn name(self) -> &'static str {
        match self {
            Direction::Upload => "upload",
            Direction::Download => "download",
            Direction::Local => "local",
            Direction::Remote => "remote",
            Direction::Delete => "delete",
        }
    }
}

pub fn direction_of(locations: &LocationRegistry, kind: OperationKind, src: &Uri, dst: &Uri) -> Direction {
    if kind == OperationKind::Delete {
        return Direction::Delete;
    }
    let local = |u: &Uri| locations.to_local_path(u).is_some();
    match (local(src), local(dst)) {
        (true, true) => Direction::Local,
        (true, false) => Direction::Upload,
        (false, true) => Direction::Download,
        (false, false) => Direction::Remote,
    }
}

/// Splits an item error `"Kind: message"` (as the engine stores it) into the
/// kind name, which the UI translates, and the detail.
pub fn split_error(text: &str) -> (String, String) {
    let (head, tail) = text.split_once(": ").unwrap_or((text, ""));
    if ErrorKind::ALL.iter().any(|k| k.name() == head) {
        (head.to_owned(), tail.to_owned())
    } else {
        (ErrorKind::Internal.name().to_owned(), text.to_owned())
    }
}

/// Engineering names of conflict choices (`answer(id, item, choice, …)`),
/// the same as the operations area uses (the enum names).
pub fn choice_name(c: ConflictChoice) -> &'static str {
    match c {
        ConflictChoice::Replace => "Replace",
        ConflictChoice::Skip => "Skip",
        ConflictChoice::KeepBoth => "KeepBoth",
        ConflictChoice::Merge => "Merge",
        ConflictChoice::ReplaceIfNewer => "ReplaceIfNewer",
        ConflictChoice::Resume => "Resume",
    }
}

pub fn parse_choice(name: &str) -> Option<ConflictChoice> {
    [
        ConflictChoice::Replace,
        ConflictChoice::Skip,
        ConflictChoice::KeepBoth,
        ConflictChoice::Merge,
        ConflictChoice::ReplaceIfNewer,
        ConflictChoice::Resume,
    ]
    .into_iter()
    .find(|c| choice_name(*c) == name)
}

/// A conflict question as the UI gets it: the core `Conflict` fields
/// (snake_case, as the shared ConflictDialog reads them), `choices` by name.
pub fn conflict_json(id: TransferId, item: u32, name: &str, c: &Conflict) -> serde_json::Value {
    serde_json::json!({
        "transferId": id,
        "item": item,
        "name": name,
        "dst_is_dir": c.dst_is_dir,
        "src_is_dir": c.src_is_dir,
        "src_size": c.src_size,
        "dst_size": c.dst_size,
        "src_mtime_ms": c.src_mtime_ms,
        "dst_mtime_ms": c.dst_mtime_ms,
        "resumable": c.resumable,
        "choices": c.choices.iter().map(|x| choice_name(*x)).collect::<Vec<_>>(),
    })
}

/// Latest known progress per running transfer, fed by `TransferEvent::Progress`
/// (at most 4 a second), and the aggregate over the list (XFR-8, INT-3).
#[derive(Debug, Default)]
pub struct ProgressBook {
    live: HashMap<TransferId, Live>,
}

#[derive(Debug, Clone, Copy)]
struct Live {
    bytes_done: u64,
    rate: u64,
    eta_secs: Option<u64>,
}

/// Totals over the transfers that are moving bytes.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Aggregate {
    /// Queued, scanning or running.
    pub active: usize,
    /// Waiting for the user, network, bridge or card.
    pub waiting: usize,
    pub paused: usize,
    /// Everything not finished.
    pub unfinished: usize,
    pub bytes_done: u64,
    pub bytes_total: u64,
    pub rate: u64,
    pub eta_secs: Option<u64>,
}

impl ProgressBook {
    pub fn update(&mut self, id: TransferId, bytes_done: u64, rate: u64, eta_secs: Option<u64>) {
        self.live.insert(
            id,
            Live {
                bytes_done,
                rate,
                eta_secs,
            },
        );
    }

    /// Keeps the entries of `ids` only (finished and removed transfers go).
    pub fn retain(&mut self, ids: &HashSet<TransferId>) {
        self.live.retain(|id, _| ids.contains(id));
    }

    /// Bytes done (the larger of the stored and the live value), rate and
    /// ETA of one transfer. Only running transfers have a rate.
    pub fn of(&self, s: &TransferSummary) -> (u64, u64, Option<u64>) {
        let live = self.live.get(&s.id).copied();
        let moving = group_of(s.state) == TransferGroup::Active;
        let done = live.map_or(s.bytes_done, |l| l.bytes_done.max(s.bytes_done));
        match live {
            Some(l) if moving => (done.min(s.bytes_total.max(done)), l.rate, l.eta_secs),
            _ => (done, 0, None),
        }
    }

    pub fn aggregate(&self, list: &[TransferSummary]) -> Aggregate {
        let mut a = Aggregate::default();
        for s in list {
            match group_of(s.state) {
                TransferGroup::Active => {
                    let (done, rate, _) = self.of(s);
                    a.active += 1;
                    a.bytes_done += done;
                    a.bytes_total += s.bytes_total;
                    a.rate += rate;
                }
                TransferGroup::Waiting => a.waiting += 1,
                TransferGroup::Paused => a.paused += 1,
                _ => {}
            }
        }
        a.unfinished = a.active + a.waiting + a.paused;
        a.eta_secs = (a.rate > 0).then(|| a.bytes_total.saturating_sub(a.bytes_done) / a.rate);
        a
    }
}

/// One row of the grouped list.
#[derive(Debug, Clone)]
pub struct TransferRow {
    pub summary: TransferSummary,
    pub group: TransferGroup,
    pub direction: Direction,
    pub bytes_done: u64,
    pub rate: u64,
    pub eta_secs: Option<u64>,
    /// Open conflict questions.
    pub questions: usize,
    /// Name of the destination location and its address (LOC-7).
    pub dest_name: String,
    pub dest_address: String,
}

impl TransferRow {
    pub fn kind_name(&self) -> &'static str {
        operation_name(self.summary.kind)
    }
}

/// What start-up found (XFR-11).
#[derive(Debug, Clone, Default)]
pub struct StartReport {
    /// Unfinished transfers that came back; paused unless auto-resumed.
    pub restored: usize,
    /// Of those, how many are paused now.
    pub paused: usize,
}

/// Orders the list for display: transfers by group, history newest first.
fn order_rows(rows: &mut [TransferRow]) {
    rows.sort_by_key(|r| {
        let g = TransferGroup::ALL.iter().position(|x| *x == r.group).unwrap_or(0);
        let tie = match r.group {
            TransferGroup::History => -r.summary.finished_ms.unwrap_or(r.summary.created_ms),
            _ => r.summary.position,
        };
        (g, tie, r.summary.id)
    });
}

impl Core {
    /// Brings back the persisted queue (XFR-11) and removes expired working
    /// copies (PRV-6). `auto_resume` is the setting AND the bridge being
    /// reachable; otherwise transfers wait paused with *Resume*.
    pub async fn start_transfers(&self, auto_resume: bool) -> Result<StartReport> {
        let restored = self.engine.load(auto_resume).await?;
        let paused = self
            .engine
            .list()
            .iter()
            .filter(|s| s.state == TransferState::Paused)
            .count();
        let _ = self.working_copies.expire().await;
        Ok(StartReport { restored, paused })
    }

    /// Whether auto-resume may start transfers (XFR-11): false only while a
    /// bridge is present but unusable (too old, consent denied). Standalone
    /// mode and a bridge that is still connecting count as reachable; the
    /// engine parks transfers on bridge locations until it is ready
    /// (NVB-12).
    pub fn bridge_reachable(&self) -> bool {
        use crate::bridge::BridgeStatus;
        self.bridge.as_ref().map_or(true, |b| {
            !matches!(b.status(), BridgeStatus::TooOld | BridgeStatus::ConsentDenied)
        })
    }

    /// Transfers still unfinished when the app closes (XFR-6).
    pub fn pending_at_close(&self) -> usize {
        self.engine.pending_summary()
    }

    /// The transfer list with live progress, in display order (§15.4).
    pub fn transfer_rows(&self, book: &ProgressBook) -> Vec<TransferRow> {
        let mut rows: Vec<TransferRow> = self
            .engine
            .list()
            .into_iter()
            .map(|s| self.row_of(book, s))
            .collect();
        order_rows(&mut rows);
        rows
    }

    fn row_of(&self, book: &ProgressBook, s: TransferSummary) -> TransferRow {
        let group = group_of(s.state);
        let direction = match self.engine.first_item(s.id) {
            Some(i) => direction_of(&self.locations, s.kind, &i.src, &i.dst),
            None => direction_of(&self.locations, s.kind, &s.dest, &s.dest),
        };
        let questions = if s.state == TransferState::Waiting(WaitReason::Question) {
            self.engine.questions(s.id).map_or(0, |q| q.len())
        } else {
            0
        };
        let (bytes_done, rate, eta_secs) = book.of(&s);
        TransferRow {
            group,
            direction,
            bytes_done,
            rate,
            eta_secs,
            questions,
            dest_name: self
                .location(&s.dest.location)
                .map(|l| l.name)
                .unwrap_or_default(),
            dest_address: self.locations.display_address(&s.dest),
            summary: s,
        }
    }

    /// Removes finished transfers from the history; returns how many.
    pub fn clear_history(&self) -> usize {
        self.engine
            .list()
            .iter()
            .filter(|s| s.state.is_finished())
            .filter(|s| self.engine.forget(s.id).is_ok())
            .count()
    }

    /// The open questions of a transfer as UI objects.
    pub fn question_list(&self, id: TransferId) -> Result<Vec<serde_json::Value>> {
        let items = self.engine.items(id)?;
        let questions = self.engine.questions(id)?;
        Ok(questions
            .iter()
            .map(|(seq, c)| {
                let name = items
                    .get(*seq as usize)
                    .and_then(|i| i.plan.dst.name())
                    .map(crate::vpath::display_name)
                    .unwrap_or_default();
                conflict_json(id, *seq, &name, c)
            })
            .collect())
    }

    /// Answers a conflict by the choice's engineering name.
    pub fn answer_question(&self, id: TransferId, item: u32, choice: &str, apply_all: bool) -> Result<()> {
        let choice = parse_choice(choice)
            .ok_or_else(|| Error::new(ErrorKind::InvalidArgument, "unknown conflict choice"))?;
        self.engine.answer(id, item, choice, apply_all)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::paths::AppPaths;
    use crate::transfer::{TransferOptions, TransferSummary};

    fn summary(id: i64, state: TransferState, done: u64, total: u64) -> TransferSummary {
        TransferSummary {
            id,
            kind: OperationKind::Copy,
            title: "t".into(),
            state,
            dest: Uri::root("a"),
            position: id,
            created_ms: id,
            finished_ms: state.is_finished().then_some(id * 10),
            bytes_total: total,
            bytes_done: done,
            items_total: 1,
            items_done: 0,
            items_failed: 0,
            error: None,
            options: TransferOptions::default(),
        }
    }

    #[test]
    fn states_map_to_groups() {
        use TransferState::*;
        let expect = [
            (Queued, TransferGroup::Active),
            (Scanning, TransferGroup::Active),
            (Running, TransferGroup::Active),
            (Paused, TransferGroup::Paused),
            (Waiting(WaitReason::Question), TransferGroup::Waiting),
            (Waiting(WaitReason::Network), TransferGroup::Waiting),
            (Waiting(WaitReason::Volume), TransferGroup::Waiting),
            (Failed, TransferGroup::History),
            (Completed, TransferGroup::History),
            (Canceled, TransferGroup::History),
        ];
        for (state, group) in expect {
            assert_eq!(group_of(state), group, "{state:?}");
        }
        assert_eq!(
            TransferGroup::ALL.map(TransferGroup::name),
            ["active", "waiting", "paused", "history"]
        );
        assert_eq!(state_name(Waiting(WaitReason::Volume)), "waiting");
        assert_eq!(wait_reason_name(Waiting(WaitReason::Volume)), "volume");
        assert_eq!(wait_reason_name(Running), "");
    }

    #[test]
    fn errors_split_into_kind_and_detail() {
        assert_eq!(
            split_error("NoSpace: disk full"),
            ("NoSpace".to_owned(), "disk full".to_owned())
        );
        assert_eq!(split_error("Locked"), ("Locked".to_owned(), String::new()));
        assert_eq!(
            split_error("Weird: thing"),
            ("Internal".to_owned(), "Weird: thing".to_owned())
        );
    }

    #[test]
    fn choice_names_round_trip() {
        for c in [
            ConflictChoice::Replace,
            ConflictChoice::Skip,
            ConflictChoice::KeepBoth,
            ConflictChoice::Merge,
            ConflictChoice::ReplaceIfNewer,
            ConflictChoice::Resume,
        ] {
            assert_eq!(parse_choice(choice_name(c)), Some(c));
        }
        assert_eq!(parse_choice("nope"), None);
    }

    #[test]
    fn aggregate_counts_only_moving_transfers() {
        use TransferState::*;
        let mut book = ProgressBook::default();
        let list = vec![
            summary(1, Running, 100, 1000),
            summary(2, Queued, 0, 500),
            summary(3, Paused, 50, 100),
            summary(4, Waiting(WaitReason::Network), 0, 100),
            summary(5, Completed, 10, 10),
        ];
        book.update(1, 300, 100, Some(7));
        book.update(3, 99, 5, None);
        let a = book.aggregate(&list);
        assert_eq!((a.active, a.waiting, a.paused, a.unfinished), (2, 1, 1, 4));
        assert_eq!(a.bytes_done, 300, "live bytes win over the stored ones");
        assert_eq!(a.bytes_total, 1500, "paused and finished do not count");
        assert_eq!(a.rate, 100, "a paused transfer has no rate");
        assert_eq!(a.eta_secs, Some(12));
        let (done, rate, eta) = book.of(&list[2]);
        assert_eq!((done, rate, eta), (99, 0, None));
        assert_eq!(book.of(&list[0]), (300, 100, Some(7)));
    }

    #[test]
    fn aggregate_without_rate_has_no_eta_and_book_forgets() {
        let mut book = ProgressBook::default();
        let list = vec![summary(1, TransferState::Running, 5, 10)];
        assert_eq!(book.aggregate(&list).eta_secs, None);
        book.update(1, 6, 2, Some(2));
        assert_eq!(book.aggregate(&list).eta_secs, Some(2));
        book.retain(&HashSet::new());
        assert_eq!(book.aggregate(&list).rate, 0);
    }

    #[test]
    fn rows_are_ordered_by_group_then_queue_then_history_newest_first() {
        let mk = |id, state| TransferRow {
            group: group_of(state),
            direction: Direction::Local,
            bytes_done: 0,
            rate: 0,
            eta_secs: None,
            questions: 0,
            dest_name: String::new(),
            dest_address: String::new(),
            summary: summary(id, state, 0, 0),
        };
        let mut rows = vec![
            mk(1, TransferState::Completed),
            mk(2, TransferState::Paused),
            mk(3, TransferState::Completed),
            mk(4, TransferState::Running),
            mk(5, TransferState::Waiting(WaitReason::Bridge)),
            mk(6, TransferState::Queued),
        ];
        order_rows(&mut rows);
        let ids: Vec<i64> = rows.iter().map(|r| r.summary.id).collect();
        assert_eq!(ids, [4, 6, 5, 2, 3, 1]);
    }

    #[test]
    fn conflict_json_carries_choice_names() {
        let c = Conflict {
            dst_is_dir: false,
            src_is_dir: false,
            src_size: Some(3),
            dst_size: Some(4),
            src_mtime_ms: Some(1),
            dst_mtime_ms: None,
            resumable: false,
            choices: vec![ConflictChoice::Replace, ConflictChoice::KeepBoth],
        };
        let j = conflict_json(7, 2, "a.txt", &c);
        assert_eq!(j["transferId"], 7);
        assert_eq!(j["item"], 2);
        assert_eq!(j["name"], "a.txt");
        assert_eq!(j["dst_size"], 4);
        assert!(j["dst_mtime_ms"].is_null());
        assert_eq!(j["choices"], serde_json::json!(["Replace", "KeepBoth"]));
    }

    #[test]
    fn direction_follows_where_the_ends_are() {
        let dir = tempfile::tempdir().unwrap();
        let paths = AppPaths::new(dir.path());
        std::fs::create_dir_all(dir.path().join("Documents")).unwrap();
        let reg = LocationRegistry::with_media_root(paths, dir.path().join("media"));
        reg.refresh();
        let local = Uri::parse("lautta://user-documents/a").unwrap();
        let remote = Uri::parse("lautta://nv-x/a").unwrap();
        let d = |k, s: &Uri, t: &Uri| direction_of(&reg, k, s, t).name();
        assert_eq!(d(OperationKind::Copy, &local, &local), "local");
        assert_eq!(d(OperationKind::Copy, &local, &remote), "upload");
        assert_eq!(d(OperationKind::Move, &remote, &local), "download");
        assert_eq!(d(OperationKind::Copy, &remote, &remote), "remote");
        assert_eq!(d(OperationKind::Delete, &local, &local), "delete");
    }
}
