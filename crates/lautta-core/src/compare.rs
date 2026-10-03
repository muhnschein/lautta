// SPDX-License-Identifier: LGPL-2.1-or-later
//! Folder comparison and sync planning (SYN-1, SYN-2). Two trees on any two
//! providers are paired by relative path; a sync plan turns the result into
//! copy and delete steps that run as ordinary transfers (SYN-3).
//!
//! Only regular files and folders take part: symlinks and special files are
//! neither compared nor synced. Without a baseline "both sides changed" cannot
//! be told from "one side changed", so same-age different content is reported
//! as [`ItemStatus::Different`] and never acted on by *update both*.

use crate::entry::{Entry, Kind};
use crate::error::{Error, ErrorKind, Result};
use crate::ops::{Conflict, ConflictChoice, OperationKind, Plan, PlanItem, PlanTotals};
use crate::provider::{Lane, Provider, ProviderResolver};
use crate::uri::Uri;
use crate::vpath::VPath;
use sha2::Digest;
use std::collections::{BTreeMap, BTreeSet, VecDeque};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;

/// SYN-1: modification times closer than this are equal (FAT stores 2 s).
pub const MTIME_TOLERANCE_MS: i64 = 2_000;
const DST_SHIFT_MS: i64 = 3_600_000;
const HASH_CHUNK: usize = 1 << 20;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum SyncMode {
    MirrorLeftToRight,
    MirrorRightToLeft,
    /// Newer wins, nothing is deleted (SYN-2).
    UpdateBoth,
}

impl SyncMode {
    pub fn as_str(self) -> &'static str {
        match self {
            SyncMode::MirrorLeftToRight => "mirror_lr",
            SyncMode::MirrorRightToLeft => "mirror_rl",
            SyncMode::UpdateBoth => "update_both",
        }
    }

    pub fn parse(s: &str) -> Option<SyncMode> {
        Some(match s {
            "mirror_lr" => SyncMode::MirrorLeftToRight,
            "mirror_rl" => SyncMode::MirrorRightToLeft,
            "update_both" => SyncMode::UpdateBoth,
            _ => return None,
        })
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Side {
    Left,
    Right,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum ItemStatus {
    Same,
    LeftOnly,
    RightOnly,
    LeftNewer,
    RightNewer,
    /// Content differs but the times do not say which side is newer, or the
    /// two sides are of different types.
    Different,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CompareOptions {
    pub mtime_tolerance_ms: i64,
    /// Also treat a difference of exactly one hour (±2 s) as equal.
    pub dst_tolerance: bool,
    /// Compare content when sizes match.
    pub checksums: bool,
    /// Globs on the relative path or the name; a trailing `/` limits a
    /// pattern to folders.
    pub excludes: Vec<String>,
}

impl Default for CompareOptions {
    fn default() -> Self {
        CompareOptions {
            mtime_tolerance_ms: MTIME_TOLERANCE_MS,
            dst_tolerance: false,
            checksums: false,
            excludes: Vec::new(),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CompareItem {
    pub rel: VPath,
    pub left: Option<Entry>,
    pub right: Option<Entry>,
    pub status: ItemStatus,
}

impl CompareItem {
    fn kind(&self) -> Kind {
        self.left
            .as_ref()
            .or(self.right.as_ref())
            .map_or(Kind::Unknown, |e| e.kind)
    }

    /// A folder on one side and a file on the other.
    pub fn is_type_mismatch(&self) -> bool {
        matches!((&self.left, &self.right), (Some(l), Some(r)) if l.kind != r.kind)
    }
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct CompareCounts {
    pub same: usize,
    pub left_only: usize,
    pub right_only: usize,
    pub left_newer: usize,
    pub right_newer: usize,
    pub different: usize,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CompareResult {
    pub left: Uri,
    pub right: Uri,
    /// Sorted by relative path, parents before their children.
    pub items: Vec<CompareItem>,
}

impl CompareResult {
    pub fn counts(&self) -> CompareCounts {
        let mut c = CompareCounts::default();
        for item in &self.items {
            match item.status {
                ItemStatus::Same => c.same += 1,
                ItemStatus::LeftOnly => c.left_only += 1,
                ItemStatus::RightOnly => c.right_only += 1,
                ItemStatus::LeftNewer => c.left_newer += 1,
                ItemStatus::RightNewer => c.right_newer += 1,
                ItemStatus::Different => c.different += 1,
            }
        }
        c
    }

    pub fn uri(&self, side: Side, rel: &VPath) -> Uri {
        let root = match side {
            Side::Left => &self.left,
            Side::Right => &self.right,
        };
        Uri::new(root.location.clone(), root.path.join_path(rel))
    }
}

/// Compiled exclusion patterns (SYN-2).
pub struct ExcludeSet {
    patterns: Vec<(glob::Pattern, bool)>,
}

impl ExcludeSet {
    pub fn new(globs: &[String]) -> Result<ExcludeSet> {
        let mut patterns = Vec::with_capacity(globs.len());
        for raw in globs {
            let (text, dir_only) = match raw.strip_suffix('/') {
                Some(t) => (t, true),
                None => (raw.as_str(), false),
            };
            let pat = glob::Pattern::new(text)
                .map_err(|_| Error::new(ErrorKind::InvalidArgument, "invalid exclusion pattern"))?;
            patterns.push((pat, dir_only));
        }
        Ok(ExcludeSet { patterns })
    }

    pub fn is_excluded(&self, rel: &VPath, is_dir: bool) -> bool {
        let path = rel.display();
        let name = String::from_utf8_lossy(rel.name().unwrap_or(b"")).into_owned();
        self.patterns
            .iter()
            .any(|(pat, dir_only)| (is_dir || !dir_only) && (pat.matches(&path) || pat.matches(&name)))
    }
}

/// Lists everything below `root` (files and folders only), breadth first.
/// Any listing error fails the walk: a partial tree would show false
/// one-sided items and a mirror would delete real data.
async fn list_tree(
    provider: &dyn Provider,
    root: &VPath,
    excludes: &ExcludeSet,
    cancel: &AtomicBool,
) -> Result<BTreeMap<VPath, Entry>> {
    let mut out = BTreeMap::new();
    let mut queue = VecDeque::from([VPath::root()]);
    while let Some(rel) = queue.pop_front() {
        if cancel.load(Ordering::Relaxed) {
            return Err(Error::kind(ErrorKind::Canceled));
        }
        let dir = root.join_path(&rel);
        for entry in crate::provider::list_all(provider, &dir, Lane::Bulk).await? {
            if !matches!(entry.kind, Kind::File | Kind::Dir) {
                continue;
            }
            let Ok(child) = rel.join(&entry.name) else {
                continue;
            };
            if excludes.is_excluded(&child, entry.kind == Kind::Dir) {
                continue;
            }
            if entry.kind == Kind::Dir {
                queue.push_back(child.clone());
            }
            out.insert(child, entry);
        }
    }
    Ok(out)
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum TimeOrder {
    Same,
    LeftNewer,
    RightNewer,
    Unknown,
}

fn time_order(left: Option<i64>, right: Option<i64>, opts: &CompareOptions) -> TimeOrder {
    let (Some(l), Some(r)) = (left, right) else {
        return TimeOrder::Unknown;
    };
    let diff = l.saturating_sub(r);
    let tol = opts.mtime_tolerance_ms;
    let shifted = (diff.abs() - DST_SHIFT_MS).abs() <= tol;
    if diff.abs() <= tol || (opts.dst_tolerance && shifted) {
        TimeOrder::Same
    } else if diff > 0 {
        TimeOrder::LeftNewer
    } else {
        TimeOrder::RightNewer
    }
}

struct Comparer<'a> {
    left: Arc<dyn Provider>,
    right: Arc<dyn Provider>,
    left_root: &'a VPath,
    right_root: &'a VPath,
    opts: &'a CompareOptions,
}

impl Comparer<'_> {
    async fn status(&self, rel: &VPath, l: &Entry, r: &Entry) -> ItemStatus {
        if l.kind != r.kind {
            return ItemStatus::Different;
        }
        if l.kind == Kind::Dir {
            return ItemStatus::Same;
        }
        let size_equal = l.size == r.size;
        let mut changed = !size_equal;
        if self.opts.checksums && size_equal {
            match self.same_content(rel).await {
                Ok(true) => return ItemStatus::Same,
                Ok(false) => changed = true,
                Err(e) => {
                    log::debug!("checksum comparison failed: {}", e.kind);
                    return ItemStatus::Different;
                }
            }
        }
        match time_order(l.modified_ms(), r.modified_ms(), self.opts) {
            TimeOrder::LeftNewer => ItemStatus::LeftNewer,
            TimeOrder::RightNewer => ItemStatus::RightNewer,
            TimeOrder::Same | TimeOrder::Unknown if changed => ItemStatus::Different,
            TimeOrder::Same | TimeOrder::Unknown => ItemStatus::Same,
        }
    }

    /// Provider-side checksums when both sides share an algorithm, local
    /// hashing through the read handle otherwise (SYN-1).
    async fn same_content(&self, rel: &VPath) -> Result<bool> {
        let lp = self.left_root.join_path(rel);
        let rp = self.right_root.join_path(rel);
        if let Some(algo) = common_algorithm(self.left.as_ref(), self.right.as_ref()) {
            let (a, b) = tokio::join!(self.left.checksum(&lp, &algo), self.right.checksum(&rp, &algo));
            match (a, b) {
                (Ok(a), Ok(b)) => return Ok(a == b),
                (Err(e), _) | (_, Err(e)) if e.kind != ErrorKind::Unsupported => return Err(e),
                _ => {}
            }
        }
        let (a, b) = tokio::join!(
            hash_local(self.left.as_ref(), &lp),
            hash_local(self.right.as_ref(), &rp)
        );
        Ok(a? == b?)
    }
}

fn common_algorithm(a: &dyn Provider, b: &dyn Provider) -> Option<String> {
    let theirs = b.capabilities().checksum_algorithms;
    a.capabilities()
        .checksum_algorithms
        .into_iter()
        .find(|algo| theirs.contains(algo))
}

async fn hash_local(provider: &dyn Provider, path: &VPath) -> Result<Vec<u8>> {
    let handle = provider.open_read(path, Lane::Bulk).await?;
    let mut hasher = sha2::Sha256::new();
    let mut offset = 0u64;
    loop {
        let chunk = handle.read_at(offset, HASH_CHUNK).await?;
        if chunk.is_empty() {
            return Ok(hasher.finalize().to_vec());
        }
        offset += chunk.len() as u64;
        hasher.update(&chunk);
    }
}

/// Compares the trees at `left` and `right` (SYN-1).
pub async fn compare_trees(
    resolver: &dyn ProviderResolver,
    left: &Uri,
    right: &Uri,
    opts: &CompareOptions,
    cancel: &AtomicBool,
) -> Result<CompareResult> {
    let lp = resolver.provider(&left.location)?;
    let rp = resolver.provider(&right.location)?;
    let excludes = ExcludeSet::new(&opts.excludes)?;
    let (lt, rt) = tokio::join!(
        list_tree(lp.as_ref(), &left.path, &excludes, cancel),
        list_tree(rp.as_ref(), &right.path, &excludes, cancel)
    );
    let (mut lt, mut rt) = (lt?, rt?);
    let comparer = Comparer {
        left: lp,
        right: rp,
        left_root: &left.path,
        right_root: &right.path,
        opts,
    };
    let rels: BTreeSet<VPath> = lt.keys().chain(rt.keys()).cloned().collect();
    let mut items = Vec::with_capacity(rels.len());
    for rel in rels {
        if cancel.load(Ordering::Relaxed) {
            return Err(Error::kind(ErrorKind::Canceled));
        }
        let (l, r) = (lt.remove(&rel), rt.remove(&rel));
        let status = match (&l, &r) {
            (Some(l), Some(r)) => comparer.status(&rel, l, r).await,
            (Some(_), None) => ItemStatus::LeftOnly,
            _ => ItemStatus::RightOnly,
        };
        items.push(CompareItem {
            rel,
            left: l,
            right: r,
            status,
        });
    }
    Ok(CompareResult {
        left: left.clone(),
        right: right.clone(),
        items,
    })
}

/// What a copy overwrites at the destination.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Replaced {
    pub size: Option<u64>,
    pub mtime_ms: Option<i64>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CopyStep {
    pub from: Uri,
    pub to: Uri,
    /// The side written to.
    pub dest: Side,
    pub kind: Kind,
    pub size: Option<u64>,
    pub mtime_ms: Option<i64>,
    pub replaces: Option<Replaced>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SyncAction {
    Copy(CopyStep),
    Delete { uri: Uri, side: Side, kind: Kind },
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Step {
    Nothing,
    CopyToRight,
    CopyToLeft,
    DeleteRight,
    DeleteLeft,
}

fn decide(status: ItemStatus, mode: SyncMode) -> Step {
    use ItemStatus as S;
    match (mode, status) {
        (_, S::Same) => Step::Nothing,
        (SyncMode::MirrorLeftToRight, S::RightOnly) => Step::DeleteRight,
        (SyncMode::MirrorLeftToRight, _) => Step::CopyToRight,
        (SyncMode::MirrorRightToLeft, S::LeftOnly) => Step::DeleteLeft,
        (SyncMode::MirrorRightToLeft, _) => Step::CopyToLeft,
        (SyncMode::UpdateBoth, S::LeftOnly | S::LeftNewer) => Step::CopyToRight,
        (SyncMode::UpdateBoth, S::RightOnly | S::RightNewer) => Step::CopyToLeft,
        (SyncMode::UpdateBoth, _) => Step::Nothing,
    }
}

fn copy_action(result: &CompareResult, item: &CompareItem, to_right: bool) -> Option<SyncAction> {
    let (src, dst, from_side, dest) = if to_right {
        (&item.left, &item.right, Side::Left, Side::Right)
    } else {
        (&item.right, &item.left, Side::Right, Side::Left)
    };
    let src = src.as_ref()?;
    Some(SyncAction::Copy(CopyStep {
        from: result.uri(from_side, &item.rel),
        to: result.uri(dest, &item.rel),
        dest,
        kind: src.kind,
        size: src.size,
        mtime_ms: src.modified_ms(),
        replaces: dst.as_ref().map(|d| Replaced {
            size: d.size,
            mtime_ms: d.modified_ms(),
        }),
    }))
}

/// Turns a comparison into steps (SYN-2). Mirror modes make the target equal
/// to the source, deleting extras; *update both* copies the newer side over
/// the older one and never deletes. Different-type pairs and `Different`
/// items in *update both* are left alone. Copies come parents first, deletes
/// last and children first. Items below an excluded path are skipped.
pub fn sync_plan(result: &CompareResult, mode: SyncMode, excluded: &BTreeSet<VPath>) -> Vec<SyncAction> {
    let mut copies = Vec::new();
    let mut deletes = Vec::new();
    for item in &result.items {
        if item.is_type_mismatch() || excluded.iter().any(|x| item.rel.starts_with(x)) {
            continue;
        }
        match decide(item.status, mode) {
            Step::Nothing => {}
            Step::CopyToRight => copies.extend(copy_action(result, item, true)),
            Step::CopyToLeft => copies.extend(copy_action(result, item, false)),
            Step::DeleteRight => deletes.push(delete_action(result, item, Side::Right)),
            Step::DeleteLeft => deletes.push(delete_action(result, item, Side::Left)),
        }
    }
    deletes.reverse();
    copies.extend(deletes);
    copies
}

fn delete_action(result: &CompareResult, item: &CompareItem, side: Side) -> SyncAction {
    SyncAction::Delete {
        uri: result.uri(side, &item.rel),
        side,
        kind: item.kind(),
    }
}

/// Counts shown on the preview sheet.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct SyncPreview {
    pub copy_files: u64,
    pub new_dirs: u64,
    pub replaced: u64,
    pub copy_bytes: u64,
    pub deletes: u64,
}

pub fn preview(actions: &[SyncAction]) -> SyncPreview {
    let mut p = SyncPreview::default();
    for action in actions {
        match action {
            SyncAction::Delete { .. } => p.deletes += 1,
            SyncAction::Copy(step) => {
                if step.kind == Kind::Dir {
                    p.new_dirs += 1;
                } else {
                    p.copy_files += 1;
                    p.copy_bytes += step.size.unwrap_or(0);
                }
                p.replaced += u64::from(step.replaces.is_some());
            }
        }
    }
    p
}

/// OPS-1 thresholds for the summary sheet.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct LargeOpLimits {
    pub items: u64,
    pub bytes: u64,
}

impl Default for LargeOpLimits {
    fn default() -> Self {
        LargeOpLimits {
            items: 1_000,
            bytes: 1 << 30,
        }
    }
}

/// The plans a sync run executes: one copy plan per destination side
/// (`OperationKind::Sync`), then the deletes (`OperationKind::Delete`).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SyncPlans {
    pub copies: Vec<Plan>,
    pub deletes: Option<Plan>,
}

pub fn build_plans(
    actions: &[SyncAction],
    left_root: &Uri,
    right_root: &Uri,
    limits: LargeOpLimits,
) -> SyncPlans {
    let mut to_right = Vec::new();
    let mut to_left = Vec::new();
    let mut deletes = Vec::new();
    let mut delete_side = Side::Right;
    for action in actions {
        match action {
            SyncAction::Copy(step) if step.dest == Side::Right => to_right.push(copy_item(step)),
            SyncAction::Copy(step) => to_left.push(copy_item(step)),
            SyncAction::Delete { uri, side, kind } => {
                delete_side = *side;
                deletes.push(delete_item(uri, *kind));
            }
        }
    }
    let copies = [(to_right, right_root), (to_left, left_root)]
        .into_iter()
        .filter(|(items, _)| !items.is_empty())
        .map(|(items, root)| make_plan(OperationKind::Sync, root, items, limits))
        .collect();
    let delete_root = match delete_side {
        Side::Left => left_root,
        Side::Right => right_root,
    };
    SyncPlans {
        copies,
        deletes: (!deletes.is_empty())
            .then(|| make_plan(OperationKind::Delete, delete_root, deletes, limits)),
    }
}

fn plan_item(src: &Uri, dst: &Uri, kind: Kind, size: Option<u64>) -> PlanItem {
    PlanItem {
        src: src.clone(),
        dst: dst.clone(),
        kind,
        size,
        mtime_ms: None,
        mode: None,
        link_target: None,
        conflict: None,
        proposed_name: None,
        resolution: None,
    }
}

fn copy_item(step: &CopyStep) -> PlanItem {
    let (size, mtime_ms) = (step.size, step.mtime_ms);
    let mut item = plan_item(&step.from, &step.to, step.kind, size);
    item.mtime_ms = mtime_ms;
    if let Some(old) = &step.replaces {
        // The user approved the preview, so replacing is the stated intent.
        item.conflict = Some(Conflict {
            dst_is_dir: false,
            src_is_dir: false,
            src_size: size,
            dst_size: old.size,
            src_mtime_ms: mtime_ms,
            dst_mtime_ms: old.mtime_ms,
            resumable: false,
            choices: vec![ConflictChoice::Replace, ConflictChoice::Skip],
        });
        item.resolution = Some(ConflictChoice::Replace);
    }
    item
}

fn delete_item(uri: &Uri, kind: Kind) -> PlanItem {
    plan_item(uri, uri, kind, None)
}

fn make_plan(kind: OperationKind, destination: &Uri, items: Vec<PlanItem>, limits: LargeOpLimits) -> Plan {
    let mut totals = PlanTotals::default();
    for item in &items {
        if item.kind == Kind::Dir {
            totals.dirs += 1;
        } else {
            totals.files += 1;
            totals.bytes += item.size.unwrap_or(0);
        }
        totals.conflicts += u64::from(item.conflict.is_some());
    }
    Plan {
        kind,
        destination: destination.clone(),
        needs_summary: items.len() as u64 > limits.items || totals.bytes > limits.bytes,
        items,
        totals,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::entry::{cap, Capabilities};
    use crate::provider::memory::MemoryProvider;
    use crate::provider::StaticResolver;

    const T0: i64 = 1_700_000_000_000;

    fn setup() -> (MemoryProvider, MemoryProvider, StaticResolver) {
        let l = MemoryProvider::default();
        let r = MemoryProvider::default();
        let res = StaticResolver::default()
            .with("l", Arc::new(l.clone()))
            .with("r", Arc::new(r.clone()));
        (l, r, res)
    }

    async fn run(res: &StaticResolver, opts: &CompareOptions) -> CompareResult {
        compare_trees(
            res,
            &Uri::root("l"),
            &Uri::root("r"),
            opts,
            &AtomicBool::new(false),
        )
        .await
        .unwrap()
    }

    fn status_of(result: &CompareResult, rel: &str) -> ItemStatus {
        result
            .items
            .iter()
            .find(|i| i.rel.display() == rel)
            .unwrap_or_else(|| panic!("no item {rel}"))
            .status
    }

    fn vp(s: &str) -> VPath {
        VPath::parse(s.as_bytes()).unwrap()
    }

    #[tokio::test]
    async fn classifies_every_status() {
        let (l, r, res) = setup();
        l.add_file("same", b"abc", T0);
        r.add_file("same", b"abc", T0 + 1_500);
        l.add_file("only-l", b"x", T0);
        r.add_file("only-r", b"x", T0);
        l.add_file("l-newer", b"abc", T0 + 60_000);
        r.add_file("l-newer", b"abc", T0);
        l.add_file("r-newer", b"abc", T0);
        r.add_file("r-newer", b"abc", T0 + 60_000);
        l.add_file("size-diff", b"abcd", T0);
        r.add_file("size-diff", b"abc", T0 + 2_000);
        l.add_file("size-newer", b"abcd", T0 + 10_000);
        r.add_file("size-newer", b"abc", T0);
        let result = run(&res, &CompareOptions::default()).await;
        assert_eq!(status_of(&result, "same"), ItemStatus::Same);
        assert_eq!(status_of(&result, "only-l"), ItemStatus::LeftOnly);
        assert_eq!(status_of(&result, "only-r"), ItemStatus::RightOnly);
        assert_eq!(status_of(&result, "l-newer"), ItemStatus::LeftNewer);
        assert_eq!(status_of(&result, "r-newer"), ItemStatus::RightNewer);
        assert_eq!(status_of(&result, "size-diff"), ItemStatus::Different);
        assert_eq!(status_of(&result, "size-newer"), ItemStatus::LeftNewer);
        let c = result.counts();
        assert_eq!((c.same, c.left_only, c.right_only), (1, 1, 1));
        assert_eq!((c.left_newer, c.right_newer, c.different), (2, 1, 1));
    }

    #[tokio::test]
    async fn tolerance_edge_is_exactly_two_seconds() {
        let (l, r, res) = setup();
        l.add_file("a", b"x", T0);
        r.add_file("a", b"x", T0 + 2_000);
        l.add_file("b", b"x", T0);
        r.add_file("b", b"x", T0 + 2_001);
        let result = run(&res, &CompareOptions::default()).await;
        assert_eq!(status_of(&result, "a"), ItemStatus::Same);
        assert_eq!(status_of(&result, "b"), ItemStatus::RightNewer);
    }

    #[tokio::test]
    async fn dst_tolerance_is_opt_in_and_exact() {
        let (l, r, res) = setup();
        l.add_file("hour", b"x", T0);
        r.add_file("hour", b"x", T0 + 3_600_000);
        l.add_file("hour-ish", b"x", T0 + 3_601_500);
        r.add_file("hour-ish", b"x", T0);
        l.add_file("hour-off", b"x", T0 + 3_603_000);
        r.add_file("hour-off", b"x", T0);
        let strict = run(&res, &CompareOptions::default()).await;
        assert_eq!(status_of(&strict, "hour"), ItemStatus::RightNewer);
        let dst = CompareOptions {
            dst_tolerance: true,
            ..CompareOptions::default()
        };
        let result = run(&res, &dst).await;
        assert_eq!(status_of(&result, "hour"), ItemStatus::Same);
        assert_eq!(status_of(&result, "hour-ish"), ItemStatus::Same);
        assert_eq!(status_of(&result, "hour-off"), ItemStatus::LeftNewer);
    }

    #[tokio::test]
    async fn checksums_find_same_size_changes_and_confirm_equal_content() {
        let (l, r, res) = setup();
        l.add_file("changed", b"aaaa", T0);
        r.add_file("changed", b"bbbb", T0);
        l.add_file("touched", b"same", T0);
        r.add_file("touched", b"same", T0 + 600_000);
        let plain = run(&res, &CompareOptions::default()).await;
        assert_eq!(status_of(&plain, "changed"), ItemStatus::Same);
        assert_eq!(status_of(&plain, "touched"), ItemStatus::RightNewer);
        let sums = CompareOptions {
            checksums: true,
            ..CompareOptions::default()
        };
        let result = run(&res, &sums).await;
        assert_eq!(status_of(&result, "changed"), ItemStatus::Different);
        assert_eq!(status_of(&result, "touched"), ItemStatus::Same);
    }

    #[tokio::test]
    async fn checksums_use_the_provider_when_both_share_an_algorithm() {
        let mut caps = Capabilities::with(&[cap::CHECKSUMS]);
        caps.checksum_algorithms = vec!["sha256".into()];
        let l = MemoryProvider::new(caps.clone());
        let r = MemoryProvider::new(caps);
        l.add_file("f", b"1234", T0);
        r.add_file("f", b"1234", T0);
        let res = StaticResolver::default()
            .with("l", Arc::new(l.clone()))
            .with("r", Arc::new(r.clone()));
        let sums = CompareOptions {
            checksums: true,
            ..CompareOptions::default()
        };
        let result = run(&res, &sums).await;
        assert_eq!(status_of(&result, "f"), ItemStatus::Same);
        assert!(l.calls().iter().any(|c| c.starts_with("checksum")));
        assert!(!l.calls().iter().any(|c| c.starts_with("open_read")));
    }

    #[tokio::test]
    async fn checksum_errors_make_the_item_different_not_same() {
        let (l, r, res) = setup();
        l.add_file("f", b"1234", T0);
        r.add_file("f", b"1234", T0);
        l.fail_next("open_read", "f", Error::kind(ErrorKind::PermissionDenied));
        let sums = CompareOptions {
            checksums: true,
            ..CompareOptions::default()
        };
        assert_eq!(status_of(&run(&res, &sums).await, "f"), ItemStatus::Different);
    }

    #[tokio::test]
    async fn excludes_prune_files_and_folders() {
        let (l, r, res) = setup();
        l.add_file("keep.txt", b"x", T0);
        l.add_file("junk.tmp", b"x", T0);
        l.add_file("sub/inner.tmp", b"x", T0);
        l.add_file("build/out.bin", b"x", T0);
        l.add_file("docs/build", b"file named build", T0);
        r.add_dir("sub");
        let opts = CompareOptions {
            excludes: vec!["*.tmp".into(), "build/".into()],
            ..CompareOptions::default()
        };
        let result = run(&res, &opts).await;
        let rels: Vec<String> = result.items.iter().map(|i| i.rel.display()).collect();
        assert_eq!(rels, ["docs", "docs/build", "keep.txt", "sub"]);
    }

    #[test]
    fn bad_pattern_is_invalid_argument() {
        let err = ExcludeSet::new(&["[".to_owned()]).err().unwrap();
        assert_eq!(err.kind, ErrorKind::InvalidArgument);
    }

    #[tokio::test]
    async fn compares_subfolders_and_dirs_and_mismatches() {
        let (l, r, res) = setup();
        l.add_file("a/x", b"1", T0);
        r.add_file("b/x", b"1", T0);
        l.add_file("a/dir/f", b"1", T0);
        r.add_file("b/dir", b"now a file", T0);
        l.add_symlink("a/link", "x");
        let result = compare_trees(
            &res,
            &Uri::parse("lautta://l/a").unwrap(),
            &Uri::parse("lautta://r/b").unwrap(),
            &CompareOptions::default(),
            &AtomicBool::new(false),
        )
        .await
        .unwrap();
        let rels: Vec<String> = result.items.iter().map(|i| i.rel.display()).collect();
        assert_eq!(rels, ["dir", "dir/f", "x"]);
        assert_eq!(status_of(&result, "x"), ItemStatus::Same);
        let dir = &result.items[0];
        assert!(dir.is_type_mismatch());
        assert_eq!(dir.status, ItemStatus::Different);
        assert_eq!(
            result.uri(Side::Right, &vp("dir/f")).to_string(),
            "lautta://r/b/dir/f"
        );
    }

    #[tokio::test]
    async fn listing_errors_and_cancel_abort() {
        let (l, r, res) = setup();
        l.add_file("sub/f", b"x", T0);
        r.add_dir("sub");
        l.fail_next("list", "sub", Error::kind(ErrorKind::ConnectionLost));
        let err = compare_trees(
            &res,
            &Uri::root("l"),
            &Uri::root("r"),
            &CompareOptions::default(),
            &AtomicBool::new(false),
        )
        .await
        .unwrap_err();
        assert_eq!(err.kind, ErrorKind::ConnectionLost);
        let err = compare_trees(
            &res,
            &Uri::root("l"),
            &Uri::root("r"),
            &CompareOptions::default(),
            &AtomicBool::new(true),
        )
        .await
        .unwrap_err();
        assert_eq!(err.kind, ErrorKind::Canceled);
        let err = compare_trees(
            &res,
            &Uri::root("l"),
            &Uri::root("nope"),
            &CompareOptions::default(),
            &AtomicBool::new(false),
        )
        .await
        .unwrap_err();
        assert_eq!(err.kind, ErrorKind::NotFound);
    }

    async fn sample() -> CompareResult {
        let (l, r, res) = setup();
        l.add_file("new.txt", b"new", T0);
        l.add_file("dir/inner", b"12345", T0);
        r.add_file("extra/deep/old", b"x", T0);
        l.add_file("upd", b"newer!", T0 + 90_000);
        r.add_file("upd", b"older", T0);
        l.add_file("rnew", b"a", T0);
        r.add_file("rnew", b"b", T0 + 90_000);
        l.add_file("clash", b"aaa", T0);
        r.add_file("clash", b"bbbb", T0);
        l.add_file("same", b"s", T0);
        r.add_file("same", b"s", T0);
        run(&res, &CompareOptions::default()).await
    }

    fn describe(actions: &[SyncAction]) -> Vec<String> {
        actions
            .iter()
            .map(|a| match a {
                SyncAction::Copy(c) => format!("copy {} -> {}", c.from, c.to),
                SyncAction::Delete { uri, .. } => format!("delete {uri}"),
            })
            .collect()
    }

    #[tokio::test]
    async fn mirror_left_to_right_copies_and_deletes_extras() {
        let result = sample().await;
        let actions = sync_plan(&result, SyncMode::MirrorLeftToRight, &BTreeSet::new());
        assert_eq!(
            describe(&actions),
            [
                "copy lautta://l/clash -> lautta://r/clash",
                "copy lautta://l/dir -> lautta://r/dir",
                "copy lautta://l/dir/inner -> lautta://r/dir/inner",
                "copy lautta://l/new.txt -> lautta://r/new.txt",
                "copy lautta://l/rnew -> lautta://r/rnew",
                "copy lautta://l/upd -> lautta://r/upd",
                "delete lautta://r/extra/deep/old",
                "delete lautta://r/extra/deep",
                "delete lautta://r/extra",
            ]
        );
    }

    #[tokio::test]
    async fn mirror_right_to_left_is_the_mirror_image() {
        let result = sample().await;
        let actions = sync_plan(&result, SyncMode::MirrorRightToLeft, &BTreeSet::new());
        let text = describe(&actions);
        assert!(text.contains(&"copy lautta://r/upd -> lautta://l/upd".to_owned()));
        assert!(text.contains(&"copy lautta://r/extra -> lautta://l/extra".to_owned()));
        assert!(text.contains(&"delete lautta://l/new.txt".to_owned()));
        assert_eq!(text.last().unwrap(), "delete lautta://l/dir");
        assert!(!text.iter().any(|t| t.contains("delete lautta://r/")));
    }

    #[tokio::test]
    async fn update_both_newer_wins_and_never_deletes() {
        let result = sample().await;
        let actions = sync_plan(&result, SyncMode::UpdateBoth, &BTreeSet::new());
        let text = describe(&actions);
        assert!(text.contains(&"copy lautta://l/upd -> lautta://r/upd".to_owned()));
        assert!(text.contains(&"copy lautta://r/rnew -> lautta://l/rnew".to_owned()));
        assert!(text.contains(&"copy lautta://r/extra/deep/old -> lautta://l/extra/deep/old".to_owned()));
        assert!(text.contains(&"copy lautta://l/new.txt -> lautta://r/new.txt".to_owned()));
        assert!(
            !text.iter().any(|t| t.contains("clash")),
            "Different is not acted on"
        );
        assert!(!actions.iter().any(|a| matches!(a, SyncAction::Delete { .. })));
    }

    #[tokio::test]
    async fn excluded_items_and_their_children_are_skipped() {
        let result = sample().await;
        let excluded: BTreeSet<VPath> = [vp("dir"), vp("upd")].into_iter().collect();
        let actions = sync_plan(&result, SyncMode::MirrorLeftToRight, &excluded);
        let text = describe(&actions).join("\n");
        assert!(!text.contains("dir/inner") && !text.contains("lautta://l/dir ") && !text.contains("upd"));
        assert!(text.contains("new.txt"));
    }

    #[tokio::test]
    async fn preview_counts() {
        let result = sample().await;
        let actions = sync_plan(&result, SyncMode::MirrorLeftToRight, &BTreeSet::new());
        let p = preview(&actions);
        assert_eq!(
            p,
            SyncPreview {
                copy_files: 5,
                new_dirs: 1,
                replaced: 3,
                copy_bytes: 3 + 5 + 3 + 1 + 6,
                deletes: 3
            }
        );
    }

    #[tokio::test]
    async fn plans_split_by_destination_and_carry_conflicts() {
        let result = sample().await;
        let (lr, rr) = (Uri::root("l"), Uri::root("r"));
        let mirror = sync_plan(&result, SyncMode::MirrorLeftToRight, &BTreeSet::new());
        let plans = build_plans(&mirror, &lr, &rr, LargeOpLimits::default());
        assert_eq!(plans.copies.len(), 1);
        let copy = &plans.copies[0];
        assert_eq!((copy.kind, &copy.destination), (OperationKind::Sync, &rr));
        assert_eq!(
            (copy.totals.files, copy.totals.dirs, copy.totals.conflicts),
            (5, 1, 3)
        );
        assert_eq!(copy.totals.bytes, 18);
        assert!(!copy.needs_summary);
        let upd = copy
            .items
            .iter()
            .find(|i| i.dst.to_string() == "lautta://r/upd")
            .unwrap();
        assert_eq!(upd.resolution, Some(ConflictChoice::Replace));
        assert_eq!(upd.conflict.as_ref().unwrap().dst_size, Some(5));
        let fresh = copy
            .items
            .iter()
            .find(|i| i.dst.to_string() == "lautta://r/new.txt")
            .unwrap();
        assert!(fresh.conflict.is_none() && fresh.resolution.is_none());
        let del = plans.deletes.unwrap();
        assert_eq!((del.kind, &del.destination), (OperationKind::Delete, &rr));
        assert_eq!(del.items.len(), 3);
        assert_eq!(del.items[0].src, del.items[0].dst);

        let both = sync_plan(&result, SyncMode::UpdateBoth, &BTreeSet::new());
        let plans = build_plans(&both, &lr, &rr, LargeOpLimits::default());
        assert_eq!(plans.copies.len(), 2);
        assert_eq!(plans.copies[0].destination, rr);
        assert_eq!(plans.copies[1].destination, lr);
        assert!(plans.deletes.is_none());
    }

    #[tokio::test]
    async fn large_plans_need_a_summary() {
        let result = sample().await;
        let actions = sync_plan(&result, SyncMode::MirrorLeftToRight, &BTreeSet::new());
        let tiny = LargeOpLimits {
            items: 5,
            bytes: 1 << 30,
        };
        let plans = build_plans(&actions, &Uri::root("l"), &Uri::root("r"), tiny);
        assert!(plans.copies[0].needs_summary);
        assert!(!plans.deletes.unwrap().needs_summary);
        let heavy = LargeOpLimits {
            items: 1_000,
            bytes: 17,
        };
        let plans = build_plans(&actions, &Uri::root("l"), &Uri::root("r"), heavy);
        assert!(plans.copies[0].needs_summary);
    }

    #[test]
    fn mode_names_round_trip() {
        for m in [
            SyncMode::MirrorLeftToRight,
            SyncMode::MirrorRightToLeft,
            SyncMode::UpdateBoth,
        ] {
            assert_eq!(SyncMode::parse(m.as_str()), Some(m));
        }
        assert_eq!(SyncMode::parse("x"), None);
    }

    #[test]
    fn unknown_mtimes_do_not_invent_an_order() {
        let opts = CompareOptions::default();
        assert_eq!(time_order(None, Some(1), &opts), TimeOrder::Unknown);
        assert_eq!(time_order(Some(0), Some(10_000), &opts), TimeOrder::RightNewer);
    }
}
