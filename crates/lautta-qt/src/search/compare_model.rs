// SPDX-License-Identifier: LGPL-2.1-or-later
//! `CompareModel { leftUri, rightUri }` (SYN-1..3): compares two folders,
//! lists what a sync would do grouped by action, lets the user exclude single
//! items, and runs the sync as ordinary transfers.

use crate::json::to_json;
use crate::runtime::{blocking_then, core, spawn_then};
use lautta_core::app::Core;
use lautta_core::app_search::{excludes_text, options_of, pair_spec, parse_excludes};
use lautta_core::compare::{
    sync_plan, CompareItem, CompareOptions, CompareResult, ItemStatus, Side, SyncAction, SyncMode,
};
use lautta_core::entry::{Entry, Kind};
use lautta_core::{Uri, VPath};
use qmetaobject::prelude::*;
use qmetaobject::{QPointer, USER_ROLE};
use std::collections::{BTreeSet, HashMap};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;

/// One listed item: what a sync run would do with it.
struct Row {
    rel: VPath,
    name: String,
    /// `copy_right`, `replace_right`, `delete_right`, the same towards the
    /// left, or `skipped` (differs but nothing is done).
    group: &'static str,
    status: &'static str,
    is_dir: bool,
    size: i64,
    modified: i64,
    excluded: bool,
}

const ROLES: [&str; 8] = [
    "group", "rel", "name", "status", "isDir", "size", "modified", "excluded",
];
const GROUP_ORDER: [&str; 7] = [
    "copy_right",
    "replace_right",
    "delete_right",
    "copy_left",
    "replace_left",
    "delete_left",
    "skipped",
];

/// A saved pair read back from the database.
struct PairData {
    left: Uri,
    right: Uri,
    mode: SyncMode,
    options: CompareOptions,
    label: String,
}

type Failure = (String, String);

enum Outcome {
    Compared(Result<CompareResult, Failure>),
    Synced(Result<usize, Failure>),
    Saved(Result<i64, Failure>),
    Loaded(Result<PairData, Failure>, i64),
}

#[derive(QObject, Default)]
pub struct CompareModel {
    base: qt_base_class!(trait QAbstractListModel),

    leftUri: qt_property!(QString; NOTIFY inputsChanged),
    rightUri: qt_property!(QString; NOTIFY inputsChanged),
    /// `mirror_lr`, `mirror_rl` or `update_both` (SYN-2).
    mode: qt_property!(QString; NOTIFY modeChanged WRITE set_mode),
    modeChanged: qt_signal!(),
    /// Treat a difference of exactly one hour as equal (SYN-1).
    dstTolerance: qt_property!(bool; NOTIFY inputsChanged),
    checksums: qt_property!(bool; NOTIFY inputsChanged),
    /// Exclusion patterns as typed: "*.tmp, .thumbnails/" (SYN-2).
    excludesText: qt_property!(QString; NOTIFY inputsChanged),
    inputsChanged: qt_signal!(),
    /// Id of the saved pair this comparison belongs to, 0 when none.
    pairId: qt_property!(i64; NOTIFY pairChanged),
    pairLabel: qt_property!(QString; NOTIFY pairChanged),
    pairChanged: qt_signal!(),

    running: qt_property!(bool; NOTIFY runningChanged),
    runningChanged: qt_signal!(),
    /// A comparison result is held.
    ready: qt_property!(bool; NOTIFY runningChanged),
    errorKind: qt_property!(QString; NOTIFY runningChanged),
    errorMessage: qt_property!(QString; NOTIFY runningChanged),

    /// Listed items (what a sync would do, plus skipped ones).
    count: qt_property!(i32; NOTIFY resultChanged),
    totalCount: qt_property!(i32; NOTIFY resultChanged),
    sameCount: qt_property!(i32; NOTIFY resultChanged),
    leftOnlyCount: qt_property!(i32; NOTIFY resultChanged),
    rightOnlyCount: qt_property!(i32; NOTIFY resultChanged),
    leftNewerCount: qt_property!(i32; NOTIFY resultChanged),
    rightNewerCount: qt_property!(i32; NOTIFY resultChanged),
    differentCount: qt_property!(i32; NOTIFY resultChanged),
    /// Files to copy, folders to create, replaced files, deletes and bytes
    /// to copy for the current mode and exclusions.
    copyCount: qt_property!(i32; NOTIFY resultChanged),
    newFolderCount: qt_property!(i32; NOTIFY resultChanged),
    replaceCount: qt_property!(i32; NOTIFY resultChanged),
    deleteCount: qt_property!(i32; NOTIFY resultChanged),
    copyBytes: qt_property!(i64; NOTIFY resultChanged),
    /// Items the sync would touch ("Sync 39 items").
    syncCount: qt_property!(i32; NOTIFY resultChanged),
    resultChanged: qt_signal!(),

    compare: qt_method!(fn(&mut self) -> bool),
    cancel: qt_method!(fn(&mut self)),
    toggleExcluded: qt_method!(fn(&mut self, row: i32)),
    syncPreviewJson: qt_method!(fn(&self, mode: QString) -> QString),
    sync: qt_method!(fn(&mut self, mode: QString) -> bool),
    saveAsPair: qt_method!(fn(&mut self, label: QString)),
    loadPair: qt_method!(fn(&mut self, id: i64)),
    parseExcludes: qt_method!(fn(&self, text: QString) -> QString),

    /// `count` transfers were queued.
    syncStarted: qt_signal!(count: i32),
    syncFailed: qt_signal!(kind: QString, message: QString),
    pairSaved: qt_signal!(id: i64),
    pairLoaded: qt_signal!(id: i64),
    pairFailed: qt_signal!(kind: QString, message: QString),

    rows: Vec<Row>,
    result: Option<CompareResult>,
    excluded: BTreeSet<VPath>,
    generation: u64,
    cancel_flag: Option<Arc<AtomicBool>>,
}

impl Drop for CompareModel {
    fn drop(&mut self) {
        if let Some(flag) = &self.cancel_flag {
            flag.store(true, Ordering::Relaxed);
        }
    }
}

impl QAbstractListModel for CompareModel {
    fn row_count(&self) -> i32 {
        self.rows.len() as i32
    }

    fn data(&self, index: QModelIndex, role: i32) -> QVariant {
        let Some(r) = usize::try_from(index.row()).ok().and_then(|i| self.rows.get(i)) else {
            return QVariant::default();
        };
        let text = |s: &str| QVariant::from(QString::from(s));
        match role - USER_ROLE {
            0 => text(r.group),
            1 => text(&r.rel.display()),
            2 => text(&r.name),
            3 => text(r.status),
            4 => QVariant::from(r.is_dir),
            5 => QVariant::from(r.size),
            6 => QVariant::from(r.modified),
            7 => QVariant::from(r.excluded),
            _ => QVariant::default(),
        }
    }

    fn role_names(&self) -> HashMap<i32, QByteArray> {
        ROLES
            .iter()
            .enumerate()
            .map(|(i, n)| (USER_ROLE + i as i32, QByteArray::from(*n)))
            .collect()
    }
}

fn status_name(s: ItemStatus) -> &'static str {
    match s {
        ItemStatus::Same => "same",
        ItemStatus::LeftOnly => "left_only",
        ItemStatus::RightOnly => "right_only",
        ItemStatus::LeftNewer => "left_newer",
        ItemStatus::RightNewer => "right_newer",
        ItemStatus::Different => "different",
    }
}

fn pair_err(e: lautta_core::Error) -> Failure {
    (e.kind.name().to_owned(), e.message)
}

/// Which group each planned action puts its item in, keyed by the URI the
/// action writes to (copies) or removes (deletes).
fn action_groups(actions: &[SyncAction]) -> HashMap<Uri, &'static str> {
    actions
        .iter()
        .map(|action| match action {
            SyncAction::Copy(step) => {
                let group = match (step.dest, step.replaces.is_some()) {
                    (Side::Right, false) => "copy_right",
                    (Side::Right, true) => "replace_right",
                    (Side::Left, false) => "copy_left",
                    (Side::Left, true) => "replace_left",
                };
                (step.to.clone(), group)
            }
            SyncAction::Delete { uri, side, .. } => {
                let group = if *side == Side::Right {
                    "delete_right"
                } else {
                    "delete_left"
                };
                (uri.clone(), group)
            }
        })
        .collect()
}

/// The entry whose size and date the row shows: the source of a copy, the
/// item to be deleted.
fn acting_entry<'a>(item: &'a CompareItem, group: &str) -> Option<&'a Entry> {
    match group {
        "copy_right" | "replace_right" | "delete_left" => item.left.as_ref(),
        "copy_left" | "replace_left" | "delete_right" => item.right.as_ref(),
        _ => item.left.as_ref().or(item.right.as_ref()),
    }
}

impl CompareModel {
    fn uri_of(s: &QString) -> Option<Uri> {
        Uri::parse(&s.to_string()).ok()
    }

    fn sync_mode(&self) -> SyncMode {
        SyncMode::parse(&self.mode.to_string()).unwrap_or(SyncMode::UpdateBoth)
    }

    fn options(&self) -> CompareOptions {
        CompareOptions {
            dst_tolerance: self.dstTolerance,
            checksums: self.checksums,
            excludes: parse_excludes(&self.excludesText.to_string()),
            ..CompareOptions::default()
        }
    }

    fn set_mode(&mut self, mode: QString) {
        self.mode = mode;
        self.modeChanged();
        self.rebuild();
    }

    fn parseExcludes(&self, text: QString) -> QString {
        QString::from(to_json(&parse_excludes(&text.to_string())).as_str())
    }

    fn compare(&mut self) -> bool {
        let (Some(core), Some(left), Some(right)) =
            (core(), Self::uri_of(&self.leftUri), Self::uri_of(&self.rightUri))
        else {
            return false;
        };
        self.cancel();
        self.running = true;
        self.ready = false;
        self.errorKind = QString::default();
        self.errorMessage = QString::default();
        self.runningChanged();
        let options = self.options();
        let flag = Arc::new(AtomicBool::new(false));
        self.cancel_flag = Some(flag.clone());
        let generation = self.generation;
        let me = self.poster(generation);
        spawn_then(
            async move {
                let result = core.compare_folders(&left, &right, &options, &flag).await;
                Outcome::Compared(result.map_err(pair_err))
            },
            me,
        );
        true
    }

    /// The callback that hands an outcome back to this model unless a newer
    /// action replaced it.
    fn poster(&self, generation: u64) -> impl FnOnce(Outcome) + 'static {
        let me = QPointer::from(self);
        move |outcome| {
            if let Some(model) = me.as_pinned() {
                model.borrow_mut().apply(generation, outcome);
            }
        }
    }

    fn cancel(&mut self) {
        if let Some(flag) = self.cancel_flag.take() {
            flag.store(true, Ordering::Relaxed);
        }
        self.generation += 1;
        if self.running {
            self.running = false;
            self.runningChanged();
        }
    }

    fn apply(&mut self, generation: u64, outcome: Outcome) {
        // Saving a pair and loading one are not tied to a comparison run.
        match outcome {
            Outcome::Saved(res) => self.applied_save(res),
            Outcome::Loaded(res, id) => self.applied_load(res, id),
            Outcome::Synced(res) => self.applied_sync(res),
            Outcome::Compared(res) if generation == self.generation => self.applied_compare(res),
            Outcome::Compared(_) => {}
        }
    }

    fn applied_compare(&mut self, res: Result<CompareResult, Failure>) {
        self.running = false;
        self.cancel_flag = None;
        match res {
            Ok(result) => {
                self.excluded.clear();
                self.result = Some(result);
                self.ready = true;
            }
            Err((kind, message)) => {
                self.result = None;
                self.errorKind = QString::from(kind.as_str());
                self.errorMessage = QString::from(message.as_str());
            }
        }
        self.rebuild();
        self.runningChanged();
    }

    fn applied_sync(&mut self, res: Result<usize, Failure>) {
        match res {
            Ok(count) => self.syncStarted(count as i32),
            Err((kind, message)) => {
                self.syncFailed(QString::from(kind.as_str()), QString::from(message.as_str()))
            }
        }
    }

    fn applied_save(&mut self, res: Result<i64, Failure>) {
        match res {
            Ok(id) => {
                self.pairId = id;
                self.pairChanged();
                self.pairSaved(id);
            }
            Err((k, m)) => self.pairFailed(QString::from(k.as_str()), QString::from(m.as_str())),
        }
    }

    fn applied_load(&mut self, res: Result<PairData, Failure>, id: i64) {
        let pair = match res {
            Ok(v) => v,
            Err((k, m)) => {
                self.pairFailed(QString::from(k.as_str()), QString::from(m.as_str()));
                return;
            }
        };
        self.leftUri = QString::from(pair.left.to_string().as_str());
        self.rightUri = QString::from(pair.right.to_string().as_str());
        self.mode = QString::from(pair.mode.as_str());
        self.dstTolerance = pair.options.dst_tolerance;
        self.checksums = pair.options.checksums;
        self.excludesText = QString::from(excludes_text(&pair.options.excludes).as_str());
        self.pairId = id;
        self.pairLabel = QString::from(pair.label.as_str());
        self.inputsChanged();
        self.modeChanged();
        self.pairChanged();
        self.pairLoaded(id);
    }

    fn loadPair(&mut self, id: i64) {
        let Some(core) = core() else { return };
        let me = self.poster(self.generation);
        blocking_then(
            move || {
                let found =
                    core.sync_pairs.get(id).map_err(pair_err).and_then(|p| {
                        p.ok_or_else(|| ("NotFound".to_owned(), "no such sync pair".to_owned()))
                    });
                Outcome::Loaded(
                    found.map(|p| PairData {
                        options: options_of(&p.spec),
                        left: p.spec.left,
                        right: p.spec.right,
                        mode: p.spec.mode,
                        label: p.spec.label,
                    }),
                    id,
                )
            },
            me,
        );
    }

    fn saveAsPair(&mut self, label: QString) {
        let (Some(core), Some(left), Some(right)) =
            (core(), Self::uri_of(&self.leftUri), Self::uri_of(&self.rightUri))
        else {
            return;
        };
        let spec = pair_spec(
            &label.to_string(),
            &left,
            &right,
            self.sync_mode(),
            &self.options(),
        );
        let existing = self.pairId;
        self.pairLabel = label;
        let me = self.poster(self.generation);
        blocking_then(
            move || {
                let stored = if existing > 0 && matches!(core.sync_pairs.get(existing), Ok(Some(_))) {
                    core.sync_pairs.update(existing, &spec).map(|()| existing)
                } else {
                    core.sync_pairs.add(&spec).map(|p| p.id)
                };
                Outcome::Saved(stored.map_err(pair_err))
            },
            me,
        );
    }

    fn actions(&self) -> Vec<SyncAction> {
        match &self.result {
            Some(r) => sync_plan(r, self.sync_mode(), &self.excluded),
            None => Vec::new(),
        }
    }

    fn syncPreviewJson(&self, mode: QString) -> QString {
        let mode = SyncMode::parse(&mode.to_string()).unwrap_or_else(|| self.sync_mode());
        let preview = match &self.result {
            Some(r) => Core::sync_preview(r, mode, &self.excluded),
            None => Default::default(),
        };
        QString::from(
            to_json(&serde_json::json!({
                "copyFiles": preview.copy_files,
                "newFolders": preview.new_dirs,
                "replaced": preview.replaced,
                "copyBytes": preview.copy_bytes,
                "deletes": preview.deletes,
            }))
            .as_str(),
        )
    }

    fn sync(&mut self, mode: QString) -> bool {
        let (Some(core), Some(result)) = (core(), self.result.clone()) else {
            return false;
        };
        if let Some(m) = SyncMode::parse(&mode.to_string()) {
            self.mode = QString::from(m.as_str());
            self.modeChanged();
        }
        let (mode, excluded) = (self.sync_mode(), self.excluded.clone());
        let me = self.poster(self.generation);
        spawn_then(
            async move {
                let ids = core.run_sync(&result, mode, &excluded).await;
                Outcome::Synced(ids.map(|v| v.len()).map_err(pair_err))
            },
            me,
        );
        true
    }

    fn toggleExcluded(&mut self, row: i32) {
        let Some(r) = usize::try_from(row).ok().and_then(|i| self.rows.get(i)) else {
            return;
        };
        let rel = r.rel.clone();
        if !self.excluded.remove(&rel) {
            self.excluded.insert(rel);
        }
        self.rebuild();
    }

    /// Recomputes the listed items and every count from the result, the mode
    /// and the exclusions.
    fn rebuild(&mut self) {
        self.begin_reset_model();
        self.rows = match &self.result {
            Some(result) => build_rows(result, &self.excluded, self.sync_mode()),
            None => Vec::new(),
        };
        self.end_reset_model();
        self.publish_counts();
    }

    fn publish_counts(&mut self) {
        let counts = self
            .result
            .as_ref()
            .map(CompareResult::counts)
            .unwrap_or_default();
        self.count = self.rows.len() as i32;
        self.totalCount = self.result.as_ref().map_or(0, |r| r.items.len() as i32);
        self.sameCount = counts.same as i32;
        self.leftOnlyCount = counts.left_only as i32;
        self.rightOnlyCount = counts.right_only as i32;
        self.leftNewerCount = counts.left_newer as i32;
        self.rightNewerCount = counts.right_newer as i32;
        self.differentCount = counts.different as i32;
        let actions = self.actions();
        let p = lautta_core::compare::preview(&actions);
        self.copyCount = p.copy_files as i32;
        self.newFolderCount = p.new_dirs as i32;
        self.replaceCount = p.replaced as i32;
        self.deleteCount = p.deletes as i32;
        self.copyBytes = i64::try_from(p.copy_bytes).unwrap_or(i64::MAX);
        self.syncCount = actions.len() as i32;
        self.resultChanged();
    }
}

fn build_rows(result: &CompareResult, excluded: &BTreeSet<VPath>, mode: SyncMode) -> Vec<Row> {
    // Excluded items stay listed in the group they would have been in.
    let groups = action_groups(&sync_plan(result, mode, &BTreeSet::new()));
    let mut rows: Vec<Row> = result
        .items
        .iter()
        .filter(|i| i.status != ItemStatus::Same)
        .map(|item| {
            let group = [Side::Right, Side::Left]
                .iter()
                .find_map(|side| groups.get(&result.uri(*side, &item.rel)))
                .copied()
                .unwrap_or("skipped");
            let entry = acting_entry(item, group);
            Row {
                name: item
                    .rel
                    .name()
                    .map(|n| String::from_utf8_lossy(n).into_owned())
                    .unwrap_or_default(),
                rel: item.rel.clone(),
                group,
                status: status_name(item.status),
                is_dir: entry.is_some_and(|e| e.kind == Kind::Dir),
                size: entry
                    .and_then(|e| e.size)
                    .map_or(-1, |s| i64::try_from(s).unwrap_or(i64::MAX)),
                modified: entry.and_then(Entry::modified_ms).unwrap_or(-1),
                excluded: excluded.iter().any(|x| item.rel.starts_with(x)),
            }
        })
        .collect();
    // Stable: items stay in path order inside a group.
    rows.sort_by_key(|r| {
        GROUP_ORDER
            .iter()
            .position(|g| *g == r.group)
            .unwrap_or(GROUP_ORDER.len())
    });
    rows
}
