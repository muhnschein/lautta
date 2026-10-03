// SPDX-License-Identifier: LGPL-2.1-or-later
//! User-level actions of the transfers area on [`Core`](crate::app::Core):
//! the grouped transfer list with live progress (XFR-8, §15.4), start-up
//! restore (XFR-11), working copies and their write-back (EDT-1..3).
//! Everything here is Qt-free; the Qt layer turns it into models.

use crate::app::Core;
use crate::error::{Error, ErrorKind, Result};
use crate::locations::LocationRegistry;
use crate::ops::{Conflict, ConflictChoice, OperationKind};
use crate::transfer::model::operation_name;
use crate::transfer::{TransferId, TransferState, TransferSummary, WaitReason};
use crate::uri::Uri;
use crate::watch::FileWatcher;
use crate::workcopy::{
    locally_changed, EditConflict, EditConflictChoice, Purpose, Resolution, WorkingCopy, WorkingCopyId,
};
use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};

/// The groups of the Transfers page (§15.4), in display order.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum TransferGroup {
    Active,
    /// Waiting for the user, the network, the bridge or a card.
    Waiting,
    Paused,
    /// Working copies (EDT-3); never a transfer.
    Edited,
    History,
}

impl TransferGroup {
    pub const ALL: [TransferGroup; 5] = [
        TransferGroup::Active,
        TransferGroup::Waiting,
        TransferGroup::Paused,
        TransferGroup::Edited,
        TransferGroup::History,
    ];

    pub fn name(self) -> &'static str {
        match self {
            TransferGroup::Active => "active",
            TransferGroup::Waiting => "waiting",
            TransferGroup::Paused => "paused",
            TransferGroup::Edited => "edited",
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

/// Engineering names of conflict choices (`answer(id, item, choice, …)`).
pub fn choice_name(c: ConflictChoice) -> &'static str {
    match c {
        ConflictChoice::Replace => "replace",
        ConflictChoice::Skip => "skip",
        ConflictChoice::KeepBoth => "keep_both",
        ConflictChoice::Merge => "merge",
        ConflictChoice::ReplaceIfNewer => "replace_if_newer",
        ConflictChoice::Resume => "resume",
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

pub fn edit_choice_name(c: EditConflictChoice) -> &'static str {
    match c {
        EditConflictChoice::UploadMineAndReplace => "upload_replace",
        EditConflictChoice::SaveMineAsCopy => "save_copy",
        EditConflictChoice::DiscardMine => "discard",
    }
}

pub fn parse_edit_choice(name: &str) -> Option<EditConflictChoice> {
    [
        EditConflictChoice::UploadMineAndReplace,
        EditConflictChoice::SaveMineAsCopy,
        EditConflictChoice::DiscardMine,
    ]
    .into_iter()
    .find(|c| edit_choice_name(*c) == name)
}

/// A conflict question as the UI gets it (camelCase, `choices` by name).
pub fn conflict_json(id: TransferId, item: u32, name: &str, c: &Conflict) -> serde_json::Value {
    serde_json::json!({
        "transferId": id,
        "item": item,
        "name": name,
        "dstIsDir": c.dst_is_dir,
        "srcIsDir": c.src_is_dir,
        "srcSize": c.src_size,
        "dstSize": c.dst_size,
        "srcMtimeMs": c.src_mtime_ms,
        "dstMtimeMs": c.dst_mtime_ms,
        "resumable": c.resumable,
        "choices": c.choices.iter().map(|x| choice_name(*x)).collect::<Vec<_>>(),
    })
}

/// An edit conflict as the UI gets it.
pub fn edit_conflict_json(c: &EditConflict, name: &str, address: &str) -> serde_json::Value {
    serde_json::json!({
        "copyId": c.id,
        "name": name,
        "remote": c.remote.to_string(),
        "address": address,
        "localSize": c.local_size,
        "remoteSize": c.remote_size,
        "remoteMtimeMs": c.remote_mtime_ms,
        "remoteGone": c.remote_size.is_none() && c.remote_mtime_ms.is_none(),
        "choices": c.choices.iter().map(|x| edit_choice_name(*x)).collect::<Vec<_>>(),
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

/// A working copy of the *Edited files* group with what the list needs.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct EditedFile {
    pub copy: WorkingCopy,
    /// The local file changed since the last upload (upload pending).
    pub dirty: bool,
    pub local_size: u64,
}

/// What start-up found (XFR-11, EDT-3).
#[derive(Debug, Clone, Default)]
pub struct StartReport {
    /// Unfinished transfers that came back; paused unless auto-resumed.
    pub restored: usize,
    /// Of those, how many are paused now.
    pub paused: usize,
    /// Edited files changed while the app was closed.
    pub dirty_edits: Vec<WorkingCopy>,
}

/// The outcome of [`Core::write_back`].
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum WriteBack {
    Unchanged,
    Uploaded(WorkingCopy),
    Conflict(EditConflict),
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
    /// Brings back the persisted queue (XFR-11) and looks at the working
    /// copies (EDT-3). `auto_resume` is the setting AND the bridge being
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
        let dirty_edits = self.working_copies.scan_at_start().await.unwrap_or_default();
        Ok(StartReport {
            restored,
            paused,
            dirty_edits,
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

    // ---------------------------------------------------- working copies

    /// The *Edited files* group: edit copies, with whether an upload is
    /// pending (EDT-3). Newest first.
    pub async fn edited_files(&self) -> Result<Vec<EditedFile>> {
        let copies = self.working_copies.list(Some(Purpose::Edit)).await?;
        tokio::task::spawn_blocking(move || {
            let mut out: Vec<EditedFile> = copies.into_iter().map(edited_file).collect();
            out.sort_by_key(|e| std::cmp::Reverse(e.copy.id));
            out
        })
        .await
        .map_err(|e| Error::new(ErrorKind::Internal, e.to_string()))
    }

    /// EDT-2: the local file changed. Uploads when the remote is unchanged,
    /// reports a conflict otherwise.
    pub async fn write_back(&self, id: WorkingCopyId) -> Result<WriteBack> {
        let copy = self.working_copies.get(id).await?;
        let provider = self.provider(&copy.remote.location)?;
        match self.working_copies.on_local_change(provider.as_ref(), id).await? {
            crate::workcopy::Decision::Unchanged => Ok(WriteBack::Unchanged),
            crate::workcopy::Decision::Upload(_) => self
                .working_copies
                .upload(provider.as_ref(), id)
                .await
                .map(WriteBack::Uploaded),
            crate::workcopy::Decision::Conflict(c) => Ok(WriteBack::Conflict(c)),
        }
    }

    /// The conflict of a working copy, when it has one (the dialog asks).
    pub async fn edit_conflict(&self, id: WorkingCopyId) -> Result<Option<EditConflict>> {
        let copy = self.working_copies.get(id).await?;
        let provider = self.provider(&copy.remote.location)?;
        Ok(
            match self.working_copies.on_local_change(provider.as_ref(), id).await? {
                crate::workcopy::Decision::Conflict(c) => Some(c),
                _ => None,
            },
        )
    }

    /// Answers an edit conflict (EDT-2).
    pub async fn resolve_edit_conflict(
        &self,
        id: WorkingCopyId,
        choice: EditConflictChoice,
    ) -> Result<Resolution> {
        let copy = self.working_copies.get(id).await?;
        let provider = self.provider(&copy.remote.location)?;
        self.working_copies.resolve(provider.as_ref(), id, choice).await
    }

    /// Display name of a working copy's remote file.
    pub fn working_copy_name(&self, c: &WorkingCopy) -> String {
        c.remote
            .name()
            .map(crate::vpath::display_name)
            .unwrap_or_default()
    }
}

fn edited_file(copy: WorkingCopy) -> EditedFile {
    match crate::workcopy::local_stamp(&copy.local_path) {
        Ok((size, mtime)) => EditedFile {
            dirty: locally_changed(&copy, size, mtime),
            local_size: size,
            copy,
        },
        Err(_) => EditedFile {
            dirty: false,
            local_size: 0,
            copy,
        },
    }
}

/// Makes the watcher follow the edit copies (EDT-3): new files are watched,
/// files of removed copies are not. Files whose folder is gone are skipped.
pub fn sync_watches(watcher: &FileWatcher, copies: &[WorkingCopy]) {
    let wanted: HashSet<PathBuf> = copies
        .iter()
        .filter(|c| c.purpose == Purpose::Edit)
        .map(|c| c.local_path.clone())
        .collect();
    for gone in watcher
        .watched_files()
        .into_iter()
        .filter(|f| !wanted.contains(f))
    {
        let _ = watcher.unwatch_file(&gone);
    }
    for file in &wanted {
        if let Err(e) = watcher.watch_file(file) {
            log::debug!("not watching a working copy: {e}");
        }
    }
}

/// The working copy a written file belongs to.
pub fn copy_for_path(copies: &[WorkingCopy], path: &Path) -> Option<WorkingCopyId> {
    copies
        .iter()
        .find(|c| c.purpose == Purpose::Edit && c.local_path == path)
        .map(|c| c.id)
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
            ["active", "waiting", "paused", "edited", "history"]
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
        for c in [
            EditConflictChoice::UploadMineAndReplace,
            EditConflictChoice::SaveMineAsCopy,
            EditConflictChoice::DiscardMine,
        ] {
            assert_eq!(parse_edit_choice(edit_choice_name(c)), Some(c));
        }
        assert_eq!(parse_edit_choice("x"), None);
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
    fn conflict_and_edit_conflict_json() {
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
        assert_eq!(j["dstSize"], 4);
        assert!(j["dstMtimeMs"].is_null());
        assert_eq!(j["choices"], serde_json::json!(["replace", "keep_both"]));
        let e = EditConflict {
            id: 3,
            remote: Uri::parse("lautta://srv/a/b.odt").unwrap(),
            local_size: 84,
            remote_size: None,
            remote_mtime_ms: None,
            choices: vec![EditConflictChoice::DiscardMine],
        };
        let j = edit_conflict_json(&e, "b.odt", "sftp://srv/a/b.odt");
        assert_eq!(j["copyId"], 3);
        assert_eq!(j["remoteGone"], true);
        assert_eq!(j["choices"], serde_json::json!(["discard"]));
        assert_eq!(j["remote"], "lautta://srv/a/b.odt");
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

    fn copy(id: i64, path: &str, purpose: Purpose) -> WorkingCopy {
        WorkingCopy {
            id,
            remote: Uri::parse("lautta://srv/x").unwrap(),
            local_path: PathBuf::from(path),
            base_size: Some(1),
            base_mtime_ms: None,
            base_etag: None,
            local_mtime_ms: Some(0),
            pinned: false,
            last_upload_ms: None,
            purpose,
        }
    }

    #[test]
    fn copies_are_found_by_path_and_only_edit_copies() {
        let copies = vec![copy(1, "/a/x", Purpose::Open), copy(2, "/a/y", Purpose::Edit)];
        assert_eq!(copy_for_path(&copies, Path::new("/a/y")), Some(2));
        assert_eq!(copy_for_path(&copies, Path::new("/a/x")), None);
        assert_eq!(copy_for_path(&copies, Path::new("/a/z")), None);
    }

    #[test]
    fn watches_follow_the_edit_copies() {
        let dir = tempfile::tempdir().unwrap();
        let (a, b, open) = (dir.path().join("a"), dir.path().join("b"), dir.path().join("o"));
        for f in [&a, &b, &open] {
            std::fs::write(f, b"x").unwrap();
        }
        let (watcher, _rx) = FileWatcher::new(std::time::Duration::from_millis(50)).unwrap();
        let path = |p: &Path| p.to_str().unwrap().to_owned();
        let both = vec![
            copy(1, &path(&a), Purpose::Edit),
            copy(2, &path(&b), Purpose::Edit),
            copy(3, &path(&open), Purpose::Open),
        ];
        sync_watches(&watcher, &both);
        assert_eq!(watcher.watched_files(), vec![a.clone(), b.clone()]);
        sync_watches(&watcher, &both[1..]);
        assert_eq!(watcher.watched_files(), vec![b.clone()]);
        sync_watches(&watcher, &[]);
        assert!(watcher.watched_files().is_empty());
    }

    #[test]
    fn edited_file_notices_local_changes() {
        let dir = tempfile::tempdir().unwrap();
        let f = dir.path().join("f");
        std::fs::write(&f, b"abc").unwrap();
        let (size, mtime) = crate::workcopy::local_stamp(&f).unwrap();
        let mut c = copy(1, f.to_str().unwrap(), Purpose::Edit);
        c.base_size = Some(size);
        c.local_mtime_ms = Some(mtime);
        let clean = edited_file(c.clone());
        assert!(!clean.dirty);
        assert_eq!(clean.local_size, 3);
        std::fs::write(&f, b"abcdef").unwrap();
        assert!(edited_file(c.clone()).dirty);
        std::fs::remove_file(&f).unwrap();
        let gone = edited_file(c);
        assert!(!gone.dirty && gone.local_size == 0);
    }
}
