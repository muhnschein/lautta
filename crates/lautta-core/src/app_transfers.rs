// SPDX-License-Identifier: LGPL-2.1-or-later

use crate::app::Core;
use crate::app_home::RecentLook;
use crate::app_operations::kind_name;
use crate::date_group::DateGroup;
use crate::error::{Error, ErrorKind, Result};
use crate::locations::LocationRegistry;
use crate::ops::{Conflict, ConflictChoice, OperationKind};
use crate::transfer::model::operation_name;
use crate::transfer::{TransferId, TransferState, TransferSummary, WaitReason};
use crate::uri::Uri;
use std::collections::{HashMap, HashSet};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum TransferGroup {
    Active,
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

pub fn state_name(state: TransferState) -> &'static str {
    state.to_db().0
}

pub fn wait_reason_name(state: TransferState) -> &'static str {
    match state {
        TransferState::Waiting(r) => WaitReason::name(r),
        _ => "",
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Direction {
    Upload,
    Download,
    Local,
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

pub fn split_error(text: &str) -> (String, String) {
    let (head, tail) = text.split_once(": ").unwrap_or((text, ""));
    if ErrorKind::ALL.iter().any(|k| k.name() == head) {
        (head.to_owned(), tail.to_owned())
    } else {
        (ErrorKind::Internal.name().to_owned(), text.to_owned())
    }
}

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

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct QuestionAbout {
    pub name: String,
    pub kind: &'static str,
    pub open: usize,
    pub source: String,
    pub destination: String,
    pub source_look: RecentLook,
    pub destination_look: RecentLook,
}

pub fn conflict_json(id: TransferId, item: u32, about: &QuestionAbout, c: &Conflict) -> serde_json::Value {
    serde_json::json!({
        "transferId": id,
        "item": item,
        "name": about.name,
        "kind": about.kind,
        "open": about.open,
        "source": about.source,
        "destination": about.destination,
        "source_look": about.source_look,
        "destination_look": about.destination_look,
        "dst_is_dir": c.dst_is_dir,
        "src_is_dir": c.src_is_dir,
        "src_size": c.src_size,
        "dst_size": c.dst_size,
        "src_mtime_ms": c.src_mtime_ms,
        "dst_mtime_ms": c.dst_mtime_ms,
        "resumable": c.resumable,
        "choices": c.choices.iter().map(|x| choice_name(*x)).collect::<Vec<_>>(),
        "add_only": c.add_only,
    })
}

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

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Aggregate {
    pub active: usize,
    pub waiting: usize,
    pub paused: usize,
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

    pub fn retain(&mut self, ids: &HashSet<TransferId>) {
        self.live.retain(|id, _| ids.contains(id));
    }

    pub fn of(&self, s: &TransferSummary) -> (u64, u64, Option<u64>) {
        let group = group_of(s.state);
        let live = self
            .live
            .get(&s.id)
            .copied()
            .filter(|_| group != TransferGroup::History);
        let moving = group == TransferGroup::Active;
        let done = live.map_or(s.bytes_done, |l| l.bytes_done.max(s.bytes_done));
        match live {
            Some(l) if moving => (done.min(s.bytes_total.max(done)), l.rate, l.eta_secs),
            _ => (done, 0, None),
        }
    }

    pub fn aggregate(&self, list: &[TransferSummary]) -> Aggregate {
        let mut a = Aggregate::default();
        let (mut paused_done, mut paused_total) = (0, 0);
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
                TransferGroup::Paused => {
                    a.paused += 1;
                    paused_done += self.of(s).0;
                    paused_total += s.bytes_total;
                }
                _ => {}
            }
        }
        if a.active == 0 {
            (a.bytes_done, a.bytes_total) = (paused_done, paused_total);
        }
        a.unfinished = a.active + a.waiting + a.paused;
        a.eta_secs = (a.rate > 0).then(|| a.bytes_total.saturating_sub(a.bytes_done) / a.rate);
        a
    }
}

#[derive(Debug, Clone)]
pub struct TransferRow {
    pub summary: TransferSummary,
    pub group: TransferGroup,
    pub date_group: DateGroup,
    pub direction: Direction,
    pub bytes_done: u64,
    pub rate: u64,
    pub eta_secs: Option<u64>,
    pub questions: usize,
    pub dest_name: String,
    pub dest_address: String,
}

impl TransferRow {
    pub fn kind_name(&self) -> &'static str {
        operation_name(self.summary.kind)
    }
}

#[derive(Debug, Clone, Default)]
pub struct StartReport {
    pub restored: usize,
    pub paused: usize,
}

fn ended_ms(s: &TransferSummary) -> i64 {
    s.finished_ms.unwrap_or(s.created_ms)
}

pub fn date_group_of(s: &TransferSummary, now_ms: i64) -> DateGroup {
    if s.state.is_finished() {
        DateGroup::at(ended_ms(s), now_ms)
    } else {
        DateGroup::InProgress
    }
}

fn needs_you_rank(state: TransferState) -> u8 {
    match state {
        TransferState::Waiting(WaitReason::Question) => 0,
        TransferState::Waiting(_) => 2,
        TransferState::Paused => 3,
        _ => 1,
    }
}

fn order_rows(rows: &mut [TransferRow]) {
    rows.sort_by_key(|r| {
        let s = &r.summary;
        let ended = s.state.is_finished();
        let (rank, tie) = if ended {
            (0, ended_ms(s).saturating_neg())
        } else {
            (needs_you_rank(s.state), s.position)
        };
        (ended, rank, tie, s.id)
    });
}

impl Core {
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

    pub fn pending_at_close(&self) -> usize {
        self.engine.pending_summary()
    }

    pub fn transfer_rows(&self, book: &ProgressBook) -> Vec<TransferRow> {
        let now = crate::org::now_ms();
        let mut rows: Vec<TransferRow> = self
            .engine
            .list()
            .into_iter()
            .map(|s| self.row_of(book, s, now))
            .collect();
        order_rows(&mut rows);
        rows
    }

    fn row_of(&self, book: &ProgressBook, s: TransferSummary, now_ms: i64) -> TransferRow {
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
            date_group: date_group_of(&s, now_ms),
            direction,
            bytes_done,
            rate,
            eta_secs,
            questions,
            dest_name: self.display_title(&s.dest),
            dest_address: self.locations.display_address(&s.dest),
            summary: s,
        }
    }

    pub fn forget_finished(&self, id: TransferId) -> bool {
        self.engine.forget(id).is_ok()
    }

    pub fn clear_history(&self) -> usize {
        self.engine
            .list()
            .iter()
            .filter(|s| s.state.is_finished())
            .filter(|s| self.engine.forget(s.id).is_ok())
            .count()
    }

    pub fn question_list(&self, id: TransferId) -> Result<Vec<serde_json::Value>> {
        let items = self.engine.items(id)?;
        let questions = self.engine.questions(id)?;
        let kind = self.engine.get(id).map_or("copy", |s| kind_name(s.kind));
        let settings = self.settings();
        Ok(questions
            .iter()
            .map(|(seq, c)| {
                let mut about = QuestionAbout {
                    kind,
                    open: questions.len(),
                    ..QuestionAbout::default()
                };
                if let Some(plan) = items.get(*seq as usize).map(|i| &i.plan) {
                    about.name = plan
                        .dst
                        .name()
                        .map(crate::vpath::display_name)
                        .unwrap_or_default();
                    about.source = plan.src.to_string();
                    about.destination = plan.dst.to_string();
                    about.source_look = self.item_look(&plan.src, c.src_is_dir, &settings);
                    about.destination_look = self.item_look(&plan.dst, c.dst_is_dir, &settings);
                }
                conflict_json(id, *seq, &about, c)
            })
            .collect())
    }

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
    fn with_nothing_moving_aggregate_shows_paused_progress() {
        use TransferState::*;
        let mut book = ProgressBook::default();
        let list = vec![
            summary(1, Paused, 10, 1000),
            summary(2, Paused, 0, 200),
            summary(3, Waiting(WaitReason::Network), 40, 100),
            summary(4, Completed, 10, 10),
        ];
        book.update(1, 880, 300, Some(1));
        let a = book.aggregate(&list);
        assert_eq!((a.active, a.paused), (0, 2));
        assert_eq!(a.bytes_done, 880, "paused at its live bytes");
        assert_eq!(a.bytes_total, 1200, "waiting and finished do not count");
        assert_eq!((a.rate, a.eta_secs), (0, None));
    }

    #[test]
    fn finished_transfers_show_stored_bytes_not_live_ones() {
        let mut book = ProgressBook::default();
        book.update(1, 600, 100, Some(7));
        let canceled = summary(1, TransferState::Canceled, 200, 1200);
        assert_eq!(
            book.of(&canceled),
            (200, 0, None),
            "partial file of a cancelled copy is gone"
        );
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
    fn questions_lead_then_running_waiting_paused_in_queue_order_then_ended_newest_first() {
        use TransferState::*;
        let mk = |id, state, position, finished_ms| {
            let mut summary = summary(id, state, 0, 0);
            summary.position = position;
            summary.created_ms = id * 1000;
            summary.finished_ms = finished_ms;
            TransferRow {
                group: group_of(state),
                date_group: DateGroup::InProgress,
                direction: Direction::Local,
                bytes_done: 0,
                rate: 0,
                eta_secs: None,
                questions: 0,
                dest_name: String::new(),
                dest_address: String::new(),
                summary,
            }
        };
        let mut rows = vec![
            mk(1, Completed, 0, Some(5000)),
            mk(2, Paused, 1, None),
            mk(3, Failed, 0, Some(9000)),
            mk(4, Running, 2, None),
            mk(5, Waiting(WaitReason::Bridge), 3, None),
            mk(6, Queued, 4, None),
            mk(7, Canceled, 0, None),
            mk(8, Scanning, 0, None),
            mk(9, Completed, 0, Some(5000)),
            mk(10, Waiting(WaitReason::Question), 5, None),
        ];
        order_rows(&mut rows);
        let ids: Vec<i64> = rows.iter().map(|r| r.summary.id).collect();
        assert_eq!(ids, [10, 8, 4, 6, 5, 2, 3, 7, 1, 9]);
    }

    #[test]
    fn unfinished_transfers_are_in_progress_and_ended_ones_go_by_their_end() {
        use TransferState::*;
        let day = 86_400_000;
        let now = 1_800_000_000_000;
        let unfinished = [
            Queued,
            Scanning,
            Running,
            Paused,
            Waiting(WaitReason::Question),
            Waiting(WaitReason::Network),
            Waiting(WaitReason::Volume),
        ];
        for state in unfinished {
            let mut s = summary(1, state, 0, 0);
            s.created_ms = now - 90 * day;
            assert_eq!(date_group_of(&s, now), DateGroup::InProgress, "{state:?}");
        }
        for state in [Completed, Failed, Canceled] {
            let mut s = summary(1, state, 0, 0);
            s.created_ms = now - 90 * day;
            s.finished_ms = Some(now - 60_000);
            assert_eq!(date_group_of(&s, now), DateGroup::Today, "{state:?}");
            s.finished_ms = Some(now - 45 * day);
            assert_eq!(date_group_of(&s, now), DateGroup::Older, "{state:?}");
            s.finished_ms = None;
            s.created_ms = now - 10 * day;
            assert_eq!(date_group_of(&s, now), DateGroup::LastThirtyDays, "{state:?}");
        }
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
            add_only: true,
        };
        let about = QuestionAbout {
            name: "a.txt".into(),
            kind: "move",
            open: 3,
            ..QuestionAbout::default()
        };
        let j = conflict_json(7, 2, &about, &c);
        assert_eq!(j["transferId"], 7);
        assert_eq!(j["item"], 2);
        assert_eq!(j["name"], "a.txt");
        assert_eq!(j["kind"], "move");
        assert_eq!(j["open"], 3);
        assert_eq!(j["source_look"]["thumbnailSource"], "");
        assert_eq!(j["dst_size"], 4);
        assert!(j["dst_mtime_ms"].is_null());
        assert_eq!(j["choices"], serde_json::json!(["Replace", "KeepBoth"]));
        assert_eq!(j["add_only"], true);
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
