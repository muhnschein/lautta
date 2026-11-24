// SPDX-License-Identifier: LGPL-2.1-or-later
//! Operations area on `Core`: plan summaries, pending plans, conflicts, compress and extract,
//! Trash, share. Qt-free.

use crate::app::{Core, Started};
use crate::app_directory::file_url;
use crate::app_home::RecentLook;
use crate::compress::{compress, ArchiveKind, CompressOptions};
use crate::entry::{cap, Entry, Kind};
use crate::error::{Error, ErrorKind, Result};
use crate::locations::LocationKind;
use crate::mime;
use crate::ops::conflict as conflict_ops;
use crate::ops::names::{keep_both_dir_name, keep_both_name, split_extension};
use crate::ops::{Conflict, ConflictChoice, OperationKind, Plan, PlanItem};
use crate::password::Password;
use crate::provider::{list_all, Disposition, Lane, Provider, WriteOptions};
use crate::settings::Settings;
use crate::sys;
use crate::thumbs::{thumbnail_source, ThumbSettings, ThumbSource};
use crate::transfer::TransferId;
use crate::trash::TrashItem;
use crate::uri::Uri;
use crate::vpath::{display_name, validate_name};
use serde::Serialize;
use serde_json::Value;
use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Mutex;

const CRUMB: &str = " › ";
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

/// A plan is lent out while a conflict answer works on it across awaits; starting it meanwhile
/// waits for it to come back, discarding it meanwhile drops it on return.
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

    pub fn insert(&self, plan: Plan) -> u64 {
        let id = self.next.fetch_add(1, Ordering::Relaxed) + 1;
        guard(&self.plans).insert(id, Slot::Ready(Box::new(plan)));
        id
    }

    pub fn take(&self, id: u64) -> Option<Plan> {
        self.take_unless_lent(id).ok().flatten()
    }

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

    pub fn lend(&self, id: u64) -> Option<Plan> {
        let mut plans = guard(&self.plans);
        match std::mem::replace(plans.get_mut(&id)?, Slot::Lent) {
            Slot::Ready(plan) => Some(*plan),
            Slot::Lent => None,
        }
    }

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

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct RenamePair {
    pub from: String,
    pub to: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct StartOptions {
    pub verify_checksums: bool,
    pub preserve_mtime: bool,
    pub preserve_mode: bool,
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
    pub destination_folder: String,
    pub origin: Option<Origin>,
    pub free_bytes: Option<u64>,
    pub renames: Vec<RenamePair>,
    /// Offered only when both sides keep POSIX modes.
    pub permissions_supported: bool,
    pub options: StartOptions,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Origin {
    pub uri: String,
    pub name: String,
    pub folder: String,
    pub more: u64,
    pub is_dir: bool,
    pub look: RecentLook,
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

/// Every renamed item and what lies below it goes back to the source's own name.
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

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ConflictEntry {
    pub index: usize,
    pub name: String,
    pub folder: String,
    pub source: String,
    pub destination: String,
    pub conflict: Conflict,
    pub source_look: RecentLook,
    pub destination_look: RecentLook,
    /// Never `Replace`.
    pub default_choice: ConflictChoice,
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize)]
pub struct Measure {
    pub files: u64,
    pub dirs: u64,
    pub bytes: u64,
}

impl ArchiveKind {
    pub fn parse(text: &str) -> Option<ArchiveKind> {
        match text {
            "zip" => Some(ArchiveKind::Zip),
            "7z" => Some(ArchiveKind::SevenZ),
            "tar.xz" => Some(ArchiveKind::TarXz),
            "tar.gz" | "targz" | "tgz" => Some(ArchiveKind::TarGz),
            _ => None,
        }
    }

    pub fn extension(self) -> &'static str {
        match self {
            ArchiveKind::Zip => ".zip",
            ArchiveKind::SevenZ => ".7z",
            ArchiveKind::TarXz => ".tar.xz",
            ArchiveKind::TarGz => ".tar.gz",
        }
    }
}

pub fn archive_file_name(name: &str, kind: ArchiveKind) -> String {
    let ext = kind.extension();
    if name.to_lowercase().ends_with(ext) {
        name.to_owned()
    } else {
        format!("{name}{ext}")
    }
}

pub fn archive_folder_name(name: &str) -> String {
    let (stem, _) = split_extension(name.as_bytes(), false);
    let stem = String::from_utf8_lossy(stem).into_owned();
    if stem.is_empty() {
        name.to_owned()
    } else {
        stem
    }
}

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
    pub trashed_at: i64,
    pub days_left: i64,
    /// Stored copy has no extension; the system thumbnailer goes by this.
    pub mime_type: String,
    pub thumbnail_source: String,
}

/// Original name decides ([`thumbnail_source`]); the stored copy lost it. Empty: thumbnails off,
/// rule says none, or stored item is no regular file. A trashed link would show its target, maybe
/// outside the sandbox.
fn trash_thumbnail(item: &TrashItem, settings: &Settings) -> String {
    let kind = if item.is_dir { Kind::Dir } else { Kind::File };
    let mut entry = Entry::new(item.original_uri.name().unwrap_or_default(), kind);
    entry.size = item.size;
    let set = ThumbSettings::new(settings);
    let source = thumbnail_source(&entry, true, settings.thumbnails_local, &set);
    let stored = item.stored_path();
    if source != ThumbSource::System || !std::fs::symlink_metadata(&stored).is_ok_and(|m| m.is_file()) {
        return String::new();
    }
    file_url(&stored)
}

pub fn days_left(trashed_at: i64, now: i64, retention_days: u32) -> i64 {
    let ends = trashed_at.saturating_add(i64::from(retention_days) * SECS_PER_DAY);
    ((ends - now).max(0) + SECS_PER_DAY - 1) / SECS_PER_DAY
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SharedFile {
    pub path: String,
    pub name: String,
    pub size: u64,
    pub readable: bool,
    pub uri: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct ShareFolder {
    pub uri: String,
    pub name: String,
    pub place: String,
    pub remote: bool,
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize)]
pub struct SharePlaces {
    pub start: String,
    pub recent: Vec<ShareFolder>,
}

const SHARE_FIRST_PLACE: &str = "user-downloads";

impl Core {
    pub fn display_path(&self, uri: &Uri) -> String {
        let mut parts = vec![self
            .location(&uri.location)
            .map_or_else(|| uri.location.clone(), |l| l.name)];
        parts.extend(uri.path.components().map(display_name));
        parts.join(CRUMB)
    }

    pub fn display_folder(&self, uri: &Uri) -> String {
        uri.parent()
            .map_or_else(|| self.display_path(uri), |p| self.display_path(&p))
    }

    pub fn display_title(&self, uri: &Uri) -> String {
        uri.name().map_or_else(
            || {
                self.location(&uri.location)
                    .map_or_else(|| uri.location.clone(), |l| l.name)
            },
            display_name,
        )
    }

    fn parent_title(&self, uri: &Uri) -> String {
        uri.parent().map(|p| self.display_title(&p)).unwrap_or_default()
    }

    fn archive_file(&self, uri: &Uri) -> Option<Uri> {
        let location = self.location(&uri.location)?;
        if location.kind != LocationKind::Archive {
            return None;
        }
        Uri::parse(location.url.as_deref()?).ok()
    }

    fn origin(&self, plan: &Plan, settings: &Settings) -> Option<Origin> {
        let tops: Vec<&PlanItem> = plan
            .items
            .iter()
            .filter(|it| it.dst.parent().as_ref() == Some(&plan.destination))
            .collect();
        let first = tops.first()?;
        let archive = match plan.kind {
            OperationKind::Extract => self.archive_file(&first.src),
            _ => None,
        };
        let (uri, is_dir, more) = match archive {
            Some(file) => (file, false, 0),
            None => (first.src.clone(), first.kind == Kind::Dir, tops.len() as u64 - 1),
        };
        Some(Origin {
            name: self.display_title(&uri),
            folder: self.parent_title(&uri),
            more,
            is_dir,
            look: self.item_look(&uri, is_dir, settings),
            uri: uri.to_string(),
        })
    }

    pub async fn plan_summary(&self, plan: &Plan) -> PlanSummary {
        let dest = &plan.destination;
        let free_bytes = match self.provider(&dest.location) {
            // Extract's new folder is made on start: until then, its parent's space.
            Ok(p) => match p.space(&dest.path).await {
                Ok(s) => Some(s.free),
                Err(_) => match dest.path.parent() {
                    Some(up) => p.space(&up).await.ok().map(|s| s.free),
                    None => None,
                },
            },
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
        let settings = self.settings();
        PlanSummary {
            kind: kind_name(plan.kind).to_owned(),
            files: plan.totals.files,
            dirs: plan.totals.dirs,
            bytes: plan.totals.bytes,
            conflicts: plan.totals.conflicts,
            renamed: plan.totals.renamed,
            destination: dest.to_string(),
            destination_name: self.display_title(dest),
            destination_folder: self.parent_title(dest),
            origin: self.origin(plan, &settings),
            free_bytes,
            renames,
            permissions_supported,
            options: StartOptions::from_settings(&settings),
        }
    }

    /// Options differ from the settings for this start only: settings are put back right after
    /// queueing.
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

    pub fn unresolved_conflicts(&self, plan: &Plan) -> Vec<ConflictEntry> {
        let settings = self.settings();
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
                    source_look: self.item_look(&item.src, conflict.src_is_dir, &settings),
                    destination_look: self.item_look(&item.dst, conflict.dst_is_dir, &settings),
                    conflict,
                })
            })
            .collect()
    }

    pub async fn resolve_plan_conflict(
        &self,
        plan: &mut Plan,
        index: usize,
        choice: ConflictChoice,
        apply_to_all: bool,
    ) -> Result<usize> {
        conflict_ops::resolve(plan, index, choice, apply_to_all, &self.locations).await
    }

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

    /// Next to the archive, as macOS does: a single top item (folder or file) comes out as is,
    /// anything else in a new folder named after the archive. Taken name gets a number: nothing
    /// merges, nothing is replaced.
    /// `password` only for a locked archive not open yet.
    pub async fn extract(&self, archive: &Uri, password: Option<Password>) -> Result<Started> {
        let folder = archive
            .parent()
            .ok_or_else(|| Error::new(ErrorKind::InvalidArgument, "archive is no file"))?;
        let target = self.provider(&folder.location)?;
        if !self.may_add_to(&folder) {
            return Err(Error::kind(ErrorKind::ReadOnlyFilesystem));
        }
        let (location, hold) = self.open_archive(archive, password).await?;
        let root = Uri::root(location.clone());
        let source = self.provider(&location)?;
        let children = list_all(source.as_ref(), &root.path, Lane::Interactive).await?;
        let (sources, dest, made) = match children.as_slice() {
            [] => return Err(Error::new(ErrorKind::InvalidArgument, "the archive is empty")),
            [only] => (vec![root.join(&only.name)?], folder.clone(), false),
            many => {
                let taken: HashSet<Vec<u8>> = list_all(target.as_ref(), &folder.path, Lane::Interactive)
                    .await?
                    .into_iter()
                    .map(|e| e.name)
                    .collect();
                let name = archive_folder_name(&archive.name().map(display_name).unwrap_or_default());
                let name = free_dir_name(name.as_bytes(), &taken);
                let dest = folder.join(&name)?;
                // Exclusive: a folder made meanwhile fails the extract rather than merging.
                target.make_dir(&dest.path, true).await?;
                let sources = many
                    .iter()
                    .map(|e| root.join(&e.name))
                    .collect::<Result<Vec<_>>>()?;
                (sources, dest, true)
            }
        };
        let planned = self.plan(OperationKind::Copy, sources, dest.clone()).await;
        // Planner needs the folder; start_plan makes it again, so an unstarted summary leaves none.
        // Best effort: nothing is in it yet.
        if made {
            let _ = target.remove_dir(&dest.path).await;
        }
        self.invalidate(&folder);
        let mut plan = planned?;
        if dest == folder && plan.items.first().is_some_and(|it| it.conflict.is_some()) {
            conflict_ops::resolve(&mut plan, 0, ConflictChoice::KeepBoth, false, &self.locations).await?;
            plan.totals.conflicts = conflict_ops::unresolved(&plan).len() as u64;
        }
        plan.kind = OperationKind::Extract;
        plan.hold = hold;
        if plan.needs_summary {
            return Ok(Started::NeedsSummary(Box::new(plan)));
        }
        self.start_plan(plan).await.map(Started::Transfer)
    }

    /// Failed or canceled run leaves no partial file. `opts`: kind, password, progress, cancel; the
    /// rest is filled here.
    pub async fn compress_to(
        &self,
        sources: &[Uri],
        dest_dir: &Uri,
        name: &str,
        mut opts: CompressOptions,
    ) -> Result<Uri> {
        let provider = self.provider(&dest_dir.location)?;
        if !self.may_add_to(dest_dir) {
            return Err(Error::kind(ErrorKind::ReadOnlyFilesystem));
        }
        let file_name = archive_file_name(name.trim(), opts.kind);
        validate_name(file_name.as_bytes())?;
        let taken: HashSet<Vec<u8>> = list_all(provider.as_ref(), &dest_dir.path, Lane::Interactive)
            .await?
            .into_iter()
            .map(|e| e.name)
            .collect();
        let final_name = free_name(file_name.as_bytes(), &taken);
        let target = dest_dir.join(&final_name)?;
        opts.total_hint = self.measure(sources).await.ok().map(|m| m.bytes);
        opts.spool_dir = Some(self.paths.cache_dir());
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

    pub fn purge_trash(&self, now_secs: i64) -> Result<usize> {
        let retention = self.settings().recently_deleted_retention_days;
        self.trash
            .purge_before(now_secs.saturating_sub(i64::from(retention) * SECS_PER_DAY))
    }

    pub fn tidy_trash(&self, now_secs: i64) -> Result<usize> {
        let purged = self.purge_trash(now_secs);
        let tidied = self.trash.tidy();
        purged.and_then(|n| tidied.map(|()| n))
    }

    pub fn empty_trash(&self) -> Result<usize> {
        self.trash.empty()
    }

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

    pub fn share_places(&self) -> SharePlaces {
        let recent: Vec<ShareFolder> = self
            .settings()
            .share_folders
            .iter()
            .filter_map(|folder| Uri::parse(folder).ok())
            .filter(|uri| self.share_folder_there(uri))
            .map(|uri| self.share_folder(&uri))
            .collect();
        let start = recent
            .first()
            .map(|folder| folder.uri.clone())
            .or_else(|| {
                self.location(SHARE_FIRST_PLACE)
                    .map(|l| Uri::root(l.id).to_string())
            })
            .unwrap_or_default();
        SharePlaces { start, recent }
    }

    fn share_folder_there(&self, uri: &Uri) -> bool {
        let Some(location) = self.location(&uri.location) else {
            return false;
        };
        !location.is_local() || self.locations.to_local_path(uri).is_some_and(|p| p.is_dir())
    }

    fn share_folder(&self, uri: &Uri) -> ShareFolder {
        ShareFolder {
            uri: uri.to_string(),
            name: uri.name().map_or_else(|| self.place_of(uri), display_name),
            place: uri.parent().map(|up| self.place_of(&up)).unwrap_or_default(),
            remote: self.location(&uri.location).is_some_and(|l| !l.is_local()),
        }
    }
}

fn free_dir_name(name: &[u8], taken: &HashSet<Vec<u8>>) -> Vec<u8> {
    if taken.contains(name) {
        keep_both_dir_name(name, &|n| taken.contains(n))
    } else {
        name.to_vec()
    }
}

pub(crate) fn free_name(name: &[u8], taken: &HashSet<Vec<u8>>) -> Vec<u8> {
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

/// Whichever side fails, the half-written file is removed.
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
