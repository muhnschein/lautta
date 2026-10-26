// SPDX-License-Identifier: LGPL-2.1-or-later
//! User-level actions of the operations area on [`Core`](crate::app::Core):
//! plan summaries and pending plans (OPS-1), conflicts before the run
//! (OPS-2), compress and extract (PRV-10/11) and the *Trash*
//! (OPS-8). Everything is Qt-free; the Qt layer only
//! forwards.

use crate::app::{Core, Started};
use crate::app_directory::file_url;
use crate::compress::{compress, ArchiveKind, CompressOptions};
use crate::entry::{cap, Entry, Kind};
use crate::error::{Error, ErrorKind, Result};
use crate::mime;
use crate::ops::conflict as conflict_ops;
use crate::ops::names::{keep_both_name, split_extension};
use crate::ops::{Conflict, ConflictChoice, OperationKind, Plan};
use crate::provider::{list_all, Disposition, Lane, ProgressSink, Provider, WriteOptions};
use crate::settings::Settings;
use crate::sys;
use crate::thumbs::{app_thumbnail_url, thumbnail_source, trash_item_uri, ThumbSettings, ThumbSource};
use crate::transfer::TransferId;
use crate::trash::TrashItem;
use crate::uri::Uri;
use crate::vpath::{display_name, validate_name, VPath};
use serde::Serialize;
use serde_json::Value;
use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::{Arc, Mutex};

/// Separator of the breadcrumb shown for a folder ("Documents › Uni").
const CRUMB: &str = " › ";
/// Names listed in the summary sheet ("Names that need changing").
const SUMMARY_RENAMES: usize = 50;
const SECS_PER_DAY: i64 = 86_400;

fn guard<T>(m: &Mutex<T>) -> std::sync::MutexGuard<'_, T> {
    match m.lock() {
        Ok(g) => g,
        Err(p) => p.into_inner(),
    }
}

fn join_err(e: tokio::task::JoinError) -> Error {
    Error::new(ErrorKind::Internal, e.to_string())
}

// ------------------------------------------------------------ pending plans

/// Large plans wait for the user's confirmation under an id (OPS-1).
///
/// A plan is lent out while a conflict answer works on it across awaits
/// ([`lend`](Self::lend)); starting it meanwhile waits for it to come back
/// ([`take_when_returned`](Self::take_when_returned)) instead of finding
/// nothing, and discarding it meanwhile drops it when it comes back.
#[derive(Default)]
pub struct PendingPlans {
    next: AtomicU64,
    plans: Mutex<HashMap<u64, Slot>>,
    returned: tokio::sync::Notify,
}

enum Slot {
    Ready(Box<Plan>),
    Lent,
}

impl PendingPlans {
    pub fn new() -> PendingPlans {
        PendingPlans::default()
    }

    /// Keeps `plan` and returns its id (1, 2, …).
    pub fn insert(&self, plan: Plan) -> u64 {
        let id = self.next.fetch_add(1, Ordering::Relaxed) + 1;
        guard(&self.plans).insert(id, Slot::Ready(Box::new(plan)));
        id
    }

    /// Takes the plan out for good (to start it); `None` while it is lent.
    pub fn take(&self, id: u64) -> Option<Plan> {
        self.take_unless_lent(id).ok().flatten()
    }

    /// The plan taken out, `Ok(None)` without one, `Err` while it is lent.
    fn take_unless_lent(&self, id: u64) -> std::result::Result<Option<Plan>, ()> {
        let mut plans = guard(&self.plans);
        match plans.remove(&id) {
            Some(Slot::Ready(plan)) => Ok(Some(*plan)),
            Some(Slot::Lent) => {
                plans.insert(id, Slot::Lent);
                Err(())
            }
            None => Ok(None),
        }
    }

    /// The plan, once it is back if it is lent; `None` when there is no
    /// such plan or it was discarded meanwhile.
    pub async fn take_when_returned(&self, id: u64) -> Option<Plan> {
        loop {
            let returned = self.returned.notified();
            let mut returned = std::pin::pin!(returned);
            // Registered before looking, so a return in between is not missed.
            returned.as_mut().enable();
            if let Ok(plan) = self.take_unless_lent(id) {
                return plan;
            }
            returned.await;
        }
    }

    /// Takes the plan out to work on it across awaits; it stays known and
    /// comes back with [`put_back`](Self::put_back).
    pub fn lend(&self, id: u64) -> Option<Plan> {
        let mut plans = guard(&self.plans);
        match std::mem::replace(plans.get_mut(&id)?, Slot::Lent) {
            Slot::Ready(plan) => Some(*plan),
            Slot::Lent => None,
        }
    }

    /// Returns a plan taken out with [`lend`](Self::lend); one discarded
    /// while it was out is dropped.
    pub fn put_back(&self, id: u64, plan: Plan) {
        if let Some(slot) = guard(&self.plans).get_mut(&id) {
            *slot = Slot::Ready(Box::new(plan));
        }
        self.returned.notify_waiters();
    }

    pub fn discard(&self, id: u64) -> bool {
        let found = guard(&self.plans).remove(&id).is_some();
        self.returned.notify_waiters();
        found
    }

    pub fn with<R>(&self, id: u64, f: impl FnOnce(&Plan) -> R) -> Option<R> {
        match guard(&self.plans).get(&id) {
            Some(Slot::Ready(plan)) => Some(f(plan)),
            _ => None,
        }
    }

    pub fn len(&self) -> usize {
        guard(&self.plans).len()
    }

    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }
}

// ------------------------------------------------------------ plan summary

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct RenamePair {
    pub from: String,
    pub to: String,
}

/// The options of the summary sheet, seeded from the settings (OPS-5, XFR-4).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct StartOptions {
    pub verify_checksums: bool,
    pub preserve_mtime: bool,
    pub preserve_mode: bool,
    /// Keep the safe names the planner proposed (OPS-7).
    pub suggested_names: bool,
}

impl StartOptions {
    pub fn from_settings(s: &Settings) -> StartOptions {
        StartOptions {
            verify_checksums: s.verify_checksums,
            preserve_mtime: s.preserve_mtimes,
            preserve_mode: s.preserve_permissions,
            suggested_names: true,
        }
    }

    /// Reads the options sheet's JSON; missing keys keep `self`.
    pub fn merged_with_json(self, text: &str) -> StartOptions {
        let Ok(Value::Object(m)) = serde_json::from_str::<Value>(text) else {
            return self;
        };
        let flag = |key: &str, old: bool| m.get(key).and_then(Value::as_bool).unwrap_or(old);
        StartOptions {
            verify_checksums: flag("verifyChecksums", self.verify_checksums),
            preserve_mtime: flag("preserveMtime", self.preserve_mtime),
            preserve_mode: flag("preserveMode", self.preserve_mode),
            suggested_names: flag("suggestedNames", self.suggested_names),
        }
    }
}

/// What the PlanSummary sheet shows.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PlanSummary {
    pub kind: String,
    pub files: u64,
    pub dirs: u64,
    pub bytes: u64,
    pub conflicts: u64,
    pub renamed: u64,
    pub destination: String,
    pub destination_name: String,
    pub free_bytes: Option<u64>,
    pub renames: Vec<RenamePair>,
    /// Both sides keep POSIX modes (OPS-5): the option is only offered then.
    pub permissions_supported: bool,
    pub options: StartOptions,
}

pub fn kind_name(kind: OperationKind) -> &'static str {
    match kind {
        OperationKind::Copy => "copy",
        OperationKind::Move => "move",
        OperationKind::Delete => "delete",
        OperationKind::Compress => "compress",
        OperationKind::Extract => "extract",
    }
}

fn rebase(uri: &Uri, old_root: &Uri, new_root: &Uri) -> Uri {
    match uri.path.strip_prefix(&old_root.path) {
        Some(rel) if uri.location == old_root.location => {
            Uri::new(new_root.location.clone(), new_root.path.join_path(&rel))
        }
        _ => uri.clone(),
    }
}

/// Undoes the planner's safe-name proposals (OPS-7): every renamed item (and
/// what lies below it) goes back to the source's own name.
pub fn revert_suggested_names(plan: &mut Plan) {
    for i in 0..plan.items.len() {
        if plan.items[i].proposed_name.is_none() {
            continue;
        }
        plan.items[i].proposed_name = None;
        let old = plan.items[i].dst.clone();
        let original = plan.items[i].src.name().map(<[u8]>::to_vec);
        let (Some(parent), Some(name)) = (old.parent(), original) else {
            continue;
        };
        let Ok(new) = parent.join(&name) else { continue };
        for item in &mut plan.items[i..] {
            item.dst = rebase(&item.dst, &old, &new);
        }
    }
    plan.totals.renamed = 0;
}

/// A conflict that waits for an answer before the run (OPS-2).
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ConflictEntry {
    pub index: usize,
    pub name: String,
    /// Where it already exists ("NAS › srv › photos").
    pub folder: String,
    pub source: String,
    pub destination: String,
    pub conflict: Conflict,
    /// Never `Replace` (OPS-2).
    pub default_choice: ConflictChoice,
}

// ------------------------------------------------------------ compress

/// What the Compress dialog needs to size the job.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize)]
pub struct Measure {
    pub files: u64,
    pub dirs: u64,
    pub bytes: u64,
}

/// Contents of an archive (Extract dialog, ArchiveView header).
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ArchiveInfo {
    pub location: String,
    pub root_uri: String,
    pub name: String,
    /// The archive's name without its extension(s): the "new folder".
    pub folder_name: String,
    pub files: u64,
    pub dirs: u64,
    pub bytes: u64,
}

impl ArchiveKind {
    pub fn parse(text: &str) -> Option<ArchiveKind> {
        match text {
            "zip" => Some(ArchiveKind::Zip),
            "tar.gz" | "targz" | "tgz" => Some(ArchiveKind::TarGz),
            _ => None,
        }
    }

    pub fn extension(self) -> &'static str {
        match self {
            ArchiveKind::Zip => ".zip",
            ArchiveKind::TarGz => ".tar.gz",
        }
    }
}

/// `name` with the archive extension (once).
pub fn archive_file_name(name: &str, kind: ArchiveKind) -> String {
    let ext = kind.extension();
    if name.to_lowercase().ends_with(ext) {
        name.to_owned()
    } else {
        format!("{name}{ext}")
    }
}

/// The archive's name without extension, for "New folder 'dataset'".
pub fn archive_folder_name(name: &str) -> String {
    let (stem, _) = split_extension(name.as_bytes(), false);
    let stem = String::from_utf8_lossy(stem).into_owned();
    if stem.is_empty() {
        name.to_owned()
    } else {
        stem
    }
}

// ------------------------------------------------------------ trash

/// One row of the *Trash* (OPS-8).
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct TrashEntry {
    pub id: i64,
    pub name: String,
    pub original_uri: String,
    pub folder_uri: String,
    pub folder_name: String,
    pub is_dir: bool,
    pub size: Option<u64>,
    /// Unix seconds.
    pub trashed_at: i64,
    pub days_left: i64,
    /// From the original name's extension; empty when none is known. Stored
    /// copy has no extension, so the system thumbnailer goes by this.
    pub mime_type: String,
    /// `file://` URL of the stored copy of an image or video, or the
    /// `image://lautta-thumb/` URL of a PDF, that gets a thumbnail, else empty
    /// ([`trash_thumbnail`]).
    pub thumbnail_source: String,
}

/// What a trashed file shows as its thumbnail (OPS-8, PRV-1, PRV-14): the
/// shared rule ([`thumbnail_source`]) decides by the original name, which
/// the stored copy has lost. An image or video gives the stored copy's
/// `file://` URL for the system thumbnailer, a PDF an app thumbnail of the
/// Trash item. Empty = row keeps its glyph: thumbnails off (BRW-4), the rule
/// says none, or stored item is no regular file. A trashed link would show
/// its target, maybe outside the sandbox (BRW-10); a copy gone from the trash
/// folder has nothing to show.
fn trash_thumbnail(item: &TrashItem, settings: &Settings) -> String {
    let kind = if item.is_dir { Kind::Dir } else { Kind::File };
    let mut entry = Entry::new(item.original_uri.name().unwrap_or_default(), kind);
    entry.size = item.size;
    let set = ThumbSettings::new(settings);
    let source = thumbnail_source(&entry, true, settings.thumbnails_local, &set);
    let stored = item.stored_path();
    if source == ThumbSource::Icon || !std::fs::symlink_metadata(&stored).is_ok_and(|m| m.is_file()) {
        return String::new();
    }
    match source {
        ThumbSource::App => {
            trash_item_uri(item.id, &item.stored_name).map_or_else(String::new, |uri| app_thumbnail_url(&uri))
        }
        _ => file_url(&stored),
    }
}

/// Whole days left before purge, never negative (OPS-8).
pub fn days_left(trashed_at: i64, now: i64, retention_days: u32) -> i64 {
    let ends = trashed_at.saturating_add(i64::from(retention_days) * SECS_PER_DAY);
    ((ends - now).max(0) + SECS_PER_DAY - 1) / SECS_PER_DAY
}

// ------------------------------------------------------------ share

/// A file received through the share target (INT-1).
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SharedFile {
    pub path: String,
    pub name: String,
    pub size: u64,
    pub readable: bool,
    pub uri: String,
}

/// A place a shared file can be saved to.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct Destination {
    pub uri: String,
    pub name: String,
    pub kind: String,
}

fn location_kind_name(kind: &crate::locations::LocationKind) -> &'static str {
    use crate::locations::LocationKind as K;
    match kind {
        K::UserFolder => "device",
        K::Android => "android",
        K::Volume => "volume",
        K::Server { .. } | K::AdHoc => "server",
        K::Archive => "archive",
    }
}

// ============================================================ Core methods

impl Core {
    /// `Location › folder › subfolder` for a URI (what the sheets show).
    pub fn display_path(&self, uri: &Uri) -> String {
        let mut parts = vec![self
            .location(&uri.location)
            .map_or_else(|| uri.location.clone(), |l| l.name)];
        parts.extend(uri.path.components().map(display_name));
        parts.join(CRUMB)
    }

    /// The breadcrumb of the folder that contains `uri`.
    pub fn display_folder(&self, uri: &Uri) -> String {
        uri.parent()
            .map_or_else(|| self.display_path(uri), |p| self.display_path(&p))
    }

    // -------------------------------------------------------- plans

    /// The numbers and names the summary sheet shows (OPS-1).
    pub async fn plan_summary(&self, plan: &Plan) -> PlanSummary {
        let dest = &plan.destination;
        let free_bytes = match self.provider(&dest.location) {
            Ok(p) => p.space(&dest.path).await.ok().map(|s| s.free),
            Err(_) => None,
        };
        let has_modes = |location: &str| {
            self.provider(location)
                .is_ok_and(|p| p.capabilities().has(cap::PERMISSIONS))
        };
        let permissions_supported = has_modes(&dest.location)
            && plan
                .items
                .first()
                .is_none_or(|first| has_modes(&first.src.location));
        let renames = plan
            .items
            .iter()
            .filter(|it| it.proposed_name.is_some())
            .take(SUMMARY_RENAMES)
            .map(|it| RenamePair {
                from: it.src.name().map(display_name).unwrap_or_default(),
                to: it.dst.name().map(display_name).unwrap_or_default(),
            })
            .collect();
        PlanSummary {
            kind: kind_name(plan.kind).to_owned(),
            files: plan.totals.files,
            dirs: plan.totals.dirs,
            bytes: plan.totals.bytes,
            conflicts: plan.totals.conflicts,
            renamed: plan.totals.renamed,
            destination: dest.to_string(),
            destination_name: self.display_path(dest),
            free_bytes,
            renames,
            permissions_supported,
            options: StartOptions::from_settings(&self.settings()),
        }
    }

    /// Queues `plan` with the sheet's options (OPS-5, OPS-7, XFR-4). The
    /// options only differ from the settings for this one start: the settings
    /// are put back right after the transfer is queued.
    pub async fn start_plan_with(&self, mut plan: Plan, options: StartOptions) -> Result<TransferId> {
        if !options.suggested_names {
            revert_suggested_names(&mut plan);
        }
        let saved = self.settings();
        let wanted = Settings {
            verify_checksums: options.verify_checksums,
            preserve_mtimes: options.preserve_mtime,
            preserve_permissions: options.preserve_mode,
            ..saved.clone()
        };
        if wanted == saved {
            return self.start_plan(plan).await;
        }
        self.apply_settings(wanted.clone());
        let started = self.start_plan(plan).await;
        if self.settings() == wanted {
            self.apply_settings(saved);
        }
        started
    }

    /// The conflicts of a plan that still wait for an answer, in order.
    pub fn unresolved_conflicts(&self, plan: &Plan) -> Vec<ConflictEntry> {
        conflict_ops::unresolved(plan)
            .into_iter()
            .filter_map(|index| {
                let item = &plan.items[index];
                let conflict = item.conflict.clone()?;
                Some(ConflictEntry {
                    index,
                    name: item.dst.name().map(display_name).unwrap_or_default(),
                    folder: self.display_folder(&item.dst),
                    source: item.src.to_string(),
                    destination: item.dst.to_string(),
                    default_choice: conflict_ops::default_choice(&conflict),
                    conflict,
                })
            })
            .collect()
    }

    /// Answers a conflict of a pending plan (OPS-2); returns how many items
    /// the answer settled.
    pub async fn resolve_plan_conflict(
        &self,
        plan: &mut Plan,
        index: usize,
        choice: ConflictChoice,
        apply_to_all: bool,
    ) -> Result<usize> {
        conflict_ops::resolve(plan, index, choice, apply_to_all, &self.locations).await
    }

    // -------------------------------------------------------- archives

    /// Totals of a selection (Compress: "Size before compression").
    pub async fn measure(&self, sources: &[Uri]) -> Result<Measure> {
        let first = sources
            .first()
            .ok_or_else(|| Error::new(ErrorKind::InvalidArgument, "nothing selected"))?;
        let dest = first.parent().unwrap_or_else(|| first.clone());
        let plan = self.plan(OperationKind::Delete, sources.to_vec(), dest).await?;
        Ok(Measure {
            files: plan.totals.files,
            dirs: plan.totals.dirs,
            bytes: plan.totals.bytes,
        })
    }

    /// Opens the archive as a location and counts what it holds (PRV-10).
    pub async fn archive_info(&self, archive: &Uri) -> Result<ArchiveInfo> {
        let location = self.open_archive(archive).await?;
        let provider = self.provider(&location)?;
        let mut info = ArchiveInfo {
            location: location.clone(),
            root_uri: Uri::root(location).to_string(),
            name: String::new(),
            folder_name: String::new(),
            files: 0,
            dirs: 0,
            bytes: 0,
        };
        let name = archive.name().map(display_name).unwrap_or_default();
        info.folder_name = archive_folder_name(&name);
        info.name = name;
        let mut pending = vec![VPath::root()];
        while let Some(dir) = pending.pop() {
            for e in list_all(provider.as_ref(), &dir, Lane::Interactive).await? {
                if e.kind == Kind::Dir {
                    info.dirs += 1;
                    pending.push(dir.join(&e.name)?);
                } else {
                    info.files += 1;
                    info.bytes += e.size.unwrap_or(0);
                }
            }
        }
        Ok(info)
    }

    /// Extracts the whole archive into `dest` (created when missing): a copy
    /// plan from the archive's root (XFR-3 "archive → anywhere").
    pub async fn extract(&self, archive: &Uri, dest: &Uri) -> Result<Started> {
        let location = self.open_archive(archive).await?;
        let root = Uri::root(location.clone());
        let source = self.provider(&location)?;
        let children = list_all(source.as_ref(), &root.path, Lane::Interactive).await?;
        if children.is_empty() {
            return Err(Error::new(ErrorKind::InvalidArgument, "the archive is empty"));
        }
        let sources = children
            .iter()
            .map(|e| root.join(&e.name))
            .collect::<Result<Vec<_>>>()?;
        let target = self.provider(&dest.location)?;
        target.make_dir(&dest.path, false).await?;
        if let Some(parent) = dest.parent() {
            self.invalidate(&parent);
        }
        self.copy_or_move(OperationKind::Copy, sources, dest.clone())
            .await
    }

    /// Creates a zip or tar.gz of `sources` as `name` in `dest_dir` (PRV-11):
    /// straight into a local file, or into a pipe that is uploaded while the
    /// archive is produced. A name that is taken gets "name 2.zip". A failed
    /// or canceled run leaves no partial file. Returns the archive's URI.
    pub async fn compress_to(
        &self,
        sources: &[Uri],
        dest_dir: &Uri,
        name: &str,
        kind: ArchiveKind,
        progress: ProgressSink,
        cancel: Arc<AtomicBool>,
    ) -> Result<Uri> {
        let provider = self.provider(&dest_dir.location)?;
        if !provider.capabilities().writable() {
            return Err(Error::kind(ErrorKind::ReadOnlyFilesystem));
        }
        let file_name = archive_file_name(name.trim(), kind);
        validate_name(file_name.as_bytes())?;
        let taken: HashSet<Vec<u8>> = list_all(provider.as_ref(), &dest_dir.path, Lane::Interactive)
            .await?
            .into_iter()
            .map(|e| e.name)
            .collect();
        let final_name = free_name(file_name.as_bytes(), &taken);
        let target = dest_dir.join(&final_name)?;
        let mut opts = CompressOptions::new(kind);
        opts.progress = progress;
        opts.cancel = cancel;
        opts.total_hint = self.measure(sources).await.ok().map(|m| m.bytes);
        match self.locations.to_local_path(&target) {
            Some(path) => self.compress_local(sources, &path, &opts).await?,
            None => compress_remote(self, sources, provider.as_ref(), &target, &opts).await?,
        }
        self.invalidate(dest_dir);
        Ok(target)
    }

    async fn compress_local(&self, sources: &[Uri], path: &Path, opts: &CompressOptions) -> Result<()> {
        let part = part_path(path);
        let sink = {
            let part = part.clone();
            tokio::task::spawn_blocking(move || {
                std::fs::OpenOptions::new()
                    .write(true)
                    .create_new(true)
                    .open(part)
            })
            .await
            .map_err(join_err)??
        };
        let done = compress(&self.locations, sources, sink, opts).await;
        let (from, to) = (part.clone(), path.to_path_buf());
        let failed = done.is_err();
        let finished = tokio::task::spawn_blocking(move || -> Result<()> {
            if failed {
                let _ = std::fs::remove_file(&from);
                return Ok(());
            }
            sys::rename_noreplace(&from, &to).map_err(|e| {
                let _ = std::fs::remove_file(&from);
                Error::from(e)
            })
        })
        .await
        .map_err(join_err)?;
        done.map(|_| ())?;
        finished
    }

    // -------------------------------------------------------- trash

    /// The *Trash*, newest first (OPS-8).
    pub fn trash_entries(&self, now_secs: i64) -> Result<Vec<TrashEntry>> {
        let settings = self.settings();
        let retention = settings.recently_deleted_retention_days;
        let mut items = self.trash.list()?;
        items.sort_by(|a, b| b.trashed_at.cmp(&a.trashed_at).then(b.id.cmp(&a.id)));
        Ok(items
            .into_iter()
            .map(|item| {
                let folder = item.original_uri.parent();
                let name = item.original_uri.name().unwrap_or_default();
                let kind = if item.is_dir {
                    None
                } else {
                    mime::extension_of(name).and_then(|e| mime::from_extension(&e))
                };
                TrashEntry {
                    id: item.id,
                    name: display_name(name),
                    original_uri: item.original_uri.to_string(),
                    folder_uri: folder.as_ref().map(Uri::to_string).unwrap_or_default(),
                    folder_name: self.display_folder(&item.original_uri),
                    is_dir: item.is_dir,
                    size: item.size,
                    trashed_at: item.trashed_at,
                    days_left: days_left(item.trashed_at, now_secs, retention),
                    mime_type: kind.map_or_else(String::new, |(m, _)| m.to_owned()),
                    thumbnail_source: trash_thumbnail(&item, &settings),
                }
            })
            .collect())
    }

    /// Removes what is older than the retention setting from the Trash
    /// (OPS-8). Returns the number of items removed.
    pub fn purge_trash(&self, now_secs: i64) -> Result<usize> {
        let retention = self.settings().recently_deleted_retention_days;
        self.trash
            .purge_before(now_secs.saturating_sub(i64::from(retention) * SECS_PER_DAY))
    }

    /// The Trash's work at start (OPS-8), once the settings are known: the
    /// expired items go, then the copies that an interruption left half made
    /// ([`Trash::tidy`](crate::trash::Trash::tidy)). Returns the number of
    /// items purged. Blocking.
    pub fn tidy_trash(&self, now_secs: i64) -> Result<usize> {
        let purged = self.purge_trash(now_secs);
        let tidied = self.trash.tidy();
        purged.and_then(|n| tidied.map(|()| n))
    }

    /// Empties the Trash: every item, and what a crash left in the trash
    /// folder (OPS-8). Returns the number of items removed.
    pub fn empty_trash(&self) -> Result<usize> {
        self.trash.empty()
    }

    // -------------------------------------------------------- share

    /// Which received files Lautta can read and where they live (INT-1).
    /// Only paths inside a location that the app can open are readable.
    pub fn probe_shared(&self, paths: &[PathBuf]) -> Vec<SharedFile> {
        paths
            .iter()
            .map(|path| {
                let meta = std::fs::metadata(path).ok().filter(std::fs::Metadata::is_file);
                let opened = meta.is_some() && std::fs::File::open(path).is_ok();
                let uri = self.locations.uri_for_local_path(path);
                SharedFile {
                    path: path.display().to_string(),
                    name: path
                        .file_name()
                        .map(|n| n.to_string_lossy().into_owned())
                        .unwrap_or_default(),
                    size: meta.map_or(0, |m| m.len()),
                    readable: opened && uri.is_some(),
                    uri: uri.map(|u| u.to_string()).unwrap_or_default(),
                }
            })
            .collect()
    }

    /// The writable locations offered as share destinations.
    pub fn destinations(&self) -> Vec<Destination> {
        self.locations
            .locations()
            .into_iter()
            .filter(|l| !matches!(l.kind, crate::locations::LocationKind::Archive))
            .filter(|l| self.provider(&l.id).is_ok_and(|p| p.capabilities().writable()))
            .map(|l| Destination {
                uri: Uri::root(l.id.clone()).to_string(),
                name: l.name.clone(),
                kind: location_kind_name(&l.kind).to_owned(),
            })
            .collect()
    }
}

/// `name` when no item of that name exists, else "name 2.ext", ….
fn free_name(name: &[u8], taken: &HashSet<Vec<u8>>) -> Vec<u8> {
    if taken.contains(name) {
        keep_both_name(name, &|n| taken.contains(n))
    } else {
        name.to_vec()
    }
}

fn part_path(path: &Path) -> PathBuf {
    let name = path
        .file_name()
        .map(|n| n.to_string_lossy().into_owned())
        .unwrap_or_default();
    path.with_file_name(format!(".{name}.lautta-part"))
}

/// Archive into a pipe and upload its read end while it is written
/// (PRV-11). Whichever side fails, the half-written file is removed.
async fn compress_remote(
    core: &Core,
    sources: &[Uri],
    provider: &dyn Provider,
    target: &Uri,
    opts: &CompressOptions,
) -> Result<()> {
    let (read_end, write_end) = rustix::pipe::pipe().map_err(|e| Error::from(std::io::Error::from(e)))?;
    let sink = std::fs::File::from(write_end);
    let write_opts = WriteOptions {
        disposition: Disposition::Create,
        ..WriteOptions::default()
    };
    let (packed, uploaded) = tokio::join!(
        compress(&core.locations, sources, sink, opts),
        provider.upload_from(read_end, &target.path, write_opts, opts.progress.clone()),
    );
    let failure = match (packed, uploaded) {
        (Ok(_), Ok(())) => return Ok(()),
        (Err(e), _) | (_, Err(e)) => e,
    };
    let _ = provider.remove_file(&target.path).await;
    Err(failure)
}

/// The engineering names of conflict choices (QML ⇄ core, OPS-2).
pub fn parse_choice(name: &str) -> Option<ConflictChoice> {
    Some(match name {
        "Replace" => ConflictChoice::Replace,
        "Skip" => ConflictChoice::Skip,
        "KeepBoth" => ConflictChoice::KeepBoth,
        "Merge" => ConflictChoice::Merge,
        "ReplaceIfNewer" => ConflictChoice::ReplaceIfNewer,
        "Resume" => ConflictChoice::Resume,
        _ => return None,
    })
}

#[cfg(test)]
mod tests;
