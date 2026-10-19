// SPDX-License-Identifier: LGPL-2.1-or-later
//! *Recently deleted* (SPEC OPS-8): the trash for local files.
//!
//! Every location that keeps a trash has its own hidden bin at its root,
//! `<root>/.lautta-trash` ([`BIN_NAME`], [`bin_of`]), created on the first
//! delete there. Items move in by `rename`, so trashing is instant. The bin
//! has to live on the location's own mount: Sailjail mounts a tmpfs on
//! `$HOME` and bind-mounts every permitted folder on its own (§3.3), and
//! `rename(2)` between two mounts fails with `EXDEV` even on one file
//! system, so one trash folder for all locations could never take an item.
//!
//! Each item gets a unique stored name and a `trash_items` row (DAT-2) that
//! remembers where it came from and which bin holds it. Rows from before
//! schema 4 have no bin: those items stay in the app data folder
//! (`AppPaths::trash_dir`, DAT-1) until they are restored, deleted or
//! purged. All calls block: run them in `spawn_blocking` (ARC-6).

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
use std::time::{SystemTime, UNIX_EPOCH};

/// Items are purged after this many days (OPS-8).
pub const DEFAULT_RETENTION_DAYS: u32 = 30;
/// The bin at the root of a location that keeps a trash (OPS-8). Local
/// listings never show it, hidden files shown or not.
pub const BIN_NAME: &str = ".lautta-trash";
const SECS_PER_DAY: i64 = 86_400;
const NAME_ATTEMPTS: usize = 8;
const ITEM_COLUMNS: &str = "id, original_uri, trashed_at, stored_name, is_dir, size, bin";

/// The bin of the location whose root is `root`.
pub fn bin_of(root: &Path) -> PathBuf {
    root.join(BIN_NAME)
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
    /// The folder holding the item: its location's bin, or the app data
    /// folder for items from before schema 4.
    pub bin: PathBuf,
}

impl TrashItem {
    pub fn stored_path(&self) -> PathBuf {
        self.bin.join(&self.stored_name)
    }
}

#[derive(Clone)]
pub struct Trash {
    /// Where the items without a recorded bin are (DAT-1).
    legacy: PathBuf,
    db: Db,
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

/// Creates a bin with mode 0700 (SEC-5) in an existing folder; true when
/// this call made it. Anything but a real folder in its place (a symlink
/// planted there) is refused, so nothing is ever moved out of the location.
fn ensure_bin(bin: &Path) -> Result<bool> {
    let created = match std::fs::DirBuilder::new().mode(0o700).create(bin) {
        Ok(()) => true,
        Err(e) if e.kind() == std::io::ErrorKind::AlreadyExists => false,
        Err(e) => return Err(e.into()),
    };
    let st = sys::stat(bin, false)?;
    if !st.is_dir() {
        return Err(Error::new(
            ErrorKind::NotADirectory,
            "the Recently deleted folder is not a folder",
        ));
    }
    if st.permissions() & 0o777 != 0o700 {
        std::fs::set_permissions(bin, std::fs::Permissions::from_mode(0o700))?;
    }
    Ok(created)
}

/// True for a real folder, not a symlink to one.
fn is_real_dir(path: &Path) -> bool {
    sys::stat(path, false).is_ok_and(|st| st.is_dir())
}

/// Moves `path` into `bin` under a fresh unique name.
fn move_in(path: &Path, bin: &Path) -> Result<String> {
    for _ in 0..NAME_ATTEMPTS {
        let name = unique_name("");
        match sys::rename_noreplace(path, &bin.join(&name)) {
            Ok(()) => return Ok(name),
            Err(e) if e.kind() == std::io::ErrorKind::AlreadyExists => {}
            Err(e) => return Err(e.into()),
        }
    }
    Err(Error::new(ErrorKind::Internal, "no unique trash name"))
}

impl Trash {
    /// `legacy` is the app data trash folder (`AppPaths::trash_dir`).
    pub fn new(legacy: PathBuf, db: Db) -> Trash {
        Trash { legacy, db }
    }

    /// The app data trash folder of items from before schema 4.
    pub fn legacy_dir(&self) -> &Path {
        &self.legacy
    }

    /// Refuses to move a bin, or a folder that holds one, into a bin.
    fn guard(&self, path: &Path, bin: &Path) -> Result<()> {
        let overlaps = |dir: &Path| path.starts_with(dir) || dir.starts_with(path);
        if overlaps(bin) || overlaps(&self.legacy) {
            return Err(Error::new(
                ErrorKind::InvalidArgument,
                "the trash cannot be moved into itself",
            ));
        }
        Ok(())
    }

    /// Moves `path` (a file, folder or symlink; never followed) into `bin`,
    /// the bin of its location ([`bin_of`] its root), and records where it
    /// came from. `CrossesDevice` when `path` is on another mount than the
    /// bin (a folder mounted inside the location): the caller then deletes
    /// permanently.
    pub fn trash(&self, path: &Path, original_uri: &Uri, bin: &Path) -> Result<TrashItem> {
        let st = sys::stat(path, false)?;
        self.guard(path, bin)?;
        let created = ensure_bin(bin)?;
        let stored_name = match move_in(path, bin) {
            Ok(name) => name,
            Err(e) => {
                if created {
                    // Leave no empty bin behind in the user's folder.
                    let _ = std::fs::remove_dir(bin);
                }
                return Err(e);
            }
        };
        let item = TrashItem {
            id: 0,
            original_uri: original_uri.clone(),
            trashed_at: now_secs(),
            size: tree_size(&bin.join(&stored_name)),
            stored_name,
            is_dir: st.is_dir(),
            bin: bin.to_path_buf(),
        };
        match self.insert_row(&item) {
            Ok(id) => Ok(TrashItem { id, ..item }),
            Err(e) => {
                // Without its row the item would be orphaned: put it back.
                let _ = std::fs::rename(item.stored_path(), path);
                Err(e)
            }
        }
    }

    fn insert_row(&self, item: &TrashItem) -> Result<i64> {
        let conn = self.db.lock();
        conn.execute(
            "INSERT INTO trash_items(original_uri, trashed_at, stored_name, is_dir, size, bin) \
             VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
            params![
                item.original_uri.to_string(),
                item.trashed_at,
                item.stored_name,
                item.is_dir,
                item.size.and_then(|s| i64::try_from(s).ok()),
                item.bin.as_os_str().as_bytes()
            ],
        )?;
        Ok(conn.last_insert_rowid())
    }

    /// The folder of a row's `bin` column.
    fn bin_path(&self, bin: Option<Vec<u8>>) -> PathBuf {
        bin.map_or_else(|| self.legacy.clone(), |b| PathBuf::from(OsStr::from_bytes(&b)))
    }

    /// All items of every bin, newest first. Rows whose URI cannot be
    /// parsed are skipped.
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

    /// The bins that hold items, the app data folder too when an item from
    /// before schema 4 is left there.
    pub fn bins(&self) -> Result<Vec<PathBuf>> {
        let conn = self.db.lock();
        let mut stmt = conn.prepare("SELECT DISTINCT bin FROM trash_items")?;
        let rows = stmt.query_map([], |r| r.get::<_, Option<Vec<u8>>>(0))?;
        let mut bins = BTreeSet::new();
        for bin in rows {
            bins.insert(self.bin_path(bin?));
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
    /// From its location's bin an item goes back by rename (one mount); an
    /// item on another mount (the app data folder in the sandbox) is copied
    /// over and then removed.
    pub fn restore_to(&self, id: i64, dest: &Path) -> Result<()> {
        let item = self.get(id)?;
        let stored = item.stored_path();
        if sys::stat(&stored, false).is_err() {
            // The file vanished behind our back; forget the stale row.
            self.delete_row(id)?;
            return Err(Error::new(ErrorKind::NotFound, "the trashed item is gone"));
        }
        if let Some(parent) = dest.parent() {
            std::fs::create_dir_all(parent)?;
        }
        match move_without_replace(&stored, dest) {
            Err(e) if e.kind == ErrorKind::CrossesDevice => move_across(&stored, dest)?,
            other => other?,
        }
        self.delete_row(id)
    }

    /// Deletes one item for good.
    pub fn delete(&self, id: i64) -> Result<()> {
        let item = self.get(id)?;
        remove_tree(&item.stored_path())?;
        self.delete_row(id)
    }

    /// Deletes everything, and the strays without a row (left by a crash)
    /// in every bin: those of the items, the app data folder and `bins`
    /// (the bins of the locations that keep a trash now). Emptied bins in
    /// locations go too, leaving no trace in the user's folders. Returns
    /// the number of items removed.
    pub fn empty(&self, bins: &[PathBuf]) -> Result<usize> {
        let mut swept: BTreeSet<PathBuf> = self.bins()?.into_iter().collect();
        swept.extend(bins.iter().cloned());
        swept.insert(self.legacy.clone());
        let mut first_error = None;
        let mut removed = 0;
        for item in self.list()? {
            match self.delete(item.id) {
                Ok(()) => removed += 1,
                Err(e) => {
                    first_error.get_or_insert(e);
                }
            }
        }
        let known: HashSet<PathBuf> = self.list()?.iter().map(TrashItem::stored_path).collect();
        for bin in &swept {
            remove_strays(bin, &known)?;
            if *bin != self.legacy {
                // Fails while an item is left, which is what we want.
                let _ = std::fs::remove_dir(bin);
            }
        }
        first_error.map_or(Ok(removed), Err)
    }

    /// Deletes items trashed before `cutoff` (unix seconds), in every bin.
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

    /// The default 30-day purge (OPS-8).
    pub fn purge_expired(&self) -> Result<usize> {
        self.purge_older_than(DEFAULT_RETENTION_DAYS)
    }

    /// Total bytes of everything in the trash (from the recorded sizes).
    pub fn total_size(&self) -> Result<u64> {
        Ok(self.list()?.iter().filter_map(|i| i.size).sum())
    }
}

/// Removes what is in `bin` but not in `known`. A bin that is not a real
/// folder is left alone: never sweep through a symlink.
fn remove_strays(bin: &Path, known: &HashSet<PathBuf>) -> Result<()> {
    if !is_real_dir(bin) {
        return Ok(());
    }
    for entry in std::fs::read_dir(bin)? {
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

/// Moves between mounts without replacing: a copy under a temporary name
/// next to `to` is renamed into place, then `from` goes. A failed copy
/// leaves `from` as it was and nothing at `to`.
fn move_across(from: &Path, to: &Path) -> Result<()> {
    if sys::stat(to, false).is_ok() {
        return Err(Error::kind(ErrorKind::AlreadyExists));
    }
    let tmp = to.with_file_name(unique_name(".lautta-restore-"));
    if let Err(e) = copy_tree(from, &tmp).and_then(|()| move_without_replace(&tmp, to)) {
        let _ = remove_tree(&tmp);
        return Err(e);
    }
    remove_tree(from)
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
        for entry in std::fs::read_dir(from)? {
            let entry = entry?;
            copy_tree(&entry.path(), &to.join(entry.file_name()))?;
        }
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
    use std::os::unix::fs::symlink;

    struct Env {
        root: tempfile::TempDir,
        /// The root of a location ("Documents").
        loc: PathBuf,
        bin: PathBuf,
        trash: Trash,
    }

    fn env() -> Env {
        let root = tempfile::tempdir().unwrap();
        let loc = root.path().join("home/Documents");
        std::fs::create_dir_all(&loc).unwrap();
        let trash = Trash::new(root.path().join("data/trash"), Db::open_in_memory().unwrap());
        Env {
            bin: bin_of(&loc),
            root,
            loc,
            trash,
        }
    }

    impl Env {
        fn put(&self, name: &str, data: &str) -> PathBuf {
            let f = self.loc.join(name);
            std::fs::write(&f, data).unwrap();
            f
        }

        fn trash(&self, path: &Path, name: &str) -> Result<TrashItem> {
            self.trash.trash(path, &uri(name), &self.bin)
        }

        /// Another location ("Downloads") with its own bin.
        fn second(&self) -> (PathBuf, PathBuf) {
            let loc = self.root.path().join("home/Downloads");
            std::fs::create_dir_all(&loc).unwrap();
            let bin = bin_of(&loc);
            (loc, bin)
        }

        /// A row from before schema 4: no bin, the item in the app data folder.
        fn legacy_item(&self, stored: &str, name: &str, data: &str) -> i64 {
            std::fs::create_dir_all(self.trash.legacy_dir()).unwrap();
            std::fs::write(self.trash.legacy_dir().join(stored), data).unwrap();
            let conn = self.trash.db.lock();
            conn.execute(
                "INSERT INTO trash_items(original_uri, trashed_at, stored_name, is_dir, size) \
                 VALUES (?1, ?2, ?3, 0, ?4)",
                params![uri(name).to_string(), now_secs(), stored, data.len() as i64],
            )
            .unwrap();
            conn.last_insert_rowid()
        }

        fn resolve(&self) -> impl Fn(&Uri) -> Option<PathBuf> + '_ {
            |u: &Uri| Some(self.loc.join(OsStr::from_bytes(u.path.as_bytes())))
        }
    }

    fn uri(path: &str) -> Uri {
        Uri::parse(&format!("lautta://user-documents/{path}")).unwrap()
    }

    fn count(dir: &Path) -> usize {
        std::fs::read_dir(dir).unwrap().count()
    }

    /// A temp folder on another file system than `near`, if the host has one.
    fn other_device(near: &Path) -> Option<tempfile::TempDir> {
        let dir = tempfile::tempdir_in("/dev/shm").ok()?;
        (!sys::same_device(dir.path(), near).unwrap_or(true)).then_some(dir)
    }

    #[test]
    fn trashing_moves_the_item_into_its_locations_bin() {
        let e = env();
        let f = e.put("report.txt", "twelve bytes");
        assert!(!e.bin.exists(), "the bin is made on the first delete");
        let item = e.trash(&f, "report.txt").unwrap();
        assert!(!f.exists());
        assert_eq!(item.bin, e.loc.join(".lautta-trash"));
        assert_eq!(
            std::fs::read(e.bin.join(&item.stored_name)).unwrap(),
            b"twelve bytes"
        );
        assert_eq!(item.stored_path(), e.bin.join(&item.stored_name));
        assert_eq!(item.original_uri, uri("report.txt"));
        assert_eq!(item.size, Some(12));
        assert!(!item.is_dir);
        assert!((now_secs() - item.trashed_at).abs() < 60);
        assert_eq!(e.trash.list().unwrap(), vec![item.clone()]);
        assert_eq!(e.trash.get(item.id).unwrap(), item);
        assert!(!e.trash.legacy_dir().exists(), "the app data folder is not used");
    }

    #[test]
    fn bins_are_private() {
        let e = env();
        e.trash(&e.put("a", "x"), "a").unwrap();
        let mode = std::fs::metadata(&e.bin).unwrap().permissions().mode() & 0o777;
        assert_eq!(mode, 0o700);
        std::fs::set_permissions(&e.bin, std::fs::Permissions::from_mode(0o755)).unwrap();
        e.trash(&e.put("b", "x"), "b").unwrap();
        let mode = std::fs::metadata(&e.bin).unwrap().permissions().mode() & 0o777;
        assert_eq!(mode, 0o700, "loose permissions are tightened");
    }

    #[test]
    fn a_bin_that_is_not_a_folder_is_refused() {
        let e = env();
        let elsewhere = e.root.path().join("elsewhere");
        std::fs::create_dir(&elsewhere).unwrap();
        symlink(&elsewhere, &e.bin).unwrap();
        let f = e.put("a", "x");
        let err = e.trash(&f, "a").unwrap_err();
        assert_eq!(err.kind, ErrorKind::NotADirectory);
        assert!(f.exists());
        assert_eq!(count(&elsewhere), 0, "nothing leaves through a planted link");
        assert!(e.trash.list().unwrap().is_empty());
    }

    #[test]
    fn same_names_get_distinct_stored_names() {
        let e = env();
        let mut names = Vec::new();
        for i in 0..5 {
            let f = e.put("same.txt", &format!("v{i}"));
            names.push(e.trash(&f, "same.txt").unwrap().stored_name);
        }
        names.sort();
        names.dedup();
        assert_eq!(names.len(), 5);
        assert_eq!(e.trash.list().unwrap().len(), 5);
    }

    #[test]
    fn folders_keep_their_content_and_size() {
        let e = env();
        let d = e.loc.join("proj");
        std::fs::create_dir_all(d.join("a/b")).unwrap();
        std::fs::write(d.join("one"), "123").unwrap();
        std::fs::write(d.join("a/b/two"), "45678").unwrap();
        let item = e.trash(&d, "proj").unwrap();
        assert!(item.is_dir);
        assert_eq!(item.size, Some(8));
        assert_eq!(
            std::fs::read(item.stored_path().join("a/b/two")).unwrap(),
            b"45678"
        );
        assert!(!d.exists());
    }

    #[test]
    fn symlinks_are_trashed_as_links() {
        let e = env();
        let target = e.loc.join("target-dir");
        std::fs::create_dir(&target).unwrap();
        std::fs::write(target.join("keep"), "x").unwrap();
        let link = e.loc.join("link");
        symlink(&target, &link).unwrap();
        let item = e.trash(&link, "link").unwrap();
        assert!(!item.is_dir, "a link to a folder is not a folder");
        assert!(target.join("keep").exists());
        assert!(!link.exists());
        assert!(std::fs::symlink_metadata(item.stored_path())
            .unwrap()
            .file_type()
            .is_symlink());
    }

    #[test]
    fn non_utf8_names_and_bins_are_fine() {
        let e = env();
        let loc = e.root.path().join(OsStr::from_bytes(b"home/caf\xe9"));
        std::fs::create_dir_all(&loc).unwrap();
        let f = loc.join(OsStr::from_bytes(b"\xff\xfe"));
        std::fs::write(&f, "x").unwrap();
        let u = Uri::new("user-documents", crate::vpath::VPath::parse(b"\xff\xfe").unwrap());
        let item = e.trash.trash(&f, &u, &bin_of(&loc)).unwrap();
        let listed = e.trash.list().unwrap();
        assert_eq!(listed[0].bin, bin_of(&loc), "the bin survives the database");
        assert_eq!(listed[0].original_uri, u);
        let resolve = |u: &Uri| Some(loc.join(OsStr::from_bytes(u.path.as_bytes())));
        assert_eq!(e.trash.restore(item.id, &resolve).unwrap(), f);
        assert!(f.exists());
    }

    #[test]
    fn missing_source_is_not_found_and_leaves_no_row() {
        let e = env();
        let err = e.trash(&e.loc.join("ghost"), "ghost").unwrap_err();
        assert_eq!(err.kind, ErrorKind::NotFound);
        assert!(e.trash.list().unwrap().is_empty());
        assert!(!e.bin.exists());
    }

    #[test]
    fn the_trash_cannot_swallow_a_bin_or_what_holds_one() {
        let e = env();
        e.trash(&e.put("first", "x"), "first").unwrap();
        let inside = e.bin.join("x");
        std::fs::write(&inside, "x").unwrap();
        std::fs::create_dir_all(e.trash.legacy_dir()).unwrap();
        for p in [
            e.bin.clone(),
            inside,
            e.loc.clone(),
            e.trash.legacy_dir().to_path_buf(),
            e.trash.legacy_dir().parent().unwrap().to_path_buf(),
        ] {
            let err = e.trash(&p, "x").unwrap_err();
            assert_eq!(err.kind, ErrorKind::InvalidArgument, "{p:?}");
            assert!(p.exists());
        }
        assert_eq!(e.trash.list().unwrap().len(), 1);
    }

    #[test]
    fn other_mounts_are_refused_with_crosses_device() {
        let e = env();
        let Some(dir) = other_device(&e.loc) else {
            return;
        };
        let f = dir.path().join("f");
        std::fs::write(&f, "x").unwrap();
        let err = e.trash.trash(&f, &uri("f"), &e.bin).unwrap_err();
        assert_eq!(err.kind, ErrorKind::CrossesDevice);
        assert!(f.exists(), "a refused item stays where it was");
        assert!(e.trash.list().unwrap().is_empty());
        assert!(!e.bin.exists(), "no empty bin is left behind");
        std::fs::create_dir(&e.bin).unwrap();
        e.trash.trash(&f, &uri("f"), &e.bin).unwrap_err();
        assert!(e.bin.is_dir(), "a bin made before stays");
    }

    #[test]
    fn list_is_newest_first_across_bins() {
        let e = env();
        let (loc2, bin2) = e.second();
        let mut ids = Vec::new();
        for (i, name) in ["a", "b", "c"].iter().enumerate() {
            let item = if i == 1 {
                let f = loc2.join(name);
                std::fs::write(&f, "x").unwrap();
                e.trash.trash(&f, &uri(name), &bin2).unwrap()
            } else {
                e.trash(&e.put(name, "x"), name).unwrap()
            };
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
        assert_eq!(e.trash.bins().unwrap(), {
            let mut b = vec![e.bin.clone(), bin2];
            b.sort();
            b
        });
    }

    #[test]
    fn restore_puts_the_item_back() {
        let e = env();
        let f = e.put("doc.txt", "precious");
        let item = e.trash(&f, "doc.txt").unwrap();
        let dest = e.trash.restore(item.id, &e.resolve()).unwrap();
        assert_eq!(dest, f);
        assert_eq!(std::fs::read(&f).unwrap(), b"precious");
        assert!(e.trash.list().unwrap().is_empty());
        assert_eq!(count(&e.bin), 0);
    }

    #[test]
    fn restore_never_overwrites() {
        let e = env();
        let f = e.put("doc.txt", "old");
        let item = e.trash(&f, "doc.txt").unwrap();
        std::fs::write(&f, "new occupant").unwrap();
        let err = e.trash.restore_to(item.id, &f).unwrap_err();
        assert_eq!(err.kind, ErrorKind::AlreadyExists);
        assert_eq!(std::fs::read(&f).unwrap(), b"new occupant");
        assert_eq!(e.trash.list().unwrap().len(), 1, "still in the trash");
        let other = e.loc.join("doc 2.txt");
        e.trash.restore_to(item.id, &other).unwrap();
        assert_eq!(std::fs::read(&other).unwrap(), b"old");
    }

    #[test]
    fn restore_recreates_missing_folders_and_reports_missing_locations() {
        let e = env();
        std::fs::create_dir(e.loc.join("gone-dir")).unwrap();
        let f = e.put("gone-dir/f", "x");
        let item = e.trash(&f, "gone-dir/f").unwrap();
        std::fs::remove_dir(e.loc.join("gone-dir")).unwrap();
        let none = e.trash.restore(item.id, &|_| None).unwrap_err();
        assert_eq!(none.kind, ErrorKind::NotFound);
        assert_eq!(e.trash.list().unwrap().len(), 1);
        e.trash.restore_to(item.id, &f).unwrap();
        assert!(f.exists());
    }

    #[test]
    fn restore_of_a_vanished_item_drops_the_stale_row() {
        let e = env();
        let f = e.put("f", "x");
        let item = e.trash(&f, "f").unwrap();
        std::fs::remove_file(item.stored_path()).unwrap();
        let err = e.trash.restore_to(item.id, &f).unwrap_err();
        assert_eq!(err.kind, ErrorKind::NotFound);
        assert!(e.trash.list().unwrap().is_empty());
        assert_eq!(e.trash.get(item.id).unwrap_err().kind, ErrorKind::NotFound);
    }

    #[test]
    fn items_from_before_bins_stay_in_the_app_data_folder() {
        let e = env();
        let id = e.legacy_item("old-1", "old.txt", "legacy");
        let item = e.trash.get(id).unwrap();
        assert_eq!(item.bin, e.trash.legacy_dir());
        assert_eq!(e.trash.bins().unwrap(), [e.trash.legacy_dir().to_path_buf()]);
        e.trash.restore(id, &e.resolve()).unwrap();
        assert_eq!(std::fs::read(e.loc.join("old.txt")).unwrap(), b"legacy");
        let id = e.legacy_item("old-2", "again.txt", "x");
        e.trash.delete(id).unwrap();
        assert_eq!(count(e.trash.legacy_dir()), 0);
        assert!(e.trash.list().unwrap().is_empty());
    }

    #[test]
    fn restore_across_mounts_copies_the_tree_then_removes_it() {
        // Sailjail: the app data folder and ~/Documents are different
        // mounts, so a rename between them fails with EXDEV.
        let e = env();
        let Some(shm) = other_device(&e.loc) else {
            return;
        };
        let trash = Trash::new(shm.path().join("trash"), e.trash.db.clone());
        let stored = trash.legacy_dir().join("old-dir");
        std::fs::create_dir_all(stored.join("sub")).unwrap();
        std::fs::write(stored.join("sub/f"), "content").unwrap();
        std::fs::set_permissions(stored.join("sub/f"), std::fs::Permissions::from_mode(0o640)).unwrap();
        symlink("sub/f", stored.join("link")).unwrap();
        let mtime = UNIX_EPOCH + std::time::Duration::from_secs(1_000_000_000);
        sys::set_mtime(&stored.join("sub/f"), mtime).unwrap();
        trash
            .db
            .lock()
            .execute(
                "INSERT INTO trash_items(original_uri, trashed_at, stored_name, is_dir) \
                 VALUES (?1, 1, 'old-dir', 1)",
                params![uri("Project").to_string()],
            )
            .unwrap();
        let id = trash.list().unwrap()[0].id;
        let dest = trash.restore(id, &e.resolve()).unwrap();
        assert_eq!(dest, e.loc.join("Project"));
        assert_eq!(std::fs::read(dest.join("sub/f")).unwrap(), b"content");
        let meta = std::fs::metadata(dest.join("sub/f")).unwrap();
        assert_eq!(meta.permissions().mode() & 0o777, 0o640);
        assert_eq!(meta.modified().unwrap(), mtime);
        assert_eq!(std::fs::read_link(dest.join("link")).unwrap(), Path::new("sub/f"));
        assert!(!stored.exists(), "moved, not copied");
        assert!(trash.list().unwrap().is_empty());
        assert_eq!(count(&e.loc), 1, "no temporary copy is left");
    }

    #[test]
    fn restore_across_mounts_never_overwrites_and_keeps_the_item() {
        let e = env();
        let Some(shm) = other_device(&e.loc) else {
            return;
        };
        let trash = Trash::new(shm.path().join("trash"), e.trash.db.clone());
        std::fs::create_dir_all(trash.legacy_dir()).unwrap();
        std::fs::write(trash.legacy_dir().join("s"), "trashed").unwrap();
        trash
            .db
            .lock()
            .execute(
                "INSERT INTO trash_items(original_uri, trashed_at, stored_name) VALUES (?1, 1, 's')",
                params![uri("taken").to_string()],
            )
            .unwrap();
        let taken = e.put("taken", "occupant");
        let id = trash.list().unwrap()[0].id;
        let err = trash.restore(id, &e.resolve()).unwrap_err();
        assert_eq!(err.kind, ErrorKind::AlreadyExists);
        assert_eq!(std::fs::read(&taken).unwrap(), b"occupant");
        assert!(trash.legacy_dir().join("s").exists());
        assert_eq!(trash.list().unwrap().len(), 1);
        let fifo = trash.legacy_dir().join("s");
        std::fs::remove_file(&fifo).unwrap();
        rustix::fs::mknodat(
            rustix::fs::CWD,
            &fifo,
            rustix::fs::FileType::Fifo,
            rustix::fs::Mode::from_raw_mode(0o600),
            0,
        )
        .unwrap();
        let err = trash.restore_to(id, &e.loc.join("free")).unwrap_err();
        assert_eq!(err.kind, ErrorKind::Unsupported, "special files are not copied");
        assert_eq!(count(&e.loc), 1, "the failed copy is cleaned up");
        assert!(fifo.exists());
    }

    #[test]
    fn delete_removes_content_and_row() {
        let e = env();
        let d = e.loc.join("d");
        std::fs::create_dir_all(d.join("sub")).unwrap();
        std::fs::write(d.join("sub/f"), "x").unwrap();
        let item = e.trash(&d, "d").unwrap();
        e.trash.delete(item.id).unwrap();
        assert_eq!(count(&e.bin), 0);
        assert!(e.trash.list().unwrap().is_empty());
        assert_eq!(e.trash.delete(item.id).unwrap_err().kind, ErrorKind::NotFound);
    }

    #[test]
    fn deleting_never_follows_symlinks_out_of_the_trash() {
        let e = env();
        let outside = e.loc.join("precious");
        std::fs::create_dir(&outside).unwrap();
        std::fs::write(outside.join("file"), "keep me").unwrap();
        let d = e.loc.join("d");
        std::fs::create_dir(&d).unwrap();
        symlink(&outside, d.join("escape")).unwrap();
        symlink(outside.join("file"), d.join("filelink")).unwrap();
        let item = e.trash(&d, "d").unwrap();
        e.trash.delete(item.id).unwrap();
        assert_eq!(std::fs::read(outside.join("file")).unwrap(), b"keep me");
        assert!(!item.stored_path().exists());
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
    fn empty_clears_every_bin_including_strays() {
        let e = env();
        let (loc2, bin2) = e.second();
        e.trash(&e.put("a", "x"), "a").unwrap();
        let f = loc2.join("b");
        std::fs::write(&f, "x").unwrap();
        e.trash.trash(&f, &uri("b"), &bin2).unwrap();
        e.legacy_item("old", "c", "x");
        std::fs::write(e.bin.join("stray-from-a-crash"), "x").unwrap();
        std::fs::write(e.trash.legacy_dir().join("stray"), "x").unwrap();
        // A location's bin that no row points at.
        let loc3 = e.root.path().join("home/Music");
        let bin3 = bin_of(&loc3);
        std::fs::create_dir_all(bin3.join("stray-dir")).unwrap();
        assert_eq!(e.trash.empty(std::slice::from_ref(&bin3)).unwrap(), 3);
        assert!(e.trash.list().unwrap().is_empty());
        for bin in [&e.bin, &bin2, &bin3] {
            assert!(!bin.exists(), "{bin:?} is gone with its content");
        }
        assert_eq!(count(e.trash.legacy_dir()), 0);
        assert!(e.trash.legacy_dir().is_dir(), "the app data folder stays");
        assert!(loc3.is_dir() && e.loc.is_dir() && loc2.is_dir());
        assert_eq!(e.trash.empty(&[]).unwrap(), 0);
    }

    #[test]
    fn empty_never_sweeps_through_a_planted_link() {
        let e = env();
        let elsewhere = e.root.path().join("elsewhere");
        std::fs::create_dir(&elsewhere).unwrap();
        let mine = elsewhere.join("mine.txt");
        std::fs::write(&mine, "keep").unwrap();
        symlink(&elsewhere, &e.bin).unwrap();
        assert_eq!(e.trash.empty(std::slice::from_ref(&e.bin)).unwrap(), 0);
        assert_eq!(std::fs::read(&mine).unwrap(), b"keep");
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
    fn purge_removes_only_expired_items_from_every_bin() {
        let e = env();
        let (loc2, bin2) = e.second();
        let old = e.trash(&e.put("old", "x"), "old").unwrap();
        let f = loc2.join("old2");
        std::fs::write(&f, "x").unwrap();
        let old2 = e.trash.trash(&f, &uri("old2"), &bin2).unwrap();
        let legacy = e.legacy_item("old3", "old3", "x");
        let borderline = e.trash(&e.put("borderline", "x"), "borderline").unwrap();
        e.trash(&e.put("fresh", "x"), "fresh").unwrap();
        for id in [old.id, old2.id, legacy] {
            age(&e, id, 31);
        }
        age(&e, borderline.id, 29);
        assert_eq!(e.trash.purge_expired().unwrap(), 3);
        let left: Vec<String> = e
            .trash
            .list()
            .unwrap()
            .iter()
            .map(|i| i.original_uri.to_string())
            .collect();
        assert_eq!(left.len(), 2);
        assert!(!left.iter().any(|u| u.contains("/old")));
        assert!(!old.stored_path().exists());
        assert!(!old2.stored_path().exists());
        assert!(!e.trash.legacy_dir().join("old3").exists());
        assert!(borderline.stored_path().exists());
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
        e.trash(&e.put("f", "1234"), "f").unwrap();
        let d = e.loc.join("d");
        std::fs::create_dir(&d).unwrap();
        std::fs::write(d.join("x"), "123456").unwrap();
        e.trash(&d, "d").unwrap();
        assert_eq!(e.trash.total_size().unwrap(), 10);
    }

    #[test]
    fn unreadable_rows_are_skipped_in_the_listing() {
        let e = env();
        e.trash(&e.put("f", "x"), "f").unwrap();
        e.trash
            .db
            .lock()
            .execute(
                "INSERT INTO trash_items(original_uri, trashed_at, stored_name, is_dir) VALUES ('not a uri', 1, 'zz', 0)",
                [],
            )
            .unwrap();
        assert_eq!(e.trash.list().unwrap().len(), 1);
        assert_eq!(e.trash.bins().unwrap().len(), 2, "its bin is still known");
    }

    #[test]
    fn row_failure_puts_the_item_back() {
        let e = env();
        e.trash.db.lock().execute("DROP TABLE trash_items", []).unwrap();
        let f = e.put("f", "x");
        assert!(e.trash(&f, "f").is_err());
        assert_eq!(std::fs::read(&f).unwrap(), b"x");
        assert_eq!(count(&e.bin), 0);
    }
}
