// SPDX-License-Identifier: LGPL-2.1-or-later
//! The *Trash* (SPEC OPS-8): what is deleted from this device's folders
//! waits in one folder of the app data, `AppPaths::trash_dir` (DAT-1), until
//! it is restored, deleted or purged. Volumes and servers delete permanently.
//!
//! Each item gets a unique stored name and a `trash_items` row (DAT-2) that
//! remembers where it came from. An item moves in by `rename` where it can.
//! In the sandbox it cannot: Sailjail bind-mounts every permitted folder and
//! the app data folder on their own (§3.3), and `rename(2)` between two
//! mounts fails with `EXDEV` even on one file system. The item is then
//! copied in (folders, symlinks, modes and modification times) and removed
//! from its folder only once the copy and its row are in place; a failed
//! copy leaves it where it was. Restore goes back the same way.
//!
//! Schemas 4 and 5 kept a bin at the root of each location instead
//! (`<root>/.lautta-trash`, [`old_bin_of`]); the rows with a `bin` are still
//! there until [`Trash::tidy`] has moved them in. All calls block: run them
//! in `spawn_blocking` (ARC-6). The calls that move or remove items take
//! turns, so a drain, a restore and an empty never work on one item at once.

use crate::db::Db;
use crate::error::{Error, ErrorKind, Result};
use crate::sys;
use crate::uri::Uri;
use rusqlite::params;
use std::collections::{BTreeSet, HashSet};
use std::ffi::OsStr;
use std::os::unix::ffi::OsStrExt;
use std::os::unix::fs::{DirBuilderExt, PermissionsExt};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex, MutexGuard, PoisonError};
use std::time::{SystemTime, UNIX_EPOCH};

/// Items are purged after this many days (OPS-8).
pub const DEFAULT_RETENTION_DAYS: u32 = 30;
/// The bin a location kept at its root in schemas 4 and 5 (OPS-8).
pub const OLD_BIN_NAME: &str = ".lautta-trash";
/// A copy on its way into place has a name starting so; one that an
/// interruption left in the trash folder goes at the next [`Trash::tidy`].
const PARTIAL_PREFIX: &str = ".lautta-partial-";
const SECS_PER_DAY: i64 = 86_400;
const NAME_ATTEMPTS: usize = 8;
const ITEM_COLUMNS: &str = "id, original_uri, trashed_at, stored_name, is_dir, size, bin";

/// The bin of schemas 4 and 5 at the root of a location.
pub fn old_bin_of(root: &Path) -> PathBuf {
    root.join(OLD_BIN_NAME)
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TrashItem {
    pub id: i64,
    pub original_uri: Uri,
    /// Unix seconds.
    pub trashed_at: i64,
    pub stored_name: String,
    pub is_dir: bool,
    /// Bytes of file content (recursive for folders).
    pub size: Option<u64>,
    /// The folder holding the item: the trash folder, or the old bin of an
    /// item from schemas 4 and 5 that is not drained yet.
    pub bin: PathBuf,
}

impl TrashItem {
    pub fn stored_path(&self) -> PathBuf {
        self.bin.join(&self.stored_name)
    }
}

/// How an item got to its new place.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Moved {
    /// On one mount: the item is gone from where it was.
    Renamed,
    /// Across mounts: the original is still there, for the caller to remove.
    Copied,
}

/// What draining one item of an old bin came to.
enum Drained {
    Moved,
    /// Nothing to move: the item is gone (its row too), or it moved already.
    Nothing,
    /// Its location is not there now: next time.
    Waiting,
}

/// The rename that moves an item between folders. Tests swap in one that
/// fails as between two mounts.
type RenameFn = fn(&Path, &Path) -> Result<()>;

#[derive(Clone)]
pub struct Trash {
    /// The trash folder (DAT-1).
    dir: PathBuf,
    db: Db,
    /// Held by every call that moves or removes items.
    turn: Arc<Mutex<()>>,
    rename: RenameFn,
}

/// Unix seconds now.
pub fn now_secs() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_or(0, |d| i64::try_from(d.as_secs()).unwrap_or(i64::MAX))
}

/// A name no other process or thread picks: time, process and a counter.
fn unique_name(prefix: &str) -> String {
    static COUNTER: AtomicU64 = AtomicU64::new(0);
    let ms = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_or(0, |d| d.as_millis());
    let n = COUNTER.fetch_add(1, Ordering::Relaxed);
    format!("{prefix}{ms:013x}-{:x}-{n:x}", std::process::id())
}

/// True for a real folder, not a symlink to one.
fn is_real_dir(path: &Path) -> bool {
    sys::stat(path, false).is_ok_and(|st| st.is_dir())
}

fn exists(path: &Path) -> bool {
    sys::stat(path, false).is_ok()
}

impl Trash {
    /// `dir` is the trash folder (`AppPaths::trash_dir`), made 0700 (SEC-5)
    /// on the first delete.
    pub fn new(dir: PathBuf, db: Db) -> Trash {
        Trash {
            dir,
            db,
            turn: Arc::default(),
            rename: move_without_replace,
        }
    }

    /// The trash folder.
    pub fn dir(&self) -> &Path {
        &self.dir
    }

    fn turn(&self) -> MutexGuard<'_, ()> {
        self.turn.lock().unwrap_or_else(PoisonError::into_inner)
    }

    /// Refuses to trash the trash folder, an old bin that still holds
    /// items, or what is in or holds one of them.
    fn guard(&self, path: &Path) -> Result<()> {
        let overlaps = |dir: &Path| path.starts_with(dir) || dir.starts_with(path);
        if overlaps(&self.dir) || self.pending_bins()?.iter().any(|b| overlaps(b)) {
            return Err(Error::new(
                ErrorKind::InvalidArgument,
                "the trash cannot be moved into itself",
            ));
        }
        Ok(())
    }

    /// Moves `path` (a file, folder or symlink; never followed) into the
    /// trash and records where it came from. Across mounts the item is
    /// copied in and removed from its folder once its row is written; if
    /// that removal fails, the error is returned and the item stays in the
    /// trash too, so nothing is lost.
    pub fn trash(&self, path: &Path, original_uri: &Uri) -> Result<TrashItem> {
        let st = sys::stat(path, false)?;
        self.guard(path)?;
        let _turn = self.turn();
        std::fs::DirBuilder::new()
            .recursive(true)
            .mode(0o700)
            .create(&self.dir)?;
        let (stored_name, moved) = self.move_in(path)?;
        let item = TrashItem {
            id: 0,
            original_uri: original_uri.clone(),
            trashed_at: now_secs(),
            size: tree_size(&self.dir.join(&stored_name)),
            stored_name,
            is_dir: st.is_dir(),
            bin: self.dir.clone(),
        };
        let id = match self.insert_row(&item) {
            Ok(id) => id,
            Err(e) => {
                // Without its row the item would be lost: undo the move.
                let _ = match moved {
                    Moved::Renamed => std::fs::rename(item.stored_path(), path).map_err(Into::into),
                    Moved::Copied => remove_tree(&item.stored_path()),
                };
                return Err(e);
            }
        };
        if moved == Moved::Copied {
            remove_tree(path)?;
        }
        Ok(TrashItem { id, ..item })
    }

    /// Puts `path` into the trash folder under a fresh unique name.
    fn move_in(&self, path: &Path) -> Result<(String, Moved)> {
        for _ in 0..NAME_ATTEMPTS {
            let name = unique_name("");
            match self.put(path, &self.dir.join(&name)) {
                Ok(moved) => return Ok((name, moved)),
                Err(e) if e.kind == ErrorKind::AlreadyExists => {}
                Err(e) => return Err(e),
            }
        }
        Err(Error::new(ErrorKind::Internal, "no unique trash name"))
    }

    /// Puts `from` at `to`, which must not exist: by rename on one mount,
    /// else by a copy under a temporary name next to `to` that is renamed
    /// into place. The copy is a step of a move, so `from` has to be one
    /// this process can remove afterwards. A failed copy leaves `from` as it
    /// was and nothing at `to`.
    fn put(&self, from: &Path, to: &Path) -> Result<Moved> {
        match (self.rename)(from, to) {
            Err(e) if e.kind == ErrorKind::CrossesDevice => {}
            other => return other.map(|()| Moved::Renamed),
        }
        if exists(to) {
            return Err(Error::kind(ErrorKind::AlreadyExists));
        }
        if !from.parent().is_some_and(sys::can_change) {
            return Err(Error::new(
                ErrorKind::PermissionDenied,
                "the folder of the item cannot be changed",
            ));
        }
        let tmp = to.with_file_name(unique_name(PARTIAL_PREFIX));
        if let Err(e) = copy_tree(from, &tmp).and_then(|()| move_without_replace(&tmp, to)) {
            let _ = remove_tree(&tmp);
            return Err(e);
        }
        Ok(Moved::Copied)
    }

    fn insert_row(&self, item: &TrashItem) -> Result<i64> {
        let conn = self.db.lock();
        conn.execute(
            "INSERT INTO trash_items(original_uri, trashed_at, stored_name, is_dir, size) \
             VALUES (?1, ?2, ?3, ?4, ?5)",
            params![
                item.original_uri.to_string(),
                item.trashed_at,
                item.stored_name,
                item.is_dir,
                item.size.and_then(|s| i64::try_from(s).ok()),
            ],
        )?;
        Ok(conn.last_insert_rowid())
    }

    /// The folder of a row's `bin` column.
    fn bin_path(&self, bin: Option<Vec<u8>>) -> PathBuf {
        bin.map_or_else(|| self.dir.clone(), |b| PathBuf::from(OsStr::from_bytes(&b)))
    }

    /// All items, newest first. Rows whose URI cannot be parsed are skipped.
    pub fn list(&self) -> Result<Vec<TrashItem>> {
        let conn = self.db.lock();
        let mut stmt = conn.prepare(&format!(
            "SELECT {ITEM_COLUMNS} FROM trash_items ORDER BY trashed_at DESC, id DESC"
        ))?;
        let rows = stmt.query_map([], row_to_raw)?;
        let mut out = Vec::new();
        for raw in rows {
            match raw?.into_item(self) {
                Ok(item) => out.push(item),
                Err(e) => log::warn!("skipping unreadable trash row: {e}"),
            }
        }
        Ok(out)
    }

    pub fn get(&self, id: i64) -> Result<TrashItem> {
        let conn = self.db.lock();
        let raw = conn
            .query_row(
                &format!("SELECT {ITEM_COLUMNS} FROM trash_items WHERE id = ?1"),
                params![id],
                row_to_raw,
            )
            .map_err(|e| match e {
                rusqlite::Error::QueryReturnedNoRows => Error::new(ErrorKind::NotFound, "no such trash item"),
                other => other.into(),
            })?;
        raw.into_item(self)
    }

    /// The old bins (schemas 4 and 5) that still hold items.
    pub fn pending_bins(&self) -> Result<Vec<PathBuf>> {
        let conn = self.db.lock();
        let mut stmt = conn.prepare("SELECT DISTINCT bin FROM trash_items WHERE bin IS NOT NULL")?;
        let rows = stmt.query_map([], |r| r.get::<_, Vec<u8>>(0))?;
        let mut bins = BTreeSet::new();
        for bin in rows {
            bins.insert(PathBuf::from(OsStr::from_bytes(&bin?)));
        }
        Ok(bins.into_iter().collect())
    }

    fn delete_row(&self, id: i64) -> Result<()> {
        self.db
            .lock()
            .execute("DELETE FROM trash_items WHERE id = ?1", params![id])?;
        Ok(())
    }

    /// Restores to the original location, which `resolve` maps from the
    /// recorded URI to a local path (`None` when that location is gone).
    /// `AlreadyExists` if the name is taken: the caller picks another name
    /// and uses [`Trash::restore_to`].
    pub fn restore(&self, id: i64, resolve: &dyn Fn(&Uri) -> Option<PathBuf>) -> Result<PathBuf> {
        let item = self.get(id)?;
        let dest = resolve(&item.original_uri)
            .ok_or_else(|| Error::new(ErrorKind::NotFound, "the original location is not available"))?;
        self.restore_to(id, &dest)?;
        Ok(dest)
    }

    /// Restores to `dest`, creating missing parent folders. Never replaces.
    /// By rename on one mount, else by a copy that is removed from the trash
    /// once it is in place (§3.3).
    pub fn restore_to(&self, id: i64, dest: &Path) -> Result<()> {
        let _turn = self.turn();
        let item = self.get(id)?;
        let stored = item.stored_path();
        if !exists(&stored) {
            // The file vanished behind our back; forget the stale row.
            self.delete_row(id)?;
            return Err(Error::new(ErrorKind::NotFound, "the trashed item is gone"));
        }
        if let Some(parent) = dest.parent() {
            std::fs::create_dir_all(parent)?;
        }
        let moved = self.put(&stored, dest)?;
        self.delete_row(id)?;
        if moved == Moved::Copied {
            // The item is back: a copy left over is a stray that emptying
            // the trash removes.
            if let Err(e) = remove_tree(&stored) {
                log::warn!("a restored trash item stays in the trash folder: {e}");
            }
        }
        Ok(())
    }

    /// Deletes one item for good.
    pub fn delete(&self, id: i64) -> Result<()> {
        let _turn = self.turn();
        self.delete_unlocked(id)
    }

    fn delete_unlocked(&self, id: i64) -> Result<()> {
        let item = self.get(id)?;
        remove_tree(&item.stored_path())?;
        self.delete_row(id)
    }

    /// Deletes everything, and the strays without a row (left by a crash)
    /// in the trash folder. An old bin goes too once its items are deleted.
    /// Returns the number of items removed.
    pub fn empty(&self) -> Result<usize> {
        let _turn = self.turn();
        let old_bins = self.pending_bins()?;
        let mut first_error = None;
        let mut removed = 0;
        for item in self.list()? {
            match self.delete_unlocked(item.id) {
                Ok(()) => removed += 1,
                Err(e) => {
                    first_error.get_or_insert(e);
                }
            }
        }
        let known: HashSet<PathBuf> = self.list()?.iter().map(TrashItem::stored_path).collect();
        remove_strays(&self.dir, &known)?;
        for bin in &old_bins {
            remove_empty_bin(bin);
        }
        first_error.map_or(Ok(removed), Err)
    }

    /// Deletes items trashed before `cutoff` (unix seconds).
    pub fn purge_before(&self, cutoff: i64) -> Result<usize> {
        let _turn = self.turn();
        let old: Vec<i64> = self
            .list()?
            .into_iter()
            .filter(|i| i.trashed_at < cutoff)
            .map(|i| i.id)
            .collect();
        let mut removed = 0;
        for id in old {
            self.delete_unlocked(id)?;
            removed += 1;
        }
        Ok(removed)
    }

    /// Deletes items older than `days` days.
    pub fn purge_older_than(&self, days: u32) -> Result<usize> {
        self.purge_before(now_secs() - i64::from(days) * SECS_PER_DAY)
    }

    /// The default 30-day purge (OPS-8).
    pub fn purge_expired(&self) -> Result<usize> {
        self.purge_older_than(DEFAULT_RETENTION_DAYS)
    }

    /// Total bytes of everything in the trash (from the recorded sizes).
    pub fn total_size(&self) -> Result<u64> {
        Ok(self.list()?.iter().filter_map(|i| i.size).sum())
    }

    // ------------------------------------------------------ old bins

    /// Puts right what an interruption or an older version left (OPS-8):
    /// copies that never got into place go, and while the drain of schema 6
    /// is due, the items of the old bins move into the trash folder with
    /// their rows (same id, origin and deletion time). Then the emptied
    /// bins go: theirs and the one at each of `roots` (the locations that
    /// keep a trash). The drain is done once no item is left in an old bin;
    /// an item whose location is away now waits for the next call. Safe to
    /// repeat and to interrupt. Returns the number of items moved.
    pub fn tidy(&self, roots: &[PathBuf]) -> Result<usize> {
        {
            let _turn = self.turn();
            remove_partials(&self.dir)?;
        }
        if !self.drain_due()? {
            return Ok(0);
        }
        let mut bins: BTreeSet<PathBuf> = roots.iter().map(|r| old_bin_of(r)).collect();
        bins.extend(self.pending_bins()?);
        let (moved, waiting) = self.drain_rows()?;
        for bin in &bins {
            remove_empty_bin(bin);
        }
        if !waiting {
            self.db.lock().execute("DELETE FROM trash_drain", [])?;
        }
        Ok(moved)
    }

    fn drain_due(&self) -> Result<bool> {
        let due: i64 = self
            .db
            .lock()
            .query_row("SELECT COUNT(*) FROM trash_drain", [], |r| r.get(0))?;
        Ok(due > 0)
    }

    /// Drains every item of an old bin, one turn each so that the app can
    /// go on using the trash in between. Returns how many moved and whether
    /// any has to wait.
    fn drain_rows(&self) -> Result<(usize, bool)> {
        let ids: Vec<i64> = {
            let conn = self.db.lock();
            let mut stmt = conn.prepare("SELECT id FROM trash_items WHERE bin IS NOT NULL")?;
            let rows = stmt.query_map([], |r| r.get(0))?;
            rows.collect::<rusqlite::Result<_>>()?
        };
        let (mut moved, mut waiting) = (0, false);
        for id in ids {
            let _turn = self.turn();
            match self.drain_one(id) {
                Ok(Drained::Moved) => moved += 1,
                Ok(Drained::Nothing) => {}
                Ok(Drained::Waiting) => waiting = true,
                Err(e) => {
                    log::warn!("trash item {id} stays in its old bin for now: {e}");
                    waiting = true;
                }
            }
        }
        Ok((moved, waiting))
    }

    /// Moves one item of an old bin into the trash folder under its stored
    /// name, which no other row has (the column is unique). Copies get into
    /// place only when complete, so one found there is the item, and what
    /// is left in the bin after an interruption is a duplicate.
    fn drain_one(&self, id: i64) -> Result<Drained> {
        let (stored_name, bin): (String, Option<Vec<u8>>) = self.db.lock().query_row(
            "SELECT stored_name, bin FROM trash_items WHERE id = ?1",
            params![id],
            |r| Ok((r.get(0)?, r.get(1)?)),
        )?;
        let Some(bin) = bin.map(|b| PathBuf::from(OsStr::from_bytes(&b))) else {
            return Ok(Drained::Nothing);
        };
        let (from, to) = (bin.join(&stored_name), self.dir.join(&stored_name));
        if !exists(&to) {
            // Never through a bin that is not a real folder (a planted link).
            if !is_real_dir(&bin) || !exists(&from) {
                return self.vanished(id, &bin);
            }
            std::fs::DirBuilder::new()
                .recursive(true)
                .mode(0o700)
                .create(&self.dir)?;
            self.put(&from, &to)?;
        }
        if is_real_dir(&bin) {
            remove_tree(&from)?;
        }
        self.db
            .lock()
            .execute("UPDATE trash_items SET bin = NULL WHERE id = ?1", params![id])?;
        Ok(Drained::Moved)
    }

    /// An item missing from its old bin: gone for good when the bin's
    /// location is there (the row goes), else the location is away for now.
    fn vanished(&self, id: i64, bin: &Path) -> Result<Drained> {
        if bin.parent().is_none_or(|root| sys::stat(root, true).is_err()) {
            return Ok(Drained::Waiting);
        }
        self.delete_row(id)?;
        Ok(Drained::Nothing)
    }
}

/// Removes an old bin once it is empty. One that still holds something (an
/// item waiting for its location, or a stray that a crash left without a
/// row) stays: nothing is deleted that the trash does not know.
pub fn remove_empty_bin(bin: &Path) {
    if is_real_dir(bin) {
        let _ = std::fs::remove_dir(bin);
    }
}

/// Removes the copies an interruption left on their way into `dir`. Called
/// with the turn held, so no copy is under way.
fn remove_partials(dir: &Path) -> Result<()> {
    let entries = match std::fs::read_dir(dir) {
        Ok(entries) => entries,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(()),
        Err(e) => return Err(e.into()),
    };
    for entry in entries {
        let entry = entry?;
        if entry
            .file_name()
            .as_bytes()
            .starts_with(PARTIAL_PREFIX.as_bytes())
        {
            remove_tree(&entry.path())?;
        }
    }
    Ok(())
}

/// Removes what is in `dir` but not in `known`. A folder that is not a real
/// one is left alone: never sweep through a symlink.
fn remove_strays(dir: &Path, known: &HashSet<PathBuf>) -> Result<()> {
    if !is_real_dir(dir) {
        return Ok(());
    }
    for entry in std::fs::read_dir(dir)? {
        let path = entry?.path();
        if !known.contains(&path) {
            remove_tree(&path)?;
        }
    }
    Ok(())
}

struct RawRow {
    id: i64,
    uri: String,
    trashed_at: i64,
    stored_name: String,
    is_dir: bool,
    size: Option<i64>,
    bin: Option<Vec<u8>>,
}

impl RawRow {
    fn into_item(self, trash: &Trash) -> Result<TrashItem> {
        Ok(TrashItem {
            id: self.id,
            original_uri: Uri::parse(&self.uri)?,
            trashed_at: self.trashed_at,
            stored_name: self.stored_name,
            is_dir: self.is_dir,
            size: self.size.and_then(|s| u64::try_from(s).ok()),
            bin: trash.bin_path(self.bin),
        })
    }
}

fn row_to_raw(r: &rusqlite::Row<'_>) -> rusqlite::Result<RawRow> {
    Ok(RawRow {
        id: r.get(0)?,
        uri: r.get(1)?,
        trashed_at: r.get(2)?,
        stored_name: r.get(3)?,
        is_dir: r.get(4)?,
        size: r.get(5)?,
        bin: r.get(6)?,
    })
}

/// Rename that never replaces; on filesystems without `RENAME_NOREPLACE`
/// it falls back to an exists-check. `CrossesDevice` between two mounts.
fn move_without_replace(from: &Path, to: &Path) -> Result<()> {
    match sys::rename_noreplace(from, to) {
        Ok(()) => Ok(()),
        Err(e) if sys::is_flag_unsupported(&e) => {
            if exists(to) {
                return Err(Error::kind(ErrorKind::AlreadyExists));
            }
            std::fs::rename(from, to).map_err(Into::into)
        }
        Err(e) => Err(e.into()),
    }
}

/// Copies a file, a symlink (as a link) or a folder tree, with permission
/// bits and modification times. Other kinds of files are refused.
fn copy_tree(from: &Path, to: &Path) -> Result<()> {
    let st = sys::stat(from, false)?;
    if st.is_symlink() {
        std::os::unix::fs::symlink(std::fs::read_link(from)?, to)?;
        return Ok(());
    }
    if st.is_dir() {
        std::fs::DirBuilder::new().mode(0o700).create(to)?;
        copy_children(from, to)?;
    } else if st.is_file() {
        std::fs::copy(from, to)?;
    } else {
        return Err(Error::new(ErrorKind::Unsupported, "not a regular file"));
    }
    // Folders get their mode only now: a read-only one could not be filled.
    std::fs::set_permissions(to, std::fs::Permissions::from_mode(st.permissions()))?;
    sys::set_mtime(to, st.mtime)?;
    Ok(())
}

/// Copies what the folder `from` holds into `to`. A folder with something
/// in it must be one this process can empty: the copy is a step of a move,
/// and an original that could not be removed would stay half deleted.
fn copy_children(from: &Path, to: &Path) -> Result<()> {
    let mut entries = std::fs::read_dir(from)?.peekable();
    if entries.peek().is_some() && !sys::can_change(from) {
        return Err(Error::new(
            ErrorKind::PermissionDenied,
            "a folder inside the item cannot be emptied",
        ));
    }
    for entry in entries {
        let entry = entry?;
        copy_tree(&entry.path(), &to.join(entry.file_name()))?;
    }
    Ok(())
}

/// Bytes of regular files below `path` (or of `path` itself). Symlinks are
/// counted by their own size and never followed; unreadable parts are skipped.
fn tree_size(path: &Path) -> Option<u64> {
    let top = sys::stat(path, false).ok()?;
    if !top.is_dir() {
        return Some(top.size);
    }
    let mut total = 0u64;
    let mut stack = vec![path.to_path_buf()];
    while let Some(dir) = stack.pop() {
        let Ok(rd) = std::fs::read_dir(&dir) else {
            continue;
        };
        for entry in rd.flatten() {
            let Ok(st) = sys::stat(&entry.path(), false) else {
                continue;
            };
            if st.is_dir() {
                stack.push(entry.path());
            } else {
                total = total.saturating_add(st.size);
            }
        }
    }
    Some(total)
}

/// Removes a file, symlink or folder tree without following symlinks.
/// A path that is already gone is fine.
pub fn remove_tree(path: &Path) -> Result<()> {
    let st = match sys::stat(path, false) {
        Ok(st) => st,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(()),
        Err(e) => return Err(e.into()),
    };
    if st.is_dir() {
        // std's remove_dir_all unlinks symlinks instead of descending into them.
        std::fs::remove_dir_all(path)?;
    } else {
        std::fs::remove_file(path)?;
    }
    Ok(())
}

#[cfg(test)]
mod tests;
