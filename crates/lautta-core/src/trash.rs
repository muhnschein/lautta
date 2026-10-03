// SPDX-License-Identifier: LGPL-2.1-or-later
//! *Recently deleted* (SPEC OPS-8): the app-private trash for local files.
//!
//! Items move into one flat folder (`AppPaths::trash_dir`) by `rename`, so
//! trashing is instant and only possible on the same filesystem; across
//! devices the rename fails with `CrossesDevice` and the caller deletes
//! permanently. Each item gets a unique stored name and a `trash_items` row
//! (DAT-2) that remembers where it came from. All calls block: run them in
//! `spawn_blocking` (ARC-6).

use crate::db::Db;
use crate::error::{Error, ErrorKind, Result};
use crate::sys;
use crate::uri::Uri;
use rusqlite::params;
use std::os::unix::fs::{DirBuilderExt, PermissionsExt};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::{SystemTime, UNIX_EPOCH};

/// Items are purged after this many days (OPS-8).
pub const DEFAULT_RETENTION_DAYS: u32 = 30;
const SECS_PER_DAY: i64 = 86_400;
const NAME_ATTEMPTS: usize = 8;

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
}

#[derive(Clone)]
pub struct Trash {
    dir: PathBuf,
    db: Db,
}

fn now_secs() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_or(0, |d| i64::try_from(d.as_secs()).unwrap_or(i64::MAX))
}

fn new_stored_name() -> String {
    static COUNTER: AtomicU64 = AtomicU64::new(0);
    let ms = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_or(0, |d| d.as_millis());
    let n = COUNTER.fetch_add(1, Ordering::Relaxed);
    format!("{ms:013x}-{:x}-{n:x}", std::process::id())
}

impl Trash {
    pub fn new(dir: PathBuf, db: Db) -> Trash {
        Trash { dir, db }
    }

    pub fn dir(&self) -> &Path {
        &self.dir
    }

    /// Creates the folder with mode 0700 (SEC-5).
    fn ensure_dir(&self) -> Result<()> {
        std::fs::DirBuilder::new()
            .recursive(true)
            .mode(0o700)
            .create(&self.dir)?;
        let mode = std::fs::metadata(&self.dir)?.permissions().mode() & 0o777;
        if mode != 0o700 {
            std::fs::set_permissions(&self.dir, std::fs::Permissions::from_mode(0o700))?;
        }
        Ok(())
    }

    /// True when `path` can be moved into the trash by rename (same device).
    /// The trash folder is created if needed.
    pub fn can_trash(&self, path: &Path) -> Result<bool> {
        self.ensure_dir()?;
        let parent = path.parent().unwrap_or(path);
        Ok(sys::same_device(parent, &self.dir)?)
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

    /// Moves `path` (a file, folder or symlink; never followed) into the
    /// trash and records where it came from. `CrossesDevice` when the
    /// trash is on another filesystem.
    pub fn trash(&self, path: &Path, original_uri: &Uri) -> Result<TrashItem> {
        let st = sys::stat(path, false)?;
        self.guard(path)?;
        self.ensure_dir()?;
        let stored_name = self.move_in(path)?;
        let size = tree_size(&self.dir.join(&stored_name));
        let trashed_at = now_secs();
        match self.insert_row(original_uri, trashed_at, &stored_name, st.is_dir(), size) {
            Ok(id) => Ok(TrashItem {
                id,
                original_uri: original_uri.clone(),
                trashed_at,
                stored_name,
                is_dir: st.is_dir(),
                size,
            }),
            Err(e) => {
                // Without its row the item would be orphaned: put it back.
                let _ = std::fs::rename(self.dir.join(&stored_name), path);
                Err(e)
            }
        }
    }

    fn move_in(&self, path: &Path) -> Result<String> {
        for _ in 0..NAME_ATTEMPTS {
            let name = new_stored_name();
            match sys::rename_noreplace(path, &self.dir.join(&name)) {
                Ok(()) => return Ok(name),
                Err(e) if e.kind() == std::io::ErrorKind::AlreadyExists => {}
                Err(e) => return Err(e.into()),
            }
        }
        Err(Error::new(ErrorKind::Internal, "no unique trash name"))
    }

    fn insert_row(
        &self,
        uri: &Uri,
        trashed_at: i64,
        stored_name: &str,
        is_dir: bool,
        size: Option<u64>,
    ) -> Result<i64> {
        let conn = self.db.lock();
        conn.execute(
            "INSERT INTO trash_items(original_uri, trashed_at, stored_name, is_dir, size) \
             VALUES (?1, ?2, ?3, ?4, ?5)",
            params![
                uri.to_string(),
                trashed_at,
                stored_name,
                is_dir,
                size.and_then(|s| i64::try_from(s).ok())
            ],
        )?;
        Ok(conn.last_insert_rowid())
    }

    /// All items, newest first. Rows whose URI cannot be parsed are skipped.
    pub fn list(&self) -> Result<Vec<TrashItem>> {
        let conn = self.db.lock();
        let mut stmt = conn.prepare(
            "SELECT id, original_uri, trashed_at, stored_name, is_dir, size FROM trash_items \
             ORDER BY trashed_at DESC, id DESC",
        )?;
        let rows = stmt.query_map([], row_to_raw)?;
        let mut out = Vec::new();
        for raw in rows {
            match raw?.into_item() {
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
                "SELECT id, original_uri, trashed_at, stored_name, is_dir, size FROM trash_items WHERE id = ?1",
                params![id],
                row_to_raw,
            )
            .map_err(|e| match e {
                rusqlite::Error::QueryReturnedNoRows => Error::new(ErrorKind::NotFound, "no such trash item"),
                other => other.into(),
            })?;
        raw.into_item()
    }

    fn stored_path(&self, item: &TrashItem) -> PathBuf {
        self.dir.join(&item.stored_name)
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
    pub fn restore_to(&self, id: i64, dest: &Path) -> Result<()> {
        let item = self.get(id)?;
        let stored = self.stored_path(&item);
        if sys::stat(&stored, false).is_err() {
            // The file vanished behind our back; forget the stale row.
            self.delete_row(id)?;
            return Err(Error::new(ErrorKind::NotFound, "the trashed item is gone"));
        }
        if let Some(parent) = dest.parent() {
            std::fs::create_dir_all(parent)?;
        }
        move_without_replace(&stored, dest)?;
        self.delete_row(id)
    }

    /// Deletes one item for good.
    pub fn delete(&self, id: i64) -> Result<()> {
        let item = self.get(id)?;
        remove_tree(&self.stored_path(&item))?;
        self.delete_row(id)
    }

    /// Deletes everything, including stray files in the folder that have no
    /// row (left by a crash). Returns the number of items removed.
    pub fn empty(&self) -> Result<usize> {
        let items = self.list()?;
        let mut first_error = None;
        let mut removed = 0;
        for item in &items {
            match self.delete(item.id) {
                Ok(()) => removed += 1,
                Err(e) => {
                    first_error.get_or_insert(e);
                }
            }
        }
        self.remove_orphans()?;
        first_error.map_or(Ok(removed), Err)
    }

    fn remove_orphans(&self) -> Result<()> {
        let known: std::collections::HashSet<String> =
            self.list()?.into_iter().map(|i| i.stored_name).collect();
        let Ok(rd) = std::fs::read_dir(&self.dir) else {
            return Ok(());
        };
        for entry in rd {
            let entry = entry?;
            if !known.contains(&entry.file_name().to_string_lossy().into_owned()) {
                remove_tree(&entry.path())?;
            }
        }
        Ok(())
    }

    /// Deletes items trashed before `cutoff` (unix seconds).
    pub fn purge_before(&self, cutoff: i64) -> Result<usize> {
        let old: Vec<i64> = self
            .list()?
            .into_iter()
            .filter(|i| i.trashed_at < cutoff)
            .map(|i| i.id)
            .collect();
        let mut removed = 0;
        for id in old {
            self.delete(id)?;
            removed += 1;
        }
        Ok(removed)
    }

    /// Deletes items older than `days` days.
    pub fn purge_older_than(&self, days: u32) -> Result<usize> {
        self.purge_before(now_secs() - i64::from(days) * SECS_PER_DAY)
    }

    /// The 30-day purge run at app start (OPS-8).
    pub fn purge_expired(&self) -> Result<usize> {
        self.purge_older_than(DEFAULT_RETENTION_DAYS)
    }

    /// Total bytes of everything in the trash (from the recorded sizes).
    pub fn total_size(&self) -> Result<u64> {
        Ok(self.list()?.iter().filter_map(|i| i.size).sum())
    }
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
    fn into_item(self) -> Result<TrashItem> {
        Ok(TrashItem {
            id: self.id,
            original_uri: Uri::parse(&self.uri)?,
            trashed_at: self.trashed_at,
            stored_name: self.stored_name,
            is_dir: self.is_dir,
            size: self.size.and_then(|s| u64::try_from(s).ok()),
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

/// Rename that never replaces; on filesystems without `RENAME_NOREPLACE`
/// it falls back to an exists-check.
fn move_without_replace(from: &Path, to: &Path) -> Result<()> {
    match sys::rename_noreplace(from, to) {
        Ok(()) => Ok(()),
        Err(e) if sys::is_flag_unsupported(&e) => {
            if sys::stat(to, false).is_ok() {
                return Err(Error::kind(ErrorKind::AlreadyExists));
            }
            std::fs::rename(from, to).map_err(Into::into)
        }
        Err(e) => Err(e.into()),
    }
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
mod tests {
    use super::*;
    use std::ffi::OsStr;
    use std::os::unix::ffi::OsStrExt;
    use std::os::unix::fs::symlink;

    struct Env {
        _root: tempfile::TempDir,
        home: PathBuf,
        trash: Trash,
    }

    fn env() -> Env {
        let root = tempfile::tempdir().unwrap();
        let home = root.path().join("home");
        std::fs::create_dir(&home).unwrap();
        let trash = Trash::new(root.path().join("data/trash"), Db::open_in_memory().unwrap());
        Env {
            _root: root,
            home,
            trash,
        }
    }

    fn uri(path: &str) -> Uri {
        Uri::parse(&format!("lautta://user-documents/{path}")).unwrap()
    }

    #[test]
    fn trashing_a_file_moves_it_and_records_the_origin() {
        let e = env();
        let f = e.home.join("report.txt");
        std::fs::write(&f, "twelve bytes").unwrap();
        let item = e.trash.trash(&f, &uri("report.txt")).unwrap();
        assert!(!f.exists());
        assert_eq!(
            std::fs::read(e.trash.dir().join(&item.stored_name)).unwrap(),
            b"twelve bytes"
        );
        assert_eq!(item.original_uri, uri("report.txt"));
        assert_eq!(item.size, Some(12));
        assert!(!item.is_dir);
        assert!((now_secs() - item.trashed_at).abs() < 60);
        assert_eq!(e.trash.list().unwrap(), vec![item.clone()]);
        assert_eq!(e.trash.get(item.id).unwrap(), item);
    }

    #[test]
    fn trash_folder_is_private() {
        let e = env();
        let f = e.home.join("a");
        std::fs::write(&f, "x").unwrap();
        e.trash.trash(&f, &uri("a")).unwrap();
        let mode = std::fs::metadata(e.trash.dir()).unwrap().permissions().mode() & 0o777;
        assert_eq!(mode, 0o700);
        std::fs::set_permissions(e.trash.dir(), std::fs::Permissions::from_mode(0o755)).unwrap();
        let g = e.home.join("b");
        std::fs::write(&g, "x").unwrap();
        e.trash.trash(&g, &uri("b")).unwrap();
        let mode = std::fs::metadata(e.trash.dir()).unwrap().permissions().mode() & 0o777;
        assert_eq!(mode, 0o700, "loose permissions are tightened");
    }

    #[test]
    fn same_names_get_distinct_stored_names() {
        let e = env();
        let mut names = Vec::new();
        for i in 0..5 {
            let f = e.home.join("same.txt");
            std::fs::write(&f, format!("v{i}")).unwrap();
            names.push(e.trash.trash(&f, &uri("same.txt")).unwrap().stored_name);
        }
        names.sort();
        names.dedup();
        assert_eq!(names.len(), 5);
        assert_eq!(e.trash.list().unwrap().len(), 5);
    }

    #[test]
    fn folders_keep_their_content_and_size() {
        let e = env();
        let d = e.home.join("proj");
        std::fs::create_dir_all(d.join("a/b")).unwrap();
        std::fs::write(d.join("one"), "123").unwrap();
        std::fs::write(d.join("a/b/two"), "45678").unwrap();
        let item = e.trash.trash(&d, &uri("proj")).unwrap();
        assert!(item.is_dir);
        assert_eq!(item.size, Some(8));
        assert_eq!(
            std::fs::read(e.trash.dir().join(&item.stored_name).join("a/b/two")).unwrap(),
            b"45678"
        );
        assert!(!d.exists());
    }

    #[test]
    fn symlinks_are_trashed_as_links() {
        let e = env();
        let target = e.home.join("target-dir");
        std::fs::create_dir(&target).unwrap();
        std::fs::write(target.join("keep"), "x").unwrap();
        let link = e.home.join("link");
        symlink(&target, &link).unwrap();
        let item = e.trash.trash(&link, &uri("link")).unwrap();
        assert!(!item.is_dir, "a link to a folder is not a folder");
        assert!(target.join("keep").exists());
        assert!(!link.exists());
        let stored = e.trash.dir().join(&item.stored_name);
        assert!(std::fs::symlink_metadata(stored)
            .unwrap()
            .file_type()
            .is_symlink());
    }

    #[test]
    fn non_utf8_names_are_fine() {
        let e = env();
        let f = e.home.join(OsStr::from_bytes(b"caf\xe9\xff"));
        std::fs::write(&f, "x").unwrap();
        let u = Uri::new(
            "user-documents",
            crate::vpath::VPath::parse(b"caf\xe9\xff").unwrap(),
        );
        let item = e.trash.trash(&f, &u).unwrap();
        assert_eq!(item.original_uri, u);
        let back = e.home.join(OsStr::from_bytes(b"caf\xe9\xff"));
        let resolve = |u: &Uri| Some(e.home.join(OsStr::from_bytes(u.path.as_bytes())));
        assert_eq!(e.trash.restore(item.id, &resolve).unwrap(), back);
        assert!(back.exists());
    }

    #[test]
    fn missing_source_is_not_found_and_leaves_no_row() {
        let e = env();
        let err = e.trash.trash(&e.home.join("ghost"), &uri("ghost")).unwrap_err();
        assert_eq!(err.kind, ErrorKind::NotFound);
        assert!(e.trash.list().unwrap().is_empty());
    }

    #[test]
    fn the_trash_cannot_swallow_itself_or_its_parents() {
        let e = env();
        e.trash.ensure_dir().unwrap();
        let inside = e.trash.dir().join("x");
        std::fs::write(&inside, "x").unwrap();
        for p in [
            e.trash.dir().to_path_buf(),
            inside,
            e.trash.dir().parent().unwrap().to_path_buf(),
        ] {
            let err = e.trash.trash(&p, &uri("x")).unwrap_err();
            assert_eq!(err.kind, ErrorKind::InvalidArgument, "{p:?}");
            assert!(p.exists());
        }
        assert!(e.trash.list().unwrap().is_empty());
    }

    #[test]
    fn other_devices_are_refused_with_crosses_device() {
        // /dev/shm is a different filesystem from the temp dir on most hosts.
        let shm = Path::new("/dev/shm");
        let e = env();
        let Ok(dir) = tempfile::tempdir_in(shm) else {
            return;
        };
        if sys::same_device(dir.path(), e.trash.dir().parent().unwrap()).unwrap_or(true) {
            return;
        }
        let f = dir.path().join("f");
        std::fs::write(&f, "x").unwrap();
        assert!(!e.trash.can_trash(&f).unwrap());
        let err = e.trash.trash(&f, &uri("f")).unwrap_err();
        assert_eq!(err.kind, ErrorKind::CrossesDevice);
        assert!(f.exists(), "a refused item stays where it was");
        assert!(e.trash.list().unwrap().is_empty());
        assert_eq!(std::fs::read_dir(e.trash.dir()).unwrap().count(), 0);
    }

    #[test]
    fn can_trash_on_the_same_filesystem() {
        let e = env();
        let f = e.home.join("f");
        std::fs::write(&f, "x").unwrap();
        assert!(e.trash.can_trash(&f).unwrap());
    }

    #[test]
    fn list_is_newest_first() {
        let e = env();
        let mut ids = Vec::new();
        for (i, name) in ["a", "b", "c"].iter().enumerate() {
            let f = e.home.join(name);
            std::fs::write(&f, "x").unwrap();
            let item = e.trash.trash(&f, &uri(name)).unwrap();
            e.trash
                .db
                .lock()
                .execute(
                    "UPDATE trash_items SET trashed_at = ?1 WHERE id = ?2",
                    params![1000 + i as i64, item.id],
                )
                .unwrap();
            ids.push(item.id);
        }
        let listed: Vec<i64> = e.trash.list().unwrap().iter().map(|i| i.id).collect();
        assert_eq!(listed, [ids[2], ids[1], ids[0]]);
    }

    #[test]
    fn restore_puts_the_item_back() {
        let e = env();
        let f = e.home.join("doc.txt");
        std::fs::write(&f, "precious").unwrap();
        let item = e.trash.trash(&f, &uri("doc.txt")).unwrap();
        let home = e.home.clone();
        let resolve = move |u: &Uri| Some(home.join(OsStr::from_bytes(u.path.as_bytes())));
        let dest = e.trash.restore(item.id, &resolve).unwrap();
        assert_eq!(dest, f);
        assert_eq!(std::fs::read(&f).unwrap(), b"precious");
        assert!(e.trash.list().unwrap().is_empty());
        assert_eq!(std::fs::read_dir(e.trash.dir()).unwrap().count(), 0);
    }

    #[test]
    fn restore_never_overwrites() {
        let e = env();
        let f = e.home.join("doc.txt");
        std::fs::write(&f, "old").unwrap();
        let item = e.trash.trash(&f, &uri("doc.txt")).unwrap();
        std::fs::write(&f, "new occupant").unwrap();
        let err = e.trash.restore_to(item.id, &f).unwrap_err();
        assert_eq!(err.kind, ErrorKind::AlreadyExists);
        assert_eq!(std::fs::read(&f).unwrap(), b"new occupant");
        assert_eq!(e.trash.list().unwrap().len(), 1, "still in the trash");
        let other = e.home.join("doc 2.txt");
        e.trash.restore_to(item.id, &other).unwrap();
        assert_eq!(std::fs::read(&other).unwrap(), b"old");
    }

    #[test]
    fn restore_recreates_missing_folders_and_reports_missing_locations() {
        let e = env();
        let f = e.home.join("gone-dir/f");
        std::fs::create_dir(e.home.join("gone-dir")).unwrap();
        std::fs::write(&f, "x").unwrap();
        let item = e.trash.trash(&f, &uri("gone-dir/f")).unwrap();
        std::fs::remove_dir(e.home.join("gone-dir")).unwrap();
        let none = e.trash.restore(item.id, &|_| None).unwrap_err();
        assert_eq!(none.kind, ErrorKind::NotFound);
        assert_eq!(e.trash.list().unwrap().len(), 1);
        e.trash.restore_to(item.id, &f).unwrap();
        assert!(f.exists());
    }

    #[test]
    fn restore_of_a_vanished_item_drops_the_stale_row() {
        let e = env();
        let f = e.home.join("f");
        std::fs::write(&f, "x").unwrap();
        let item = e.trash.trash(&f, &uri("f")).unwrap();
        std::fs::remove_file(e.trash.dir().join(&item.stored_name)).unwrap();
        let err = e.trash.restore_to(item.id, &f).unwrap_err();
        assert_eq!(err.kind, ErrorKind::NotFound);
        assert!(e.trash.list().unwrap().is_empty());
        assert_eq!(e.trash.get(item.id).unwrap_err().kind, ErrorKind::NotFound);
    }

    #[test]
    fn delete_removes_content_and_row() {
        let e = env();
        let d = e.home.join("d");
        std::fs::create_dir_all(d.join("sub")).unwrap();
        std::fs::write(d.join("sub/f"), "x").unwrap();
        let item = e.trash.trash(&d, &uri("d")).unwrap();
        e.trash.delete(item.id).unwrap();
        assert_eq!(std::fs::read_dir(e.trash.dir()).unwrap().count(), 0);
        assert!(e.trash.list().unwrap().is_empty());
        assert_eq!(e.trash.delete(item.id).unwrap_err().kind, ErrorKind::NotFound);
    }

    #[test]
    fn deleting_never_follows_symlinks_out_of_the_trash() {
        let e = env();
        let outside = e.home.join("precious");
        std::fs::create_dir(&outside).unwrap();
        std::fs::write(outside.join("file"), "keep me").unwrap();
        let d = e.home.join("d");
        std::fs::create_dir(&d).unwrap();
        symlink(&outside, d.join("escape")).unwrap();
        symlink(outside.join("file"), d.join("filelink")).unwrap();
        let item = e.trash.trash(&d, &uri("d")).unwrap();
        e.trash.delete(item.id).unwrap();
        assert_eq!(std::fs::read(outside.join("file")).unwrap(), b"keep me");
        assert!(!e.trash.dir().join(&item.stored_name).exists());
    }

    #[test]
    fn remove_tree_handles_all_kinds() {
        let d = tempfile::tempdir().unwrap();
        let f = d.path().join("f");
        std::fs::write(&f, "x").unwrap();
        remove_tree(&f).unwrap();
        remove_tree(&f).unwrap();
        let l = d.path().join("l");
        symlink("/nonexistent", &l).unwrap();
        remove_tree(&l).unwrap();
        assert!(std::fs::symlink_metadata(&l).is_err());
        std::fs::create_dir_all(d.path().join("t/u")).unwrap();
        remove_tree(&d.path().join("t")).unwrap();
        assert!(!d.path().join("t").exists());
    }

    #[test]
    fn empty_removes_everything_including_strays() {
        let e = env();
        for n in ["a", "b"] {
            let f = e.home.join(n);
            std::fs::write(&f, "x").unwrap();
            e.trash.trash(&f, &uri(n)).unwrap();
        }
        std::fs::write(e.trash.dir().join("stray-from-a-crash"), "x").unwrap();
        std::fs::create_dir(e.trash.dir().join("stray-dir")).unwrap();
        assert_eq!(e.trash.empty().unwrap(), 2);
        assert!(e.trash.list().unwrap().is_empty());
        assert_eq!(std::fs::read_dir(e.trash.dir()).unwrap().count(), 0);
        assert_eq!(e.trash.empty().unwrap(), 0);
        assert!(e.trash.dir().is_dir());
    }

    fn age(e: &Env, id: i64, days: i64) {
        e.trash
            .db
            .lock()
            .execute(
                "UPDATE trash_items SET trashed_at = ?1 WHERE id = ?2",
                params![now_secs() - days * SECS_PER_DAY, id],
            )
            .unwrap();
    }

    #[test]
    fn purge_removes_only_expired_items() {
        let e = env();
        let mut ids = Vec::new();
        for n in ["old", "borderline", "fresh"] {
            let f = e.home.join(n);
            std::fs::write(&f, "x").unwrap();
            ids.push(e.trash.trash(&f, &uri(n)).unwrap());
        }
        age(&e, ids[0].id, 31);
        age(&e, ids[1].id, 29);
        assert_eq!(e.trash.purge_expired().unwrap(), 1);
        let left: Vec<String> = e
            .trash
            .list()
            .unwrap()
            .iter()
            .map(|i| i.original_uri.to_string())
            .collect();
        assert_eq!(left.len(), 2);
        assert!(!left.iter().any(|u| u.ends_with("/old")));
        assert!(!e.trash.dir().join(&ids[0].stored_name).exists());
        assert!(e.trash.dir().join(&ids[1].stored_name).exists());
        assert_eq!(e.trash.purge_older_than(7).unwrap(), 1);
        assert_eq!(
            e.trash.purge_older_than(0).unwrap(),
            0,
            "items from this second are not older"
        );
        assert_eq!(e.trash.purge_before(now_secs() + 10).unwrap(), 1);
    }

    #[test]
    fn total_size_adds_up() {
        let e = env();
        assert_eq!(e.trash.total_size().unwrap(), 0);
        let f = e.home.join("f");
        std::fs::write(&f, "1234").unwrap();
        e.trash.trash(&f, &uri("f")).unwrap();
        let d = e.home.join("d");
        std::fs::create_dir(&d).unwrap();
        std::fs::write(d.join("x"), "123456").unwrap();
        e.trash.trash(&d, &uri("d")).unwrap();
        assert_eq!(e.trash.total_size().unwrap(), 10);
    }

    #[test]
    fn unreadable_rows_are_skipped_in_the_listing() {
        let e = env();
        let f = e.home.join("f");
        std::fs::write(&f, "x").unwrap();
        e.trash.trash(&f, &uri("f")).unwrap();
        e.trash
            .db
            .lock()
            .execute(
                "INSERT INTO trash_items(original_uri, trashed_at, stored_name, is_dir) VALUES ('not a uri', 1, 'zz', 0)",
                [],
            )
            .unwrap();
        assert_eq!(e.trash.list().unwrap().len(), 1);
    }

    #[test]
    fn row_failure_puts_the_item_back() {
        let e = env();
        e.trash.db.lock().execute("DROP TABLE trash_items", []).unwrap();
        let f = e.home.join("f");
        std::fs::write(&f, "x").unwrap();
        assert!(e.trash.trash(&f, &uri("f")).is_err());
        assert_eq!(std::fs::read(&f).unwrap(), b"x");
        assert_eq!(std::fs::read_dir(e.trash.dir()).unwrap().count(), 0);
    }
}
