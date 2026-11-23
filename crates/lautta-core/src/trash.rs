// SPDX-License-Identifier: LGPL-2.1-or-later

use crate::db::Db;
use crate::error::{Error, ErrorKind, Result};
use crate::sys;
use crate::uri::Uri;
use rusqlite::params;
use std::collections::HashSet;
use std::os::unix::ffi::OsStrExt;
use std::os::unix::fs::{DirBuilderExt, PermissionsExt};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex, MutexGuard, PoisonError};
use std::time::{SystemTime, UNIX_EPOCH};

pub const DEFAULT_RETENTION_DAYS: u32 = 30;
const PARTIAL_PREFIX: &str = ".lautta-partial-";
const SECS_PER_DAY: i64 = 86_400;
const NAME_ATTEMPTS: usize = 8;
const ITEM_COLUMNS: &str = "id, original_uri, trashed_at, stored_name, is_dir, size";

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TrashItem {
    pub id: i64,
    pub original_uri: Uri,
    pub trashed_at: i64,
    pub stored_name: String,
    pub is_dir: bool,
    pub size: Option<u64>,
    pub dir: PathBuf,
}

impl TrashItem {
    pub fn stored_path(&self) -> PathBuf {
        self.dir.join(&self.stored_name)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Moved {
    Renamed,
    Copied,
}

type RenameFn = fn(&Path, &Path) -> Result<()>;

#[derive(Clone)]
pub struct Trash {
    dir: PathBuf,
    db: Db,
    turn: Arc<Mutex<()>>,
    rename: RenameFn,
}

pub fn now_secs() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_or(0, |d| i64::try_from(d.as_secs()).unwrap_or(i64::MAX))
}

fn unique_name(prefix: &str) -> String {
    static COUNTER: AtomicU64 = AtomicU64::new(0);
    let ms = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_or(0, |d| d.as_millis());
    let n = COUNTER.fetch_add(1, Ordering::Relaxed);
    format!("{prefix}{ms:013x}-{:x}-{n:x}", std::process::id())
}

fn is_real_dir(path: &Path) -> bool {
    sys::stat(path, false).is_ok_and(|st| st.is_dir())
}

fn exists(path: &Path) -> bool {
    sys::stat(path, false).is_ok()
}

impl Trash {
    pub fn new(dir: PathBuf, db: Db) -> Trash {
        Trash {
            dir,
            db,
            turn: Arc::default(),
            rename: move_without_replace,
        }
    }

    pub fn dir(&self) -> &Path {
        &self.dir
    }

    fn turn(&self) -> MutexGuard<'_, ()> {
        self.turn.lock().unwrap_or_else(PoisonError::into_inner)
    }

    fn guard(&self, path: &Path) -> Result<()> {
        if path.starts_with(&self.dir) || self.dir.starts_with(path) {
            return Err(Error::new(
                ErrorKind::InvalidArgument,
                "the trash cannot be moved into itself",
            ));
        }
        Ok(())
    }

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
            dir: self.dir.clone(),
        };
        let id = match self.insert_row(&item) {
            Ok(id) => id,
            Err(e) => {
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

    fn delete_row(&self, id: i64) -> Result<()> {
        self.db
            .lock()
            .execute("DELETE FROM trash_items WHERE id = ?1", params![id])?;
        Ok(())
    }

    pub fn restore(&self, id: i64, resolve: &dyn Fn(&Uri) -> Option<PathBuf>) -> Result<PathBuf> {
        let item = self.get(id)?;
        let dest = resolve(&item.original_uri)
            .ok_or_else(|| Error::new(ErrorKind::NotFound, "the original location is not available"))?;
        self.restore_to(id, &dest)?;
        Ok(dest)
    }

    pub fn restore_to(&self, id: i64, dest: &Path) -> Result<()> {
        let _turn = self.turn();
        let item = self.get(id)?;
        let stored = item.stored_path();
        if !exists(&stored) {
            self.delete_row(id)?;
            return Err(Error::new(ErrorKind::NotFound, "the trashed item is gone"));
        }
        if let Some(parent) = dest.parent() {
            std::fs::create_dir_all(parent)?;
        }
        let moved = self.put(&stored, dest)?;
        self.delete_row(id)?;
        if moved == Moved::Copied {
            if let Err(e) = remove_tree(&stored) {
                log::warn!("a restored trash item stays in the trash folder: {e}");
            }
        }
        Ok(())
    }

    pub fn delete(&self, id: i64) -> Result<()> {
        let _turn = self.turn();
        self.delete_unlocked(id)
    }

    fn delete_unlocked(&self, id: i64) -> Result<()> {
        let item = self.get(id)?;
        remove_tree(&item.stored_path())?;
        self.delete_row(id)
    }

    pub fn empty(&self) -> Result<usize> {
        let _turn = self.turn();
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
        first_error.map_or(Ok(removed), Err)
    }

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

    pub fn purge_older_than(&self, days: u32) -> Result<usize> {
        self.purge_before(now_secs() - i64::from(days) * SECS_PER_DAY)
    }

    pub fn purge_expired(&self) -> Result<usize> {
        self.purge_older_than(DEFAULT_RETENTION_DAYS)
    }

    pub fn total_size(&self) -> Result<u64> {
        Ok(self.list()?.iter().filter_map(|i| i.size).sum())
    }

    pub fn tidy(&self) -> Result<()> {
        let _turn = self.turn();
        remove_partials(&self.dir)
    }
}

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
            dir: trash.dir.clone(),
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
    })
}

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
    std::fs::set_permissions(to, std::fs::Permissions::from_mode(st.permissions()))?;
    sys::set_mtime(to, st.mtime)?;
    Ok(())
}

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

pub fn remove_tree(path: &Path) -> Result<()> {
    let st = match sys::stat(path, false) {
        Ok(st) => st,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(()),
        Err(e) => return Err(e.into()),
    };
    if st.is_dir() {
        std::fs::remove_dir_all(path)?;
    } else {
        std::fs::remove_file(path)?;
    }
    Ok(())
}

#[cfg(test)]
mod tests;
