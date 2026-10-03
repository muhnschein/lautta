// SPDX-License-Identifier: LGPL-2.1-or-later
//! Planning (OPS-1): recursive scan, totals, conflicts, symlinks (OPS-6) and
//! destination names (OPS-7).
//!
//! Item order: copy and move plans list a folder before its children
//! (parents first, so the engine can create folders as it goes); delete plans
//! list children before their folder. A move across locations is planned like
//! a copy; the engine deletes each source after its copy is verified (OPS-4)
//! and the source folders last, in reverse order.
//!
//! A destination name that is invalid there (OPS-7) is replaced in `dst`
//! already, and `proposed_name` records that the plan changed it, so every
//! conflict check and every child path uses the name that will be created.

use super::conflict::{make_conflict, ConflictContext, Side};
use super::names::{safe_name, NameRules};
use super::{Conflict, OperationKind, Plan, PlanItem, PlanTotals};
use crate::entry::{cap, Capabilities, Entry, Kind};
use crate::error::{Error, ErrorKind, Result};
use crate::provider::{list_all, Lane, Provider, ProviderResolver};
use crate::uri::Uri;
use crate::vpath::VPath;
use futures::future::BoxFuture;
use std::collections::HashMap;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;

/// Most symlinks followed in one chain before the link is skipped (OPS-6).
pub const MAX_LINK_DEPTH: usize = 40;
pub const DEFAULT_THRESHOLD_ITEMS: u64 = 1000;
pub const DEFAULT_THRESHOLD_BYTES: u64 = 1 << 30;

/// How symlinks are copied (OPS-6).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum LinkPolicy {
    /// As links when the destination supports them and both sides are the
    /// same kind of provider; otherwise the target is copied.
    #[default]
    Auto,
    AsLinks,
    FollowTargets,
}

#[derive(Debug, Clone)]
pub struct PlanOptions {
    /// A summary sheet is needed above this many items (OPS-1, setting).
    pub threshold_items: u64,
    /// ... or above this many bytes.
    pub threshold_bytes: u64,
    pub link_policy: LinkPolicy,
    /// Whether the sources and the destination are the same kind of provider
    /// (both local, both bridge, ...). `None`: true when it is one location.
    pub same_provider_kind: Option<bool>,
    /// The destination is a local file system, which can resume a partial file.
    pub dest_is_local: bool,
}

impl Default for PlanOptions {
    fn default() -> Self {
        PlanOptions {
            threshold_items: DEFAULT_THRESHOLD_ITEMS,
            threshold_bytes: DEFAULT_THRESHOLD_BYTES,
            link_policy: LinkPolicy::Auto,
            same_provider_kind: None,
            dest_is_local: false,
        }
    }
}

pub struct Planner;

type Progress<'a> = &'a (dyn Fn(u64) + Send + Sync);

impl Planner {
    /// Plans `kind` for `sources`. For copy and move `destination` is the
    /// target folder; for delete it is only recorded. `progress` receives the
    /// number of items scanned so far.
    pub async fn plan(
        kind: OperationKind,
        sources: Vec<Uri>,
        destination: Uri,
        resolver: &dyn ProviderResolver,
        opts: &PlanOptions,
        cancel: &AtomicBool,
        progress: Progress<'_>,
    ) -> Result<Plan> {
        let items = match kind {
            OperationKind::Copy | OperationKind::Move => {
                plan_transfer(kind, &sources, &destination, resolver, opts, cancel, progress).await?
            }
            OperationKind::Delete => plan_delete(&sources, resolver, cancel, progress).await?,
            _ => {
                return Err(Error::new(
                    ErrorKind::InvalidArgument,
                    "this operation is not planned by the file planner",
                ))
            }
        };
        let renamed_in_place = kind == OperationKind::Move && same_location_all(&sources, &destination);
        Ok(finish(kind, destination, items, opts, renamed_in_place))
    }
}

fn same_location_all(sources: &[Uri], destination: &Uri) -> bool {
    sources.iter().all(|s| s.location == destination.location)
}

fn finish(
    kind: OperationKind,
    destination: Uri,
    items: Vec<PlanItem>,
    opts: &PlanOptions,
    renames: bool,
) -> Plan {
    let mut totals = PlanTotals::default();
    for it in &items {
        if it.kind == Kind::Dir {
            totals.dirs += 1;
        } else {
            totals.files += 1;
        }
        let moves_bytes = !(renames && kind == OperationKind::Move);
        if it.kind != Kind::Dir && moves_bytes {
            totals.bytes = totals.bytes.saturating_add(it.size.unwrap_or(0));
        }
        totals.conflicts += u64::from(it.conflict.is_some());
        totals.renamed += u64::from(it.proposed_name.is_some());
    }
    let needs_summary =
        totals.files + totals.dirs > opts.threshold_items || totals.bytes > opts.threshold_bytes;
    Plan {
        kind,
        destination,
        items,
        totals,
        needs_summary,
    }
}

fn check_cancel(cancel: &AtomicBool) -> Result<()> {
    if cancel.load(Ordering::Relaxed) {
        return Err(Error::kind(ErrorKind::Canceled));
    }
    Ok(())
}

/// Children sorted by name so plans are deterministic.
async fn children(provider: &dyn Provider, dir: &VPath) -> Result<Vec<Entry>> {
    let mut entries = list_all(provider, dir, Lane::Bulk).await?;
    entries.sort_by(|a, b| a.name.cmp(&b.name));
    Ok(entries)
}

fn item_from(src: Uri, dst: Uri, kind: Kind, entry: &Entry) -> PlanItem {
    let is_dir = kind == Kind::Dir;
    PlanItem {
        src,
        dst,
        kind,
        size: if is_dir || kind == Kind::Symlink {
            None
        } else {
            entry.size
        },
        mtime_ms: entry.modified_ms(),
        mode: entry.mode,
        link_target: None,
        conflict: None,
        proposed_name: None,
        resolution: None,
    }
}

// ---------------------------------------------------------------- delete

async fn plan_delete(
    sources: &[Uri],
    resolver: &dyn ProviderResolver,
    cancel: &AtomicBool,
    progress: Progress<'_>,
) -> Result<Vec<PlanItem>> {
    let mut scan = DeleteScan {
        resolver,
        cancel,
        progress,
        items: Vec::new(),
    };
    for src in sources {
        let provider = resolver.provider(&src.location)?;
        let entry = provider.stat(&src.path, false, Lane::Bulk).await?;
        scan.walk(src.clone(), entry).await?;
    }
    Ok(scan.items)
}

struct DeleteScan<'a> {
    resolver: &'a dyn ProviderResolver,
    cancel: &'a AtomicBool,
    progress: Progress<'a>,
    items: Vec<PlanItem>,
}

impl DeleteScan<'_> {
    /// Post-order: children first. Symlinks are removed, never followed.
    fn walk(&mut self, src: Uri, entry: Entry) -> BoxFuture<'_, Result<()>> {
        Box::pin(async move {
            check_cancel(self.cancel)?;
            if entry.kind == Kind::Dir {
                let provider = self.resolver.provider(&src.location)?;
                for child in children(&*provider, &src.path).await? {
                    let child_uri = src.join(&child.name)?;
                    self.walk(child_uri, child).await?;
                }
            }
            self.items.push(item_from(src.clone(), src, entry.kind, &entry));
            (self.progress)(self.items.len() as u64);
            Ok(())
        })
    }
}

// ------------------------------------------------------- copy and move

async fn plan_transfer(
    kind: OperationKind,
    sources: &[Uri],
    destination: &Uri,
    resolver: &dyn ProviderResolver,
    opts: &PlanOptions,
    cancel: &AtomicBool,
    progress: Progress<'_>,
) -> Result<Vec<PlanItem>> {
    let dst = resolver.provider(&destination.location)?;
    let dst_caps = dst.capabilities();
    if !dst_caps.writable() {
        return Err(Error::kind(ErrorKind::ReadOnlyFilesystem));
    }
    let dest_entry = dst.stat(&destination.path, true, Lane::Bulk).await?;
    if !dest_entry.is_dir() {
        return Err(Error::kind(ErrorKind::NotADirectory));
    }
    let mut scan = Scan {
        resolver,
        opts,
        cancel,
        progress,
        kind,
        dst,
        rules: NameRules::from_capabilities(&dst_caps),
        dst_caps,
        as_links: false,
        rename_move: false,
        items: Vec::new(),
        planned: HashMap::new(),
    };
    for src in sources {
        scan.add_source(src, destination).await?;
    }
    Ok(scan.items)
}

/// Where a scanned entry lives. `real` is the path that is listed (the
/// resolved target for a followed symlink), `src` the path recorded in the plan.
struct Node {
    src: Uri,
    real: VPath,
    entry: Entry,
}

#[derive(Clone, Copy)]
struct Frame {
    /// Look the destination up at the provider (false below a folder that is new).
    check_dst: bool,
    link_depth: usize,
}

struct Added {
    index: usize,
    /// The destination exists on disk and is a folder.
    dst_dir_exists: bool,
}

struct Scan<'a> {
    resolver: &'a dyn ProviderResolver,
    opts: &'a PlanOptions,
    cancel: &'a AtomicBool,
    progress: Progress<'a>,
    kind: OperationKind,
    dst: Arc<dyn Provider>,
    dst_caps: Capabilities,
    rules: NameRules,
    as_links: bool,
    rename_move: bool,
    items: Vec<PlanItem>,
    /// Destination path (normalised) to item index, for collisions inside the plan.
    planned: HashMap<Vec<u8>, usize>,
}

impl Scan<'_> {
    async fn add_source(&mut self, src: &Uri, destination: &Uri) -> Result<()> {
        check_cancel(self.cancel)?;
        let name = src
            .name()
            .ok_or_else(|| Error::new(ErrorKind::InvalidArgument, "a location root cannot be copied"))?
            .to_vec();
        let provider = self.resolver.provider(&src.location)?;
        let entry = provider.stat(&src.path, false, Lane::Bulk).await?;
        let same_location = src.location == destination.location;
        if self.kind == OperationKind::Move && !provider.capabilities().writable() {
            return Err(Error::kind(ErrorKind::ReadOnlyFilesystem));
        }
        if same_location && entry.is_dir() && destination.path.starts_with(&src.path) {
            return Err(Error::new(
                ErrorKind::InvalidArgument,
                "a folder cannot be copied or moved into itself",
            ));
        }
        if self.kind == OperationKind::Move && same_location && src.parent().as_ref() == Some(destination) {
            return Ok(());
        }
        self.rename_move = self.kind == OperationKind::Move && same_location;
        self.as_links = self.links_as_links(src, destination);
        let node = Node {
            src: src.clone(),
            real: src.path.clone(),
            entry: Entry { name, ..entry },
        };
        let frame = Frame {
            check_dst: true,
            link_depth: 0,
        };
        self.add_node(node, destination.clone(), frame, Vec::new()).await
    }

    fn links_as_links(&self, src: &Uri, destination: &Uri) -> bool {
        match self.opts.link_policy {
            LinkPolicy::AsLinks => true,
            LinkPolicy::FollowTargets => false,
            LinkPolicy::Auto => {
                let same_kind = self
                    .opts
                    .same_provider_kind
                    .unwrap_or(src.location == destination.location);
                same_kind && self.dst_caps.has(cap::SYMLINKS)
            }
        }
    }

    fn add_node(
        &mut self,
        node: Node,
        parent: Uri,
        frame: Frame,
        ancestors: Vec<VPath>,
    ) -> BoxFuture<'_, Result<()>> {
        Box::pin(async move {
            check_cancel(self.cancel)?;
            match node.entry.kind {
                Kind::Symlink => self.add_link(node, parent, frame, ancestors).await,
                Kind::Dir => self.add_dir(node, parent, frame, ancestors).await,
                _ => {
                    let kind = node.entry.kind;
                    self.add_item(&node.src, &node.entry, kind, &parent, frame, None)
                        .await
                        .map(|_| ())
                }
            }
        })
    }

    async fn add_dir(
        &mut self,
        node: Node,
        parent: Uri,
        frame: Frame,
        mut ancestors: Vec<VPath>,
    ) -> Result<()> {
        // A folder that is its own ancestor (reached through a link) is a loop (OPS-6).
        if ancestors.contains(&node.real) {
            return Ok(());
        }
        let added = self
            .add_item(&node.src, &node.entry, Kind::Dir, &parent, frame, None)
            .await?;
        let item = &self.items[added.index];
        let merging = item.conflict.as_ref().is_some_and(|c| c.dst_is_dir);
        if self.rename_move && !merging {
            return Ok(());
        }
        let dst_dir = item.dst.clone();
        let provider = self.resolver.provider(&node.src.location)?;
        let listing = children(&*provider, &node.real).await?;
        ancestors.push(node.real.clone());
        let child_frame = Frame {
            check_dst: added.dst_dir_exists,
            link_depth: frame.link_depth,
        };
        for entry in listing {
            let real = node.real.join(&entry.name)?;
            let child = Node {
                src: Uri::new(node.src.location.clone(), real.clone()),
                real,
                entry,
            };
            self.add_node(child, dst_dir.clone(), child_frame, ancestors.clone())
                .await?;
        }
        Ok(())
    }

    async fn add_link(&mut self, node: Node, parent: Uri, frame: Frame, ancestors: Vec<VPath>) -> Result<()> {
        let provider = self.resolver.provider(&node.src.location)?;
        let link = provider.read_link(&node.src.path).await?;
        if self.as_links {
            return self
                .add_item(&node.src, &node.entry, Kind::Symlink, &parent, frame, Some(link))
                .await
                .map(|_| ());
        }
        let link_depth = frame.link_depth + 1;
        if link_depth > MAX_LINK_DEPTH {
            return Ok(());
        }
        let target = match provider.stat(&node.src.path, true, Lane::Bulk).await {
            Ok(e) => e,
            // A dangling link has nothing to copy.
            Err(e) if e.kind == ErrorKind::NotFound => return Ok(()),
            Err(e) => return Err(e),
        };
        // Providers may answer a dangling link with the link itself.
        if target.kind == Kind::Symlink {
            return Ok(());
        }
        let real = resolve_link(&node.real, &link).unwrap_or_else(|| node.real.clone());
        let followed = Node {
            src: node.src,
            real,
            entry: Entry {
                name: node.entry.name,
                ..target
            },
        };
        let frame = Frame { link_depth, ..frame };
        // A followed link is a plain file or folder from here on.
        match followed.entry.kind {
            Kind::Dir => self.add_dir(followed, parent, frame, ancestors).await,
            kind => self
                .add_item(&followed.src, &followed.entry, kind, &parent, frame, None)
                .await
                .map(|_| ()),
        }
    }

    /// Adds one item below `parent`, with its destination name, conflict and
    /// bookkeeping.
    async fn add_item(
        &mut self,
        src: &Uri,
        entry: &Entry,
        kind: Kind,
        parent: &Uri,
        frame: Frame,
        link_target: Option<Vec<u8>>,
    ) -> Result<Added> {
        let safe = safe_name(&entry.name, self.rules);
        let dst = parent.join(&safe)?;
        let mut item = item_from(src.clone(), dst, kind, entry);
        item.link_target = link_target;
        if safe != entry.name {
            item.proposed_name = Some(safe);
        }
        let (conflict, dst_dir_exists) = self.detect(&item, frame.check_dst).await?;
        item.conflict = conflict;
        let index = self.items.len();
        self.planned.insert(self.key(&item.dst), index);
        self.items.push(item);
        (self.progress)(self.items.len() as u64);
        Ok(Added {
            index,
            dst_dir_exists,
        })
    }

    fn key(&self, dst: &Uri) -> Vec<u8> {
        if self.rules.case_insensitive {
            String::from_utf8_lossy(dst.path.as_bytes())
                .to_lowercase()
                .into_bytes()
        } else {
            dst.path.as_bytes().to_vec()
        }
    }

    /// The conflict for `item` (what exists at its destination, or an earlier
    /// item of this plan going to the same place) and whether a folder
    /// exists there on disk.
    async fn detect(&self, item: &PlanItem, check_dst: bool) -> Result<(Option<Conflict>, bool)> {
        let src = Side {
            is_dir: item.kind == Kind::Dir,
            size: item.size,
            mtime_ms: item.mtime_ms,
        };
        let ctx = ConflictContext {
            dest_can_resume: self.opts.dest_is_local || self.dst_caps.has(cap::RESUME_UPLOAD),
            same_item: item.src == item.dst,
        };
        if check_dst {
            if let Some(found) = self.stat_dst(&item.dst).await? {
                let dst = side_of(&found);
                return Ok((Some(make_conflict(src, dst, ctx)), dst.is_dir));
            }
        }
        let earlier = self.planned.get(&self.key(&item.dst)).map(|i| &self.items[*i]);
        let conflict = earlier.map(|e| {
            let dst = Side {
                is_dir: e.kind == Kind::Dir,
                size: e.size,
                mtime_ms: e.mtime_ms,
            };
            make_conflict(src, dst, ctx)
        });
        Ok((conflict, false))
    }

    async fn stat_dst(&self, dst: &Uri) -> Result<Option<Entry>> {
        match self.dst.stat(&dst.path, false, Lane::Bulk).await {
            Ok(e) => Ok(Some(e)),
            Err(e) if matches!(e.kind, ErrorKind::NotFound | ErrorKind::NotADirectory) => Ok(None),
            Err(e) => Err(e),
        }
    }
}

fn side_of(e: &Entry) -> Side {
    Side {
        is_dir: e.is_dir(),
        size: e.size,
        mtime_ms: e.modified_ms(),
    }
}

/// The location-relative path a link at `real` points to, with `.` and `..`
/// resolved. Absolute targets are taken relative to the location root. `None`
/// when the target climbs out of the location.
pub fn resolve_link(real: &VPath, target: &[u8]) -> Option<VPath> {
    let mut parts: Vec<Vec<u8>> = if target.first() == Some(&b'/') {
        Vec::new()
    } else {
        real.parent()?.components().map(<[u8]>::to_vec).collect()
    };
    for comp in target.split(|b| *b == b'/') {
        match comp {
            b"" | b"." => {}
            b".." => {
                parts.pop()?;
            }
            c => parts.push(c.to_vec()),
        }
    }
    VPath::parse(&parts.join(&b'/')).ok()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ops::ConflictChoice;
    use crate::provider::memory::MemoryProvider;
    use crate::provider::StaticResolver;
    use std::sync::atomic::AtomicU64;

    fn uri(loc: &str, path: &str) -> Uri {
        Uri::new(loc, VPath::parse(path.as_bytes()).unwrap())
    }

    fn setup() -> (MemoryProvider, StaticResolver) {
        let mem = MemoryProvider::default();
        let r = StaticResolver::default().with("m", Arc::new(mem.clone()));
        (mem, r)
    }

    async fn plan_with(
        kind: OperationKind,
        sources: &[&str],
        dest: &str,
        r: &StaticResolver,
        opts: &PlanOptions,
    ) -> Result<Plan> {
        let cancel = AtomicBool::new(false);
        Planner::plan(
            kind,
            sources.iter().map(|s| uri("m", s)).collect(),
            uri("m", dest),
            r,
            opts,
            &cancel,
            &|_| {},
        )
        .await
    }

    async fn plan(kind: OperationKind, sources: &[&str], dest: &str, r: &StaticResolver) -> Result<Plan> {
        plan_with(kind, sources, dest, r, &PlanOptions::default()).await
    }

    fn dsts(p: &Plan) -> Vec<String> {
        p.items.iter().map(|i| i.dst.path.display()).collect()
    }

    fn srcs(p: &Plan) -> Vec<String> {
        p.items.iter().map(|i| i.src.path.display()).collect()
    }

    #[tokio::test]
    async fn copy_scans_parents_first_with_totals() {
        let (mem, r) = setup();
        mem.add_file("s/d/b.txt", b"12345", 7);
        mem.add_file("s/d/a.txt", b"123", 8);
        mem.add_file("s/d/sub/c", b"1", 9);
        mem.add_file("s/f", b"1234567890", 10);
        mem.add_dir("dst");
        let p = plan(OperationKind::Copy, &["s/d", "s/f"], "dst", &r)
            .await
            .unwrap();
        assert_eq!(
            srcs(&p),
            vec!["s/d", "s/d/a.txt", "s/d/b.txt", "s/d/sub", "s/d/sub/c", "s/f"]
        );
        assert_eq!(
            dsts(&p),
            vec![
                "dst/d",
                "dst/d/a.txt",
                "dst/d/b.txt",
                "dst/d/sub",
                "dst/d/sub/c",
                "dst/f"
            ]
        );
        assert_eq!(p.items[0].kind, Kind::Dir);
        assert_eq!(p.items[0].size, None);
        assert_eq!(p.items[1].size, Some(3));
        assert_eq!(p.items[1].mtime_ms, Some(8));
        assert_eq!(p.items[1].mode, Some(0o644));
        assert_eq!(p.totals.files, 4);
        assert_eq!(p.totals.dirs, 2);
        assert_eq!(p.totals.bytes, 19);
        assert_eq!(p.totals.conflicts, 0);
        assert_eq!(p.totals.renamed, 0);
        assert!(!p.needs_summary);
        assert_eq!(p.kind, OperationKind::Copy);
        assert_eq!(p.destination, uri("m", "dst"));
    }

    #[tokio::test]
    async fn conflicts_are_found_by_stat_at_destination() {
        let (mem, r) = setup();
        mem.add_file("s/a", b"new!", 100);
        mem.add_file("s/d/x", b"1", 1);
        mem.add_file("s/d/y", b"2", 1);
        mem.add_file("dst/a", b"old", 50);
        mem.add_file("dst/d/x", b"1", 1);
        let p = plan(OperationKind::Copy, &["s/a", "s/d"], "dst", &r)
            .await
            .unwrap();
        assert_eq!(p.totals.conflicts, 3);
        let c = p.items[0].conflict.as_ref().unwrap();
        assert!(!c.dst_is_dir && !c.src_is_dir);
        assert_eq!((c.src_size, c.dst_size), (Some(4), Some(3)));
        assert_eq!((c.src_mtime_ms, c.dst_mtime_ms), (Some(100), Some(50)));
        assert!(c.choices.contains(&ConflictChoice::ReplaceIfNewer));
        let dir = p.items[1].conflict.as_ref().unwrap();
        assert!(dir.dst_is_dir && dir.src_is_dir);
        assert!(dir.choices.contains(&ConflictChoice::Merge));
        assert_eq!(srcs(&p), vec!["s/a", "s/d", "s/d/x", "s/d/y"]);
        assert!(p.items[2].conflict.is_some());
        assert!(p.items[3].conflict.is_none());
    }

    #[tokio::test]
    async fn children_of_a_new_folder_are_not_looked_up() {
        let (mem, r) = setup();
        mem.add_file("s/d/x", b"1", 1);
        mem.add_dir("dst");
        let p = plan(OperationKind::Copy, &["s/d"], "dst", &r).await.unwrap();
        assert!(p.items.iter().all(|i| i.conflict.is_none()));
        let calls = mem.calls();
        assert!(!calls.iter().any(|c| c.contains("dst/d/x")), "{calls:?}");
    }

    #[tokio::test]
    async fn same_name_sources_conflict_inside_the_plan() {
        let (mem, r) = setup();
        mem.add_file("a/x.txt", b"1", 1);
        mem.add_file("b/x.txt", b"22", 2);
        mem.add_dir("dst");
        let p = plan(OperationKind::Copy, &["a/x.txt", "b/x.txt"], "dst", &r)
            .await
            .unwrap();
        assert!(p.items[0].conflict.is_none());
        let c = p.items[1].conflict.as_ref().unwrap();
        assert_eq!(c.dst_size, Some(1));
        assert_eq!(p.totals.conflicts, 1);
    }

    #[tokio::test]
    async fn merging_two_same_named_folders_checks_children_against_the_plan() {
        let (mem, r) = setup();
        mem.add_file("a/d/x", b"1", 1);
        mem.add_file("b/d/x", b"2", 1);
        mem.add_dir("dst");
        let p = plan(OperationKind::Copy, &["a/d", "b/d"], "dst", &r)
            .await
            .unwrap();
        assert_eq!(srcs(&p), vec!["a/d", "a/d/x", "b/d", "b/d/x"]);
        assert!(p.items[2].conflict.as_ref().unwrap().dst_is_dir);
        assert!(p.items[3].conflict.is_some());
    }

    #[tokio::test]
    async fn copy_onto_itself_offers_keep_both_and_skip_only() {
        let (mem, r) = setup();
        mem.add_file("d/a", b"1", 1);
        let p = plan(OperationKind::Copy, &["d/a"], "d", &r).await.unwrap();
        let c = p.items[0].conflict.as_ref().unwrap();
        assert_eq!(c.choices, vec![ConflictChoice::KeepBoth, ConflictChoice::Skip]);
    }

    #[tokio::test]
    async fn folder_into_itself_or_its_subfolder_is_rejected() {
        let (mem, r) = setup();
        mem.add_file("a/b/c", b"1", 1);
        for kind in [OperationKind::Copy, OperationKind::Move] {
            for dest in ["a", "a/b"] {
                let err = plan(kind, &["a"], dest, &r).await.unwrap_err();
                assert_eq!(err.kind, ErrorKind::InvalidArgument, "{kind:?} {dest}");
            }
        }
        // A sibling whose name merely starts with the same letters is fine.
        mem.add_dir("ab");
        assert!(plan(OperationKind::Copy, &["a"], "ab", &r).await.is_ok());
    }

    #[tokio::test]
    async fn same_location_move_is_one_rename_item_without_recursion() {
        let (mem, r) = setup();
        mem.add_file("s/d/x", b"123", 1);
        mem.add_file("s/d/sub/y", b"1", 1);
        mem.add_dir("dst");
        let p = plan(OperationKind::Move, &["s/d"], "dst", &r).await.unwrap();
        assert_eq!(srcs(&p), vec!["s/d"]);
        assert_eq!(dsts(&p), vec!["dst/d"]);
        assert_eq!(p.totals.bytes, 0);
        assert_eq!(p.totals.dirs, 1);
        assert!(
            !mem.calls().iter().any(|c| c.starts_with("list s/d")),
            "{:?}",
            mem.calls()
        );
    }

    #[tokio::test]
    async fn same_location_move_of_files_moves_no_bytes_but_a_copy_does() {
        let (mem, r) = setup();
        mem.add_file("s/f", b"12345", 1);
        mem.add_dir("dst");
        let p = plan(OperationKind::Move, &["s/f"], "dst", &r).await.unwrap();
        assert_eq!((p.totals.files, p.totals.bytes), (1, 0));
        let p = plan(OperationKind::Copy, &["s/f"], "dst", &r).await.unwrap();
        assert_eq!((p.totals.files, p.totals.bytes), (1, 5));
    }

    #[tokio::test]
    async fn same_location_move_onto_existing_folder_recurses_to_merge() {
        let (mem, r) = setup();
        mem.add_file("s/d/x", b"1", 1);
        mem.add_file("dst/d/x", b"1", 1);
        let p = plan(OperationKind::Move, &["s/d"], "dst", &r).await.unwrap();
        assert_eq!(srcs(&p), vec!["s/d", "s/d/x"]);
        assert!(p.items[1].conflict.is_some());
    }

    #[tokio::test]
    async fn move_into_its_own_folder_is_a_no_op() {
        let (mem, r) = setup();
        mem.add_file("s/a", b"1", 1);
        mem.add_file("s/b", b"1", 1);
        mem.add_dir("dst");
        let p = plan(OperationKind::Move, &["s/a", "s/b"], "s", &r).await.unwrap();
        assert!(p.items.is_empty());
        let p = plan(OperationKind::Move, &["s/a", "s/b"], "dst", &r)
            .await
            .unwrap();
        assert_eq!(p.items.len(), 2);
    }

    #[tokio::test]
    async fn cross_location_move_counts_bytes() {
        let (mem, _) = setup();
        let other = MemoryProvider::default();
        other.add_dir("in");
        mem.add_file("f", b"12345", 1);
        let r = StaticResolver::default()
            .with("m", Arc::new(mem.clone()))
            .with("o", Arc::new(other));
        let cancel = AtomicBool::new(false);
        let p = Planner::plan(
            OperationKind::Move,
            vec![uri("m", "f")],
            uri("o", "in"),
            &r,
            &PlanOptions::default(),
            &cancel,
            &|_| {},
        )
        .await
        .unwrap();
        assert_eq!(p.totals.bytes, 5);
        assert_eq!(p.items[0].dst, uri("o", "in/f"));
    }

    #[tokio::test]
    async fn summary_thresholds() {
        let (mem, r) = setup();
        mem.add_file("s/a", b"1234", 1);
        mem.add_file("s/b", b"1234", 1);
        mem.add_dir("dst");
        let mut opts = PlanOptions {
            threshold_items: 2,
            ..PlanOptions::default()
        };
        let p = plan_with(OperationKind::Copy, &["s/a", "s/b"], "dst", &r, &opts)
            .await
            .unwrap();
        assert!(!p.needs_summary, "2 items is not more than 2");
        opts.threshold_items = 1;
        let p = plan_with(OperationKind::Copy, &["s/a", "s/b"], "dst", &r, &opts)
            .await
            .unwrap();
        assert!(p.needs_summary);
        let opts = PlanOptions {
            threshold_bytes: 8,
            ..PlanOptions::default()
        };
        let p = plan_with(OperationKind::Copy, &["s/a", "s/b"], "dst", &r, &opts)
            .await
            .unwrap();
        assert!(!p.needs_summary, "8 bytes is not more than 8");
        let opts = PlanOptions {
            threshold_bytes: 7,
            ..PlanOptions::default()
        };
        let p = plan_with(OperationKind::Copy, &["s/a", "s/b"], "dst", &r, &opts)
            .await
            .unwrap();
        assert!(p.needs_summary);
        let d = PlanOptions::default();
        assert_eq!((d.threshold_items, d.threshold_bytes), (1000, 1 << 30));
    }

    #[tokio::test]
    async fn delete_plan_lists_children_first_and_counts() {
        let (mem, r) = setup();
        mem.add_file("d/b", b"12", 1);
        mem.add_file("d/sub/c", b"123", 1);
        mem.add_file("f", b"1", 1);
        let p = plan(OperationKind::Delete, &["d", "f"], "", &r).await.unwrap();
        assert_eq!(srcs(&p), vec!["d/b", "d/sub/c", "d/sub", "d", "f"]);
        assert!(p.items.iter().all(|i| i.src == i.dst && i.conflict.is_none()));
        assert_eq!(p.totals.files, 3);
        assert_eq!(p.totals.dirs, 2);
        assert_eq!(p.totals.bytes, 6);
    }

    #[tokio::test]
    async fn delete_does_not_follow_symlinks() {
        let (mem, r) = setup();
        mem.add_file("real/x", b"1", 1);
        mem.add_dir("d");
        mem.add_symlink("d/l", "/real");
        let p = plan(OperationKind::Delete, &["d"], "", &r).await.unwrap();
        assert_eq!(srcs(&p), vec!["d/l", "d"]);
        assert_eq!(p.items[0].kind, Kind::Symlink);
    }

    #[tokio::test]
    async fn unsupported_kinds_and_bad_destinations() {
        let (mem, r) = setup();
        mem.add_file("f", b"1", 1);
        mem.add_file("g", b"1", 1);
        let err = plan(OperationKind::Compress, &["f"], "", &r).await.unwrap_err();
        assert_eq!(err.kind, ErrorKind::InvalidArgument);
        let err = plan(OperationKind::Copy, &["f"], "g", &r).await.unwrap_err();
        assert_eq!(err.kind, ErrorKind::NotADirectory);
        let err = plan(OperationKind::Copy, &["f"], "nope", &r).await.unwrap_err();
        assert_eq!(err.kind, ErrorKind::NotFound);
        let err = plan(OperationKind::Copy, &["missing"], "", &r).await.unwrap_err();
        assert_eq!(err.kind, ErrorKind::NotFound);
        let err = plan(OperationKind::Copy, &[""], "", &r).await.unwrap_err();
        assert_eq!(err.kind, ErrorKind::InvalidArgument);
        let err = plan(OperationKind::Copy, &["f"], "", &StaticResolver::default())
            .await
            .unwrap_err();
        assert_eq!(err.kind, ErrorKind::NotFound);
    }

    #[tokio::test]
    async fn read_only_destination_or_move_source_is_refused() {
        let ro = MemoryProvider::new(Capabilities::with(&[cap::READ_ONLY]));
        ro.add_file("f", b"1", 1);
        ro.add_dir("d");
        let (mem, _) = setup();
        mem.add_dir("w");
        let r = StaticResolver::default()
            .with("ro", Arc::new(ro))
            .with("m", Arc::new(mem));
        let cancel = AtomicBool::new(false);
        let go = |kind, from: Uri, to: Uri| {
            let r = &r;
            let cancel = &cancel;
            async move { Planner::plan(kind, vec![from], to, r, &PlanOptions::default(), cancel, &|_| {}).await }
        };
        let e = go(OperationKind::Copy, uri("ro", "f"), uri("ro", "d"))
            .await
            .unwrap_err();
        assert_eq!(e.kind, ErrorKind::ReadOnlyFilesystem);
        assert!(go(OperationKind::Copy, uri("ro", "f"), uri("m", "w"))
            .await
            .is_ok());
        let e = go(OperationKind::Move, uri("ro", "f"), uri("m", "w"))
            .await
            .unwrap_err();
        assert_eq!(e.kind, ErrorKind::ReadOnlyFilesystem);
    }

    #[tokio::test]
    async fn cancel_stops_the_scan() {
        let (mem, r) = setup();
        mem.add_file("s/a", b"1", 1);
        mem.add_dir("dst");
        let cancel = AtomicBool::new(true);
        for kind in [OperationKind::Copy, OperationKind::Delete] {
            let err = Planner::plan(
                kind,
                vec![uri("m", "s")],
                uri("m", "dst"),
                &r,
                &PlanOptions::default(),
                &cancel,
                &|_| {},
            )
            .await
            .unwrap_err();
            assert_eq!(err.kind, ErrorKind::Canceled);
        }
    }

    #[tokio::test]
    async fn progress_counts_items() {
        let (mem, r) = setup();
        mem.add_file("s/a", b"1", 1);
        mem.add_file("s/b", b"1", 1);
        mem.add_dir("dst");
        let last = AtomicU64::new(0);
        let calls = AtomicU64::new(0);
        let cancel = AtomicBool::new(false);
        let p = Planner::plan(
            OperationKind::Copy,
            vec![uri("m", "s")],
            uri("m", "dst"),
            &r,
            &PlanOptions::default(),
            &cancel,
            &|n| {
                last.store(n, Ordering::SeqCst);
                calls.fetch_add(1, Ordering::SeqCst);
            },
        )
        .await
        .unwrap();
        assert_eq!(p.items.len(), 3);
        assert_eq!(last.load(Ordering::SeqCst), 3);
        assert_eq!(calls.load(Ordering::SeqCst), 3);
    }

    fn restricted_dest() -> (MemoryProvider, MemoryProvider, StaticResolver) {
        let src = MemoryProvider::default();
        let dst = MemoryProvider::new(Capabilities::with(&[cap::WRITE, cap::RESTRICTED_NAMES]));
        dst.add_dir("fat");
        let r = StaticResolver::default()
            .with("s", Arc::new(src.clone()))
            .with("d", Arc::new(dst.clone()));
        (src, dst, r)
    }

    #[tokio::test]
    async fn invalid_destination_names_get_safe_names() {
        let (src, dst, r) = restricted_dest();
        src.add_file("x/a:b.txt", b"1", 1);
        src.add_file("x/ok.txt", b"1", 1);
        src.add_file("x/a_b.txt", b"1", 1);
        src.add_file("y/q?/CON", b"1", 1);
        dst.add_file("fat/ok.txt", b"1", 1);
        let cancel = AtomicBool::new(false);
        let sources = ["x/a:b.txt", "x/a_b.txt", "x/ok.txt", "y/q?"];
        let p = Planner::plan(
            OperationKind::Copy,
            sources.iter().map(|s| uri("s", s)).collect(),
            uri("d", "fat"),
            &r,
            &PlanOptions::default(),
            &cancel,
            &|_| {},
        )
        .await
        .unwrap();
        assert_eq!(
            dsts(&p),
            vec![
                "fat/a_b.txt",
                "fat/a_b.txt",
                "fat/ok.txt",
                "fat/q_",
                "fat/q_/CON_"
            ]
        );
        assert_eq!(p.items[0].proposed_name.as_deref(), Some(&b"a_b.txt"[..]));
        assert_eq!(p.items[1].proposed_name, None);
        assert_eq!(p.items[3].proposed_name.as_deref(), Some(&b"q_"[..]));
        assert_eq!(p.items[4].proposed_name.as_deref(), Some(&b"CON_"[..]));
        assert_eq!(p.totals.renamed, 3);
        // a:b.txt became a_b.txt and collides with the real a_b.txt, ok.txt with the existing one.
        assert!(p.items[0].conflict.is_none());
        assert!(p.items[1].conflict.is_some());
        assert!(p.items[2].conflict.is_some());
        assert!(p.items[4].conflict.is_none());
    }

    #[tokio::test]
    async fn resume_is_offered_for_local_or_capable_destinations() {
        let (mem, r) = setup();
        mem.add_file("s/f", &[0u8; 100], 5);
        mem.add_file("dst/f", &[0u8; 40], 1);
        let p = plan(OperationKind::Copy, &["s/f"], "dst", &r).await.unwrap();
        let c = p.items[0].conflict.as_ref().unwrap();
        assert!(c.resumable, "memory provider has ResumeUpload");

        let no_resume = MemoryProvider::new(Capabilities::with(&[cap::WRITE]));
        no_resume.add_file("s/f", &[0u8; 100], 5);
        no_resume.add_file("dst/f", &[0u8; 40], 1);
        let r2 = StaticResolver::default().with("m", Arc::new(no_resume));
        let p = plan(OperationKind::Copy, &["s/f"], "dst", &r2).await.unwrap();
        assert!(!p.items[0].conflict.as_ref().unwrap().resumable);
        let opts = PlanOptions {
            dest_is_local: true,
            ..PlanOptions::default()
        };
        let p = plan_with(OperationKind::Copy, &["s/f"], "dst", &r2, &opts)
            .await
            .unwrap();
        assert!(p.items[0].conflict.as_ref().unwrap().resumable);
    }

    #[tokio::test]
    async fn symlinks_are_copied_as_links_when_possible() {
        let (mem, r) = setup();
        mem.add_file("s/real", b"12345", 1);
        mem.add_symlink("s/l", "real");
        mem.add_dir("dst");
        let p = plan(OperationKind::Copy, &["s/l"], "dst", &r).await.unwrap();
        assert_eq!(p.items.len(), 1);
        assert_eq!(p.items[0].kind, Kind::Symlink);
        assert_eq!(p.items[0].link_target.as_deref(), Some(&b"real"[..]));
        assert_eq!(p.items[0].size, None);
    }

    #[tokio::test]
    async fn symlinks_are_followed_across_provider_kinds_or_without_capability() {
        let (mem, r) = setup();
        mem.add_file("s/real", b"12345", 1);
        mem.add_symlink("s/l", "real");
        mem.add_dir("dst");
        for opts in [
            PlanOptions {
                same_provider_kind: Some(false),
                ..PlanOptions::default()
            },
            PlanOptions {
                link_policy: LinkPolicy::FollowTargets,
                ..PlanOptions::default()
            },
        ] {
            let p = plan_with(OperationKind::Copy, &["s/l"], "dst", &r, &opts)
                .await
                .unwrap();
            assert_eq!(p.items[0].kind, Kind::File);
            assert_eq!(p.items[0].size, Some(5));
            assert_eq!(p.items[0].link_target, None);
            assert_eq!(p.items[0].dst, uri("m", "dst/l"));
            assert_eq!(p.items[0].src, uri("m", "s/l"));
        }
        let nolink = MemoryProvider::new(Capabilities::with(&[cap::WRITE]));
        nolink.add_file("s/real", b"12345", 1);
        nolink.add_symlink("s/l", "real");
        nolink.add_dir("dst");
        let r2 = StaticResolver::default().with("m", Arc::new(nolink));
        let p = plan(OperationKind::Copy, &["s/l"], "dst", &r2).await.unwrap();
        assert_eq!(p.items[0].kind, Kind::File);
        let forced = PlanOptions {
            link_policy: LinkPolicy::AsLinks,
            same_provider_kind: Some(false),
            ..PlanOptions::default()
        };
        let p = plan_with(OperationKind::Copy, &["s/l"], "dst", &r2, &forced)
            .await
            .unwrap();
        assert_eq!(p.items[0].kind, Kind::Symlink);
    }

    fn follow() -> PlanOptions {
        PlanOptions {
            link_policy: LinkPolicy::FollowTargets,
            ..PlanOptions::default()
        }
    }

    #[tokio::test]
    async fn followed_folder_links_are_copied_as_folders() {
        let (mem, r) = setup();
        mem.add_file("real/x", b"1", 1);
        mem.add_dir("s");
        mem.add_symlink("s/l", "/real");
        mem.add_dir("dst");
        let p = plan_with(OperationKind::Copy, &["s"], "dst", &r, &follow())
            .await
            .unwrap();
        assert_eq!(dsts(&p), vec!["dst/s", "dst/s/l", "dst/s/l/x"]);
        assert_eq!(p.items[1].kind, Kind::Dir);
        assert_eq!(p.items[2].src, uri("m", "real/x"));
    }

    fn link_to_file() -> (MemoryProvider, StaticResolver) {
        let (mem, r) = setup();
        mem.add_file("real/x", b"1", 1);
        mem.add_file("s/a", b"1", 1);
        mem.add_symlink("s/l", "/real/x");
        mem.add_dir("dst");
        (mem, r)
    }

    #[tokio::test]
    async fn a_link_that_vanishes_before_it_is_followed_is_skipped() {
        let (mem, r) = link_to_file();
        mem.fail_next("stat", "s/l", Error::kind(ErrorKind::NotFound));
        let p = plan_with(OperationKind::Copy, &["s"], "dst", &r, &follow())
            .await
            .unwrap();
        assert_eq!(dsts(&p), vec!["dst/s", "dst/s/a"]);
    }

    #[tokio::test]
    async fn other_errors_while_following_a_link_fail_the_plan() {
        let (mem, r) = link_to_file();
        mem.fail_next("stat", "s/l", Error::kind(ErrorKind::PermissionDenied));
        let err = plan_with(OperationKind::Copy, &["s"], "dst", &r, &follow())
            .await
            .unwrap_err();
        assert_eq!(err.kind, ErrorKind::PermissionDenied);
    }

    #[tokio::test]
    async fn a_failing_destination_lookup_fails_the_plan() {
        let (mem, r) = setup();
        mem.add_file("s/a", b"1", 1);
        mem.add_dir("dst");
        mem.fail_next("stat", "dst/a", Error::kind(ErrorKind::PermissionDenied));
        let err = plan(OperationKind::Copy, &["s/a"], "dst", &r).await.unwrap_err();
        assert_eq!(err.kind, ErrorKind::PermissionDenied);
    }

    #[tokio::test]
    async fn link_loops_are_skipped() {
        let src = MemoryProvider::default();
        src.add_file("a/x", b"1", 1);
        src.add_symlink("a/loop", "..");
        src.add_symlink("a/self", "/a");
        src.add_symlink("a/dangling", "nowhere");
        let dst = MemoryProvider::default();
        dst.add_dir("d");
        let r = StaticResolver::default()
            .with("s", Arc::new(src))
            .with("d", Arc::new(dst));
        let cancel = AtomicBool::new(false);
        let p = Planner::plan(
            OperationKind::Copy,
            vec![uri("s", "a")],
            uri("d", "d"),
            &r,
            &follow(),
            &cancel,
            &|_| {},
        )
        .await
        .unwrap();
        // `loop` is the location root, which holds only `a` again: skipped. `self` is `a` itself.
        assert_eq!(dsts(&p), vec!["d/a", "d/a/loop", "d/a/x"]);
        assert_eq!(p.items[1].kind, Kind::Dir);
    }

    #[tokio::test]
    async fn sibling_link_to_same_folder_is_copied_twice_not_a_loop() {
        let (mem, r) = setup();
        mem.add_file("real/x", b"1", 1);
        mem.add_dir("s");
        mem.add_symlink("s/l1", "/real");
        mem.add_symlink("s/l2", "/real");
        mem.add_dir("dst");
        let p = plan_with(OperationKind::Copy, &["s"], "dst", &r, &follow())
            .await
            .unwrap();
        assert_eq!(p.items.len(), 5);
    }

    #[tokio::test]
    async fn long_link_chains_hit_the_depth_limit() {
        let (mem, r) = setup();
        mem.add_file("d0/leaf", b"1", 1);
        mem.add_dir("s");
        for i in 1..=45 {
            mem.add_dir(&format!("d{i}"));
            mem.add_symlink(&format!("d{i}/n"), &format!("/d{}", i - 1));
        }
        mem.add_symlink("s/start", "/d45");
        mem.add_dir("dst");
        let p = plan_with(OperationKind::Copy, &["s"], "dst", &r, &follow())
            .await
            .unwrap();
        // d45 .. d6 hold links 1..=40 deep; the 41st link is skipped.
        let deepest = p.items.iter().map(|i| i.dst.path.depth()).max().unwrap();
        assert_eq!(deepest, 2 + MAX_LINK_DEPTH);
        assert!(!dsts(&p).iter().any(|d| d.ends_with("leaf")));
    }

    #[test]
    fn link_targets_resolve_lexically() {
        let p = |s: &str| VPath::parse(s.as_bytes()).unwrap();
        assert_eq!(resolve_link(&p("a/b/l"), b"../c"), Some(p("a/c")));
        assert_eq!(resolve_link(&p("a/b/l"), b"c/./d"), Some(p("a/b/c/d")));
        assert_eq!(resolve_link(&p("a/b/l"), b"/x/y"), Some(p("x/y")));
        assert_eq!(resolve_link(&p("a/l"), b".."), Some(p("")));
        assert_eq!(resolve_link(&p("a/l"), b"../.."), None);
        assert_eq!(resolve_link(&p("l"), b"x"), Some(p("x")));
        assert_eq!(resolve_link(&p(""), b"x"), None);
        assert_eq!(resolve_link(&p("a/l"), b"x//y"), Some(p("a/x/y")));
    }

    #[tokio::test]
    async fn case_insensitive_destination_detects_name_collisions_in_plan() {
        let dst = MemoryProvider::new(Capabilities::with(&[cap::WRITE, cap::CASE_INSENSITIVE]));
        dst.add_dir("d");
        let src = MemoryProvider::default();
        src.add_file("a/Photo.jpg", b"1", 1);
        src.add_file("b/photo.JPG", b"1", 1);
        let r = StaticResolver::default()
            .with("s", Arc::new(src))
            .with("d", Arc::new(dst));
        let cancel = AtomicBool::new(false);
        let p = Planner::plan(
            OperationKind::Copy,
            vec![uri("s", "a/Photo.jpg"), uri("s", "b/photo.JPG")],
            uri("d", "d"),
            &r,
            &PlanOptions::default(),
            &cancel,
            &|_| {},
        )
        .await
        .unwrap();
        assert!(p.items[1].conflict.is_some());
    }
}
