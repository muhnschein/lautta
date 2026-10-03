// SPDX-License-Identifier: LGPL-2.1-or-later
//! Conflict choices and resolution (OPS-2).
//!
//! Default choice: never *Replace*. Folders default to *Merge*, everything
//! else to *Keep both* (it never loses data and needs no timestamps).

use super::names::{keep_both_for, NameRules};
use super::{Conflict, ConflictChoice, Plan, PlanItem};
use crate::entry::Kind;
use crate::error::{Error, ErrorKind, Result};
use crate::provider::{list_all, Lane, ProviderResolver};
use crate::uri::Uri;
use std::collections::{HashMap, HashSet};

/// What one side of a conflict looks like.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Side {
    pub is_dir: bool,
    pub size: Option<u64>,
    pub mtime_ms: Option<i64>,
}

/// Facts about the destination that decide which choices are offered.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct ConflictContext {
    /// The destination can append to a partial file (cap `ResumeUpload`, or local).
    pub dest_can_resume: bool,
    /// The source and the destination are the very same item (copy into its own folder).
    pub same_item: bool,
}

/// Builds the conflict for `src` landing on `dst`, with the applicable choices.
pub fn make_conflict(src: Side, dst: Side, ctx: ConflictContext) -> Conflict {
    let resumable = !ctx.same_item && is_resumable(src, dst, ctx.dest_can_resume);
    let mut conflict = Conflict {
        dst_is_dir: dst.is_dir,
        src_is_dir: src.is_dir,
        src_size: src.size,
        dst_size: dst.size,
        src_mtime_ms: src.mtime_ms,
        dst_mtime_ms: dst.mtime_ms,
        resumable,
        choices: Vec::new(),
    };
    conflict.choices = choices_for(&conflict, ctx.same_item);
    conflict
}

fn is_resumable(src: Side, dst: Side, dest_can_resume: bool) -> bool {
    if !dest_can_resume || src.is_dir || dst.is_dir {
        return false;
    }
    matches!((src.size, dst.size), (Some(s), Some(d)) if d < s)
}

/// The choices that make sense, in display order (OPS-2).
fn choices_for(c: &Conflict, same_item: bool) -> Vec<ConflictChoice> {
    use ConflictChoice::{KeepBoth, Merge, Replace, ReplaceIfNewer, Resume, Skip};
    if same_item {
        return vec![KeepBoth, Skip];
    }
    if c.src_is_dir && c.dst_is_dir {
        return vec![Merge, KeepBoth, Skip, Replace];
    }
    let mut out = Vec::new();
    if c.resumable {
        out.push(Resume);
    }
    out.extend([KeepBoth, Skip]);
    let both_files = !c.src_is_dir && !c.dst_is_dir;
    if both_files && c.src_mtime_ms.is_some() && c.dst_mtime_ms.is_some() {
        out.push(ReplaceIfNewer);
    }
    out.push(Replace);
    out
}

/// The choice preselected in the dialog; never *Replace*.
pub fn default_choice(c: &Conflict) -> ConflictChoice {
    if c.src_is_dir && c.dst_is_dir && c.choices.contains(&ConflictChoice::Merge) {
        ConflictChoice::Merge
    } else {
        ConflictChoice::KeepBoth
    }
}

/// Conflicts of one type are resolved together by "apply to all remaining".
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum ConflictType {
    FileOnFile,
    FolderOnFolder,
    /// A file meets a folder or the other way round.
    Mismatch,
    /// An item copied onto itself (no *Replace* offered).
    SameItem,
}

impl ConflictType {
    pub fn of(c: &Conflict) -> ConflictType {
        if !c.choices.contains(&ConflictChoice::Replace) {
            ConflictType::SameItem
        } else if c.src_is_dir && c.dst_is_dir {
            ConflictType::FolderOnFolder
        } else if !c.src_is_dir && !c.dst_is_dir {
            ConflictType::FileOnFile
        } else {
            ConflictType::Mismatch
        }
    }
}

/// Remembers "apply to all remaining" choices per conflict type.
#[derive(Debug, Clone, Default)]
pub struct ConflictPolicy {
    remembered: HashMap<ConflictType, ConflictChoice>,
}

impl ConflictPolicy {
    pub fn new() -> ConflictPolicy {
        ConflictPolicy::default()
    }

    /// Remembers `choice` for every later conflict of the same type as `c`.
    pub fn remember(&mut self, c: &Conflict, choice: ConflictChoice) {
        self.remembered.insert(ConflictType::of(c), choice);
    }

    /// The remembered choice for `c`, only when it is applicable to it (a
    /// remembered *Resume* does not apply to a file that is not a prefix).
    pub fn choice_for(&self, c: &Conflict) -> Option<ConflictChoice> {
        self.remembered
            .get(&ConflictType::of(c))
            .copied()
            .filter(|choice| c.choices.contains(choice))
    }

    pub fn clear(&mut self) {
        self.remembered.clear();
    }
}

/// Conflicts that still wait for an answer.
pub fn unresolved(plan: &Plan) -> Vec<usize> {
    plan.items
        .iter()
        .enumerate()
        .filter(|(_, it)| it.conflict.is_some() && it.resolution.is_none())
        .map(|(i, _)| i)
        .collect()
}

/// Applies `choice` to the conflict of item `index`; with `apply_to_all` also
/// to every other unresolved conflict of the same type for which the choice
/// is applicable. Returns the number of items resolved by this call.
///
/// A choice on a folder carries to its children: *Skip* and *Replace* mark
/// them (their own conflicts disappear into the folder's), *Keep both* moves
/// the whole subtree to the new name and clears the children's conflicts, and
/// *Merge* leaves them to be answered one by one.
pub async fn resolve(
    plan: &mut Plan,
    index: usize,
    choice: ConflictChoice,
    apply_to_all: bool,
    resolver: &dyn ProviderResolver,
) -> Result<usize> {
    let conflict = conflict_of(plan, index)?;
    if !conflict.choices.contains(&choice) {
        return Err(Error::new(
            ErrorKind::InvalidArgument,
            "choice not applicable to this conflict",
        ));
    }
    let mut policy = ConflictPolicy::new();
    let mut names = NameCache::default();
    let mut done = apply_one(plan, index, choice, resolver, &mut names).await?;
    if apply_to_all {
        policy.remember(&conflict, choice);
        for i in unresolved(plan) {
            let Some(c) = plan.items[i].conflict.clone() else {
                continue;
            };
            if let Some(ch) = policy.choice_for(&c) {
                done += apply_one(plan, i, ch, resolver, &mut names).await?;
            }
        }
    }
    Ok(done)
}

fn conflict_of(plan: &Plan, index: usize) -> Result<Conflict> {
    plan.items
        .get(index)
        .and_then(|it| it.conflict.clone())
        .ok_or_else(|| Error::new(ErrorKind::InvalidArgument, "item has no conflict"))
}

async fn apply_one(
    plan: &mut Plan,
    index: usize,
    choice: ConflictChoice,
    resolver: &dyn ProviderResolver,
    names: &mut NameCache,
) -> Result<usize> {
    if plan.items[index].resolution.is_some() {
        return Ok(0);
    }
    let old_dst = plan.items[index].dst.clone();
    let end = subtree_end(&plan.items, index);
    plan.items[index].resolution = Some(choice);
    match choice {
        ConflictChoice::KeepBoth => {
            let new_dst = keep_both_dst(plan, index, resolver, names).await?;
            move_subtree(&mut plan.items[index..end], &old_dst, &new_dst);
        }
        ConflictChoice::Skip | ConflictChoice::Replace => {
            mark_children(&mut plan.items[index + 1..end], choice)
        }
        _ => {}
    }
    Ok(1)
}

/// Exclusive end of the items below `index` (parents come first, so the
/// children of a folder follow it directly and share its destination prefix).
fn subtree_end(items: &[PlanItem], index: usize) -> usize {
    if items[index].kind != Kind::Dir {
        return index + 1;
    }
    let root = &items[index].dst;
    let below = items[index + 1..]
        .iter()
        .take_while(|it| it.dst.is_inside(root))
        .count();
    index + 1 + below
}

fn mark_children(children: &mut [PlanItem], choice: ConflictChoice) {
    for child in children {
        if child.conflict.is_some() && child.resolution.is_none() {
            child.resolution = Some(choice);
        }
    }
}

fn move_subtree(subtree: &mut [PlanItem], old_root: &Uri, new_root: &Uri) {
    for (i, item) in subtree.iter_mut().enumerate() {
        if i == 0 {
            item.dst = new_root.clone();
            continue;
        }
        if let Some(rel) = item.dst.path.strip_prefix(&old_root.path) {
            item.dst = Uri::new(new_root.location.clone(), new_root.path.join_path(&rel));
        }
        item.conflict = None;
        item.resolution = None;
    }
}

/// Names present in a destination folder, normalised for the comparison.
#[derive(Default)]
struct NameCache {
    folders: HashMap<Uri, HashSet<Vec<u8>>>,
}

fn normalise(name: &[u8], case_insensitive: bool) -> Vec<u8> {
    if case_insensitive {
        String::from_utf8_lossy(name).to_lowercase().into_bytes()
    } else {
        name.to_vec()
    }
}

async fn keep_both_dst(
    plan: &Plan,
    index: usize,
    resolver: &dyn ProviderResolver,
    cache: &mut NameCache,
) -> Result<Uri> {
    let item = &plan.items[index];
    let (parent, name) = match (item.dst.parent(), item.dst.name()) {
        (Some(p), Some(n)) => (p, n.to_vec()),
        _ => return Err(Error::new(ErrorKind::InvalidArgument, "destination has no name")),
    };
    let provider = resolver.provider(&parent.location)?;
    let caps = provider.capabilities();
    let rules = NameRules::from_capabilities(&caps);
    if !cache.folders.contains_key(&parent) {
        let mut set = existing_names(&*provider, &parent, rules).await?;
        for it in &plan.items {
            if it.dst.parent().as_ref() == Some(&parent) {
                set.extend(it.dst.name().map(|n| normalise(n, rules.case_insensitive)));
            }
        }
        cache.folders.insert(parent.clone(), set);
    }
    let set = cache.folders.entry(parent.clone()).or_default();
    let is_dir = item.kind == Kind::Dir;
    let taken = |n: &[u8]| set.contains(&normalise(n, rules.case_insensitive));
    let fresh = keep_both_for(&name, is_dir, &taken);
    set.insert(normalise(&fresh, rules.case_insensitive));
    parent.join(&fresh)
}

async fn existing_names(
    provider: &dyn crate::provider::Provider,
    parent: &Uri,
    rules: NameRules,
) -> Result<HashSet<Vec<u8>>> {
    match list_all(provider, &parent.path, Lane::Bulk).await {
        Ok(entries) => Ok(entries
            .iter()
            .map(|e| normalise(&e.name, rules.case_insensitive))
            .collect()),
        Err(e) if matches!(e.kind, ErrorKind::NotFound | ErrorKind::NotADirectory) => Ok(HashSet::new()),
        Err(e) => Err(e),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::entry::{cap, Capabilities};
    use crate::ops::{OperationKind, PlanTotals};
    use crate::provider::memory::MemoryProvider;
    use crate::provider::StaticResolver;
    use crate::vpath::VPath;
    use std::sync::Arc;

    fn file(size: u64, mtime: i64) -> Side {
        Side {
            is_dir: false,
            size: Some(size),
            mtime_ms: Some(mtime),
        }
    }

    const DIR: Side = Side {
        is_dir: true,
        size: None,
        mtime_ms: None,
    };

    fn ctx(resume: bool) -> ConflictContext {
        ConflictContext {
            dest_can_resume: resume,
            same_item: false,
        }
    }

    use ConflictChoice::{KeepBoth, Merge, Replace, ReplaceIfNewer, Resume, Skip};

    #[test]
    fn file_on_file_choices() {
        let c = make_conflict(file(10, 5), file(10, 1), ctx(true));
        assert_eq!(c.choices, vec![KeepBoth, Skip, ReplaceIfNewer, Replace]);
        assert!(!c.resumable);
        assert_eq!(c.src_size, Some(10));
        assert_eq!(c.dst_mtime_ms, Some(1));
        assert!(!c.choices.contains(&Merge));
    }

    #[test]
    fn replace_if_newer_needs_both_mtimes() {
        let mut dst = file(10, 1);
        dst.mtime_ms = None;
        let c = make_conflict(file(10, 5), dst, ctx(false));
        assert!(!c.choices.contains(&ReplaceIfNewer));
        let mut src = file(10, 5);
        src.mtime_ms = None;
        let c = make_conflict(src, file(10, 1), ctx(false));
        assert!(!c.choices.contains(&ReplaceIfNewer));
    }

    #[test]
    fn resume_only_for_smaller_partial_on_capable_destination() {
        let c = make_conflict(file(100, 5), file(40, 1), ctx(true));
        assert!(c.resumable);
        assert_eq!(c.choices[0], Resume);
        let c = make_conflict(file(100, 5), file(40, 1), ctx(false));
        assert!(!c.resumable && !c.choices.contains(&Resume));
        let c = make_conflict(file(100, 5), file(100, 1), ctx(true));
        assert!(!c.choices.contains(&Resume));
        let c = make_conflict(file(100, 5), file(140, 1), ctx(true));
        assert!(!c.choices.contains(&Resume));
        let mut unknown = file(1, 1);
        unknown.size = None;
        assert!(!make_conflict(file(100, 5), unknown, ctx(true)).resumable);
        assert!(!make_conflict(unknown, file(1, 1), ctx(true)).resumable);
    }

    #[test]
    fn folder_and_mismatch_choices() {
        let c = make_conflict(DIR, DIR, ctx(true));
        assert_eq!(c.choices, vec![Merge, KeepBoth, Skip, Replace]);
        let c = make_conflict(DIR, file(1, 1), ctx(true));
        assert_eq!(c.choices, vec![KeepBoth, Skip, Replace]);
        let c = make_conflict(file(1, 1), DIR, ctx(true));
        assert_eq!(c.choices, vec![KeepBoth, Skip, Replace]);
        assert!(!c.resumable);
    }

    #[test]
    fn same_item_offers_no_replace() {
        let c = make_conflict(
            file(100, 5),
            file(40, 1),
            ConflictContext {
                dest_can_resume: true,
                same_item: true,
            },
        );
        assert_eq!(c.choices, vec![KeepBoth, Skip]);
        assert!(!c.resumable);
        assert_eq!(ConflictType::of(&c), ConflictType::SameItem);
    }

    #[test]
    fn defaults_are_never_replace() {
        let d = make_conflict(DIR, DIR, ctx(false));
        assert_eq!(default_choice(&d), Merge);
        let f = make_conflict(file(1, 1), file(1, 1), ctx(false));
        assert_eq!(default_choice(&f), KeepBoth);
        let m = make_conflict(DIR, file(1, 1), ctx(false));
        assert_eq!(default_choice(&m), KeepBoth);
        for c in [d, f, m] {
            assert_ne!(default_choice(&c), Replace);
        }
        let mut odd = make_conflict(DIR, DIR, ctx(false));
        odd.choices.retain(|c| *c != Merge);
        assert_eq!(default_choice(&odd), KeepBoth);
    }

    #[test]
    fn conflict_types() {
        assert_eq!(
            ConflictType::of(&make_conflict(file(1, 1), file(1, 1), ctx(false))),
            ConflictType::FileOnFile
        );
        assert_eq!(
            ConflictType::of(&make_conflict(DIR, DIR, ctx(false))),
            ConflictType::FolderOnFolder
        );
        assert_eq!(
            ConflictType::of(&make_conflict(DIR, file(1, 1), ctx(false))),
            ConflictType::Mismatch
        );
        assert_eq!(
            ConflictType::of(&make_conflict(file(1, 1), DIR, ctx(false))),
            ConflictType::Mismatch
        );
    }

    #[test]
    fn policy_applies_per_type_and_only_when_applicable() {
        let mut p = ConflictPolicy::new();
        let ff = make_conflict(file(1, 1), file(1, 1), ctx(false));
        let dd = make_conflict(DIR, DIR, ctx(false));
        assert_eq!(p.choice_for(&ff), None);
        p.remember(&ff, Skip);
        assert_eq!(p.choice_for(&ff), Some(Skip));
        assert_eq!(p.choice_for(&dd), None);
        p.remember(&dd, Merge);
        assert_eq!(p.choice_for(&dd), Some(Merge));
        p.remember(&ff, Resume);
        assert_eq!(p.choice_for(&ff), None);
        p.clear();
        assert_eq!(p.choice_for(&dd), None);
    }

    fn uri(path: &str) -> Uri {
        Uri::new("m", VPath::parse(path.as_bytes()).unwrap())
    }

    fn item(src: &str, dst: &str, kind: Kind, conflict: Option<Conflict>) -> PlanItem {
        PlanItem {
            src: uri(src),
            dst: uri(dst),
            kind,
            size: None,
            mtime_ms: None,
            link_target: None,
            mode: None,
            conflict,
            proposed_name: None,
            resolution: None,
        }
    }

    fn plan_of(items: Vec<PlanItem>) -> Plan {
        Plan {
            kind: OperationKind::Copy,
            destination: uri("dst"),
            items,
            totals: PlanTotals::default(),
            needs_summary: false,
        }
    }

    fn ff() -> Option<Conflict> {
        Some(make_conflict(file(1, 1), file(1, 1), ctx(false)))
    }

    fn dd() -> Option<Conflict> {
        Some(make_conflict(DIR, DIR, ctx(false)))
    }

    fn resolver(mem: &MemoryProvider) -> StaticResolver {
        StaticResolver::default().with("m", Arc::new(mem.clone()))
    }

    #[tokio::test]
    async fn rejects_unapplicable_choice_and_missing_conflict() {
        let mem = MemoryProvider::default();
        let r = resolver(&mem);
        let mut plan = plan_of(vec![
            item("s/a", "dst/a", Kind::File, ff()),
            item("s/b", "dst/b", Kind::File, None),
        ]);
        let err = resolve(&mut plan, 0, Merge, false, &r).await.unwrap_err();
        assert_eq!(err.kind, ErrorKind::InvalidArgument);
        let err = resolve(&mut plan, 1, Skip, false, &r).await.unwrap_err();
        assert_eq!(err.kind, ErrorKind::InvalidArgument);
        let err = resolve(&mut plan, 9, Skip, false, &r).await.unwrap_err();
        assert_eq!(err.kind, ErrorKind::InvalidArgument);
        assert!(plan.items[0].resolution.is_none());
    }

    #[tokio::test]
    async fn skip_resolves_only_that_item_without_apply_to_all() {
        let mem = MemoryProvider::default();
        let r = resolver(&mem);
        let mut plan = plan_of(vec![
            item("s/a", "dst/a", Kind::File, ff()),
            item("s/b", "dst/b", Kind::File, ff()),
        ]);
        assert_eq!(unresolved(&plan), vec![0, 1]);
        assert_eq!(resolve(&mut plan, 0, Skip, false, &r).await.unwrap(), 1);
        assert_eq!(plan.items[0].resolution, Some(Skip));
        assert_eq!(plan.items[1].resolution, None);
        assert_eq!(unresolved(&plan), vec![1]);
        // Resolving twice does nothing more.
        assert_eq!(resolve(&mut plan, 0, Skip, false, &r).await.unwrap(), 0);
    }

    #[tokio::test]
    async fn apply_to_all_covers_same_type_only() {
        let mem = MemoryProvider::default();
        let r = resolver(&mem);
        let mut plan = plan_of(vec![
            item("s/a", "dst/a", Kind::File, ff()),
            item("s/d", "dst/d", Kind::Dir, dd()),
            item("s/b", "dst/b", Kind::File, ff()),
            item("s/c", "dst/c", Kind::File, None),
        ]);
        assert_eq!(resolve(&mut plan, 0, Replace, true, &r).await.unwrap(), 2);
        assert_eq!(plan.items[0].resolution, Some(Replace));
        assert_eq!(plan.items[1].resolution, None);
        assert_eq!(plan.items[2].resolution, Some(Replace));
        assert_eq!(plan.items[3].resolution, None);
    }

    #[tokio::test]
    async fn apply_to_all_skips_inapplicable_choice() {
        let mem = MemoryProvider::default();
        let r = resolver(&mem);
        let resumable = Some(make_conflict(file(100, 1), file(10, 1), ctx(true)));
        let mut plan = plan_of(vec![
            item("s/a", "dst/a", Kind::File, resumable),
            item("s/b", "dst/b", Kind::File, ff()),
        ]);
        assert_eq!(resolve(&mut plan, 0, Resume, true, &r).await.unwrap(), 1);
        assert_eq!(plan.items[1].resolution, None);
    }

    #[tokio::test]
    async fn keep_both_avoids_existing_and_planned_names() {
        let mem = MemoryProvider::default();
        mem.add_file("dst/a.txt", b"x", 0);
        mem.add_file("dst/a 2.txt", b"x", 0);
        let r = resolver(&mem);
        let mut plan = plan_of(vec![
            item("s/a.txt", "dst/a.txt", Kind::File, ff()),
            item("t/a 3.txt", "dst/a 3.txt", Kind::File, None),
        ]);
        resolve(&mut plan, 0, KeepBoth, false, &r).await.unwrap();
        assert_eq!(plan.items[0].dst, uri("dst/a 4.txt"));
        assert_eq!(plan.items[0].resolution, Some(KeepBoth));
        assert_eq!(plan.items[1].dst, uri("dst/a 3.txt"));
    }

    #[tokio::test]
    async fn keep_both_for_many_gets_distinct_names() {
        let mem = MemoryProvider::default();
        mem.add_file("dst/a.txt", b"x", 0);
        let r = resolver(&mem);
        let mut plan = plan_of(vec![
            item("s/a.txt", "dst/a.txt", Kind::File, ff()),
            item("t/a.txt", "dst/a.txt", Kind::File, ff()),
        ]);
        assert_eq!(resolve(&mut plan, 0, KeepBoth, true, &r).await.unwrap(), 2);
        assert_eq!(plan.items[0].dst, uri("dst/a 2.txt"));
        assert_eq!(plan.items[1].dst, uri("dst/a 3.txt"));
    }

    #[tokio::test]
    async fn keep_both_is_case_insensitive_where_the_destination_is() {
        let caps = Capabilities::with(&[cap::WRITE, cap::CASE_INSENSITIVE]);
        let mem = MemoryProvider::new(caps);
        mem.add_file("dst/A.txt", b"x", 0);
        mem.add_file("dst/a 2.TXT", b"x", 0);
        let r = resolver(&mem);
        let mut plan = plan_of(vec![item("s/a.txt", "dst/A.txt", Kind::File, ff())]);
        resolve(&mut plan, 0, KeepBoth, false, &r).await.unwrap();
        assert_eq!(plan.items[0].dst, uri("dst/A 3.txt"));
    }

    #[tokio::test]
    async fn keep_both_on_missing_destination_folder_uses_plan_names() {
        let mem = MemoryProvider::default();
        let r = resolver(&mem);
        let mut plan = plan_of(vec![
            item("s/a", "gone/a", Kind::File, ff()),
            item("s/b", "gone/a 2", Kind::File, None),
        ]);
        resolve(&mut plan, 0, KeepBoth, false, &r).await.unwrap();
        assert_eq!(plan.items[0].dst, uri("gone/a 3"));
    }

    #[tokio::test]
    async fn keep_both_folder_moves_subtree_and_clears_child_conflicts() {
        let mem = MemoryProvider::default();
        mem.add_dir("dst/d");
        let r = resolver(&mem);
        let mut plan = plan_of(vec![
            item("s/d", "dst/d", Kind::Dir, dd()),
            item("s/d/x", "dst/d/x", Kind::File, ff()),
            item("s/d/sub", "dst/d/sub", Kind::Dir, dd()),
            item("s/d/sub/y", "dst/d/sub/y", Kind::File, None),
            item("s/dd", "dst/dd", Kind::File, None),
        ]);
        assert_eq!(resolve(&mut plan, 0, KeepBoth, false, &r).await.unwrap(), 1);
        let dsts: Vec<String> = plan.items.iter().map(|i| i.dst.path.display()).collect();
        assert_eq!(
            dsts,
            vec!["dst/d 2", "dst/d 2/x", "dst/d 2/sub", "dst/d 2/sub/y", "dst/dd"]
        );
        assert!(plan.items[1].conflict.is_none());
        assert!(plan.items[2].conflict.is_none());
        assert_eq!(plan.items[0].resolution, Some(KeepBoth));
        assert!(unresolved(&plan).is_empty());
    }

    #[tokio::test]
    async fn skip_and_replace_on_folder_mark_children() {
        let mem = MemoryProvider::default();
        let r = resolver(&mem);
        for choice in [Skip, Replace] {
            let mut plan = plan_of(vec![
                item("s/d", "dst/d", Kind::Dir, dd()),
                item("s/d/x", "dst/d/x", Kind::File, ff()),
                item("s/d/y", "dst/d/y", Kind::File, None),
                item("s/e", "dst/e", Kind::File, ff()),
            ]);
            resolve(&mut plan, 0, choice, false, &r).await.unwrap();
            assert_eq!(plan.items[1].resolution, Some(choice));
            assert_eq!(plan.items[2].resolution, None);
            assert_eq!(plan.items[3].resolution, None);
            assert!(plan.items[1].conflict.is_some());
        }
    }

    #[tokio::test]
    async fn merge_leaves_children_for_later() {
        let mem = MemoryProvider::default();
        let r = resolver(&mem);
        let mut plan = plan_of(vec![
            item("s/d", "dst/d", Kind::Dir, dd()),
            item("s/d/x", "dst/d/x", Kind::File, ff()),
        ]);
        resolve(&mut plan, 0, Merge, false, &r).await.unwrap();
        assert_eq!(plan.items[0].resolution, Some(Merge));
        assert_eq!(unresolved(&plan), vec![1]);
    }

    #[test]
    fn subtree_end_for_files_and_dirs() {
        let items = vec![
            item("s/d", "dst/d", Kind::Dir, None),
            item("s/d/x", "dst/d/x", Kind::File, None),
            item("s/dd", "dst/dd", Kind::Dir, None),
        ];
        assert_eq!(subtree_end(&items, 0), 2);
        assert_eq!(subtree_end(&items, 1), 2);
        assert_eq!(subtree_end(&items, 2), 3);
    }
}
