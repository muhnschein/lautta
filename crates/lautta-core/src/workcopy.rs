// SPDX-License-Identifier: LGPL-2.1-or-later
//! Working copies: remote files opened in other apps (PRV-6).
//!
//! A working copy is a local copy under `~/Downloads/Lautta/Opened/` (SEC-4:
//! readable by other apps while it exists, so every copy expires). The table
//! row keeps the size and mtime of the local file right after the download,
//! so a copy that another app changed is not removed under it.

use crate::db::Db;
use crate::entry::{system_time_to_ms, Kind};
use crate::error::{Error, ErrorKind, Result};
use crate::paths::AppPaths;
use crate::provider::{no_progress, Lane, Provider, ReadOptions};
use crate::transfer::clock::Clock;
use crate::uri::Uri;
use rusqlite::{params, Connection, OptionalExtension, Row};
use std::ffi::OsStr;
use std::os::fd::OwnedFd;
use std::os::unix::ffi::{OsStrExt, OsStringExt};
use std::path::{Path, PathBuf};
use std::sync::Arc;

/// Copies are removed this long after they were last downloaded (PRV-6).
pub const EXPIRY_MS: i64 = 24 * 60 * 60 * 1000;

pub type WorkingCopyId = i64;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct WorkingCopy {
    pub id: WorkingCopyId,
    pub remote: Uri,
    pub local_path: PathBuf,
    /// Size of the local file right after the last download.
    pub local_size: u64,
    /// Local mtime right after the last download.
    pub local_mtime_ms: i64,
    /// When the copy was last downloaded.
    pub fetched_ms: i64,
}

/// True when the local file changed since the last download.
pub fn locally_changed(c: &WorkingCopy, size: u64, mtime_ms: i64) -> bool {
    c.local_mtime_ms != mtime_ms || c.local_size != size
}

/// True when a copy may be removed now (PRV-6).
pub fn is_expired(c: &WorkingCopy, changed: bool, now_ms: i64) -> bool {
    !changed && now_ms - c.fetched_ms >= EXPIRY_MS
}

#[derive(Clone)]
pub struct WorkingCopies {
    db: Db,
    paths: AppPaths,
    clock: Arc<dyn Clock>,
}

async fn blocking<T: Send + 'static>(f: impl FnOnce() -> Result<T> + Send + 'static) -> Result<T> {
    tokio::task::spawn_blocking(f)
        .await
        .map_err(|e| Error::new(ErrorKind::Internal, e.to_string()))?
}

/// Size and mtime (ms) of a working file as it is now.
pub fn local_stamp(path: &Path) -> Result<(u64, i64)> {
    let m = std::fs::metadata(path)?;
    let ms = m.modified().map(system_time_to_ms).unwrap_or(0);
    Ok((m.len(), ms))
}

fn name_of(remote: &Uri) -> Result<Vec<u8>> {
    remote
        .name()
        .map(<[u8]>::to_vec)
        .ok_or_else(|| Error::new(ErrorKind::InvalidArgument, "the location root is not a file"))
}

/// Creates a fresh, uniquely named folder under `base`.
fn unique_dir(base: &Path, now_ms: i64) -> Result<PathBuf> {
    std::fs::create_dir_all(base)?;
    for n in 0..1000u32 {
        let stamp = u64::try_from(now_ms).unwrap_or(0) ^ (u64::from(n) << 40);
        let dir = base.join(format!("{stamp:x}"));
        match std::fs::create_dir(&dir) {
            Ok(()) => return Ok(dir),
            Err(e) if e.kind() == std::io::ErrorKind::AlreadyExists => {}
            Err(e) => return Err(e.into()),
        }
    }
    Err(Error::new(ErrorKind::AlreadyExists, "no free working folder"))
}

impl WorkingCopies {
    pub fn new(db: Db, paths: AppPaths, clock: Arc<dyn Clock>) -> WorkingCopies {
        WorkingCopies { db, paths, clock }
    }

    /// PRV-6: a copy for another app in `~/Downloads/Lautta/Opened/<unique>/name`.
    /// An existing copy of the same file is downloaded again in place.
    pub async fn open_for_view(&self, provider: &dyn Provider, remote: &Uri) -> Result<WorkingCopy> {
        if let Some(existing) = self.reuse(remote).await? {
            return self.refresh(provider, existing).await;
        }
        let entry = provider.stat(&remote.path, true, Lane::Interactive).await?;
        if entry.kind == Kind::Dir {
            return Err(Error::kind(ErrorKind::IsADirectory));
        }
        let name = name_of(remote)?;
        let now = self.clock.now_ms();
        let base = self.paths.opened_dir();
        let (path, file) = blocking(move || {
            let dir = unique_dir(&base, now)?;
            let path = dir.join(OsStr::from_bytes(&name));
            let file = std::fs::OpenOptions::new() // NOSONAR: runs on the blocking pool
                .write(true)
                .create_new(true)
                .open(&path)?;
            Ok((path, file))
        })
        .await?;
        if let Err(e) = download(provider, remote, file, &path, entry.size).await {
            self.discard_files(&path).await;
            return Err(e);
        }
        self.insert(remote, &path).await
    }

    async fn reuse(&self, remote: &Uri) -> Result<Option<WorkingCopy>> {
        let Some(c) = self.find(remote).await? else {
            return Ok(None);
        };
        let path = c.local_path.clone();
        if !blocking(move || Ok(path.exists())).await? {
            self.delete_row(c.id).await?;
            return Ok(None);
        }
        Ok(Some(c))
    }

    /// Downloads a copy again over the old file.
    async fn refresh(&self, provider: &dyn Provider, c: WorkingCopy) -> Result<WorkingCopy> {
        let entry = provider.stat(&c.remote.path, true, Lane::Interactive).await?;
        let path = c.local_path.clone();
        let file = blocking(move || {
            Ok(std::fs::OpenOptions::new() // NOSONAR: runs on the blocking pool
                .write(true)
                .truncate(true)
                .open(path)?)
        })
        .await?;
        download(provider, &c.remote, file, &c.local_path, entry.size).await?;
        self.refetched(c.id).await
    }

    pub async fn get(&self, id: WorkingCopyId) -> Result<WorkingCopy> {
        let db = self.db.clone();
        blocking(move || {
            query_one(&db.lock(), "WHERE id=?", params![id])?
                .ok_or_else(|| Error::new(ErrorKind::NotFound, "no such working copy"))
        })
        .await
    }

    pub async fn find(&self, remote: &Uri) -> Result<Option<WorkingCopy>> {
        let (db, uri) = (self.db.clone(), remote.to_string());
        blocking(move || query_one(&db.lock(), "WHERE remote_uri=?", params![uri])).await
    }

    pub async fn list(&self) -> Result<Vec<WorkingCopy>> {
        let db = self.db.clone();
        blocking(move || query_all(&db.lock())).await
    }

    /// Removes expired copies and their files (PRV-6). Copies another app
    /// changed stay. Returns what was removed.
    pub async fn expire(&self) -> Result<Vec<WorkingCopy>> {
        let now = self.clock.now_ms();
        let mut gone = Vec::new();
        for c in self.list().await? {
            let path = c.local_path.clone();
            let changed = match blocking(move || local_stamp(&path)).await {
                Ok((size, mtime)) => locally_changed(&c, size, mtime),
                Err(_) => false,
            };
            if is_expired(&c, changed, now) {
                self.remove(c.id).await?;
                gone.push(c);
            }
        }
        Ok(gone)
    }

    /// Deletes the copy, its folder and its row.
    pub async fn remove(&self, id: WorkingCopyId) -> Result<()> {
        let c = self.get(id).await?;
        self.discard_files(&c.local_path).await;
        self.delete_row(id).await
    }

    /// Removes a working file and its unique folder, but only inside the
    /// app's own Downloads folder.
    async fn discard_files(&self, file: &Path) {
        let (file, root) = (file.to_owned(), self.paths.opened_dir());
        let removed = blocking(move || {
            let dir = file.parent().map(Path::to_owned);
            match dir {
                Some(d) if d.parent() == Some(root.as_path()) => {
                    std::fs::remove_dir_all(d)?; // NOSONAR: runs on the blocking pool
                }
                _ => {
                    let _ = std::fs::remove_file(&file); // NOSONAR: runs on the blocking pool
                }
            }
            Ok(())
        })
        .await;
        if let Err(e) = removed {
            log::debug!("could not remove working file: {e}");
        }
    }

    async fn insert(&self, remote: &Uri, local: &Path) -> Result<WorkingCopy> {
        let (db, uri, path) = (self.db.clone(), remote.to_string(), local.to_owned());
        let (size, mtime) = {
            let p = path.clone();
            blocking(move || local_stamp(&p)).await?
        };
        let now = self.clock.now_ms();
        blocking(move || {
            let conn = db.lock();
            conn.execute(
                "INSERT INTO working_copies(remote_uri,local_path,local_size,local_mtime_ms,fetched_ms) \
                 VALUES (?,?,?,?,?)",
                params![
                    uri,
                    path.as_os_str().as_bytes(),
                    i64::try_from(size).unwrap_or(i64::MAX),
                    mtime,
                    now,
                ],
            )?;
            let id = conn.last_insert_rowid();
            query_one(&conn, "WHERE id=?", params![id])?
                .ok_or_else(|| Error::new(ErrorKind::Internal, "row vanished"))
        })
        .await
    }

    /// Records a fresh download: the local file as it is now, and the
    /// expiry clock starts again.
    async fn refetched(&self, id: WorkingCopyId) -> Result<WorkingCopy> {
        let c = self.get(id).await?;
        let path = c.local_path.clone();
        let (size, mtime) = blocking(move || local_stamp(&path)).await?;
        let (db, now) = (self.db.clone(), self.clock.now_ms());
        blocking(move || {
            let conn = db.lock();
            conn.execute(
                "UPDATE working_copies SET local_size=?,local_mtime_ms=?,fetched_ms=? WHERE id=?",
                params![i64::try_from(size).unwrap_or(i64::MAX), mtime, now, id],
            )?;
            query_one(&conn, "WHERE id=?", params![id])?
                .ok_or_else(|| Error::new(ErrorKind::Internal, "row vanished"))
        })
        .await
    }

    async fn delete_row(&self, id: WorkingCopyId) -> Result<()> {
        let db = self.db.clone();
        blocking(move || {
            db.lock().execute("DELETE FROM working_copies WHERE id=?", [id])?;
            Ok(())
        })
        .await
    }
}

const COLUMNS: &str = "id,remote_uri,local_path,local_size,local_mtime_ms,fetched_ms";

fn read_row(r: &Row<'_>) -> Result<WorkingCopy> {
    let uri: String = r.get(1)?;
    let path: Vec<u8> = r.get(2)?;
    Ok(WorkingCopy {
        id: r.get(0)?,
        remote: Uri::parse(&uri)?,
        local_path: PathBuf::from(std::ffi::OsString::from_vec(path)),
        local_size: u64::try_from(r.get::<_, i64>(3)?).unwrap_or(0),
        local_mtime_ms: r.get(4)?,
        fetched_ms: r.get(5)?,
    })
}

fn query_one(conn: &Connection, clause: &str, args: impl rusqlite::Params) -> Result<Option<WorkingCopy>> {
    let sql = format!("SELECT {COLUMNS} FROM working_copies {clause}");
    let row = conn.query_row(&sql, args, |r| Ok(read_row(r))).optional()?;
    row.transpose()
}

fn query_all(conn: &Connection) -> Result<Vec<WorkingCopy>> {
    let sql = format!("SELECT {COLUMNS} FROM working_copies ORDER BY id");
    let mut stmt = conn.prepare(&sql)?;
    let mut rows = stmt.query([])?;
    let mut out = Vec::new();
    while let Some(r) = rows.next()? {
        out.push(read_row(r)?);
    }
    Ok(out)
}

/// Downloads into `file` and checks the size.
async fn download(
    provider: &dyn Provider,
    remote: &Uri,
    file: std::fs::File,
    local: &Path,
    expected: Option<u64>,
) -> Result<()> {
    let fd = OwnedFd::from(file);
    provider
        .download_into(&remote.path, fd, ReadOptions::default(), no_progress())
        .await?;
    let path = local.to_owned();
    let (size, _) = blocking(move || local_stamp(&path)).await?;
    if expected.is_some_and(|e| e != size) {
        return Err(Error::new(ErrorKind::Io, "the download ended early"));
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::entry::ms_to_system_time;
    use crate::provider::memory::MemoryProvider;
    use crate::transfer::clock::ManualClock;
    use crate::vpath::VPath;

    const T0: i64 = 1_700_000_000_000;

    struct Rig {
        wc: WorkingCopies,
        remote: MemoryProvider,
        clock: Arc<ManualClock>,
        home: tempfile::TempDir,
    }

    fn rig() -> Rig {
        let home = tempfile::tempdir().unwrap();
        let clock = ManualClock::new(T0);
        let wc = WorkingCopies::new(
            Db::open_in_memory().unwrap(),
            AppPaths::new(home.path()),
            clock.clone(),
        );
        Rig {
            wc,
            remote: MemoryProvider::default(),
            clock,
            home,
        }
    }

    fn uri(p: &str) -> Uri {
        Uri::new("nv-srv", VPath::parse(p.as_bytes()).unwrap())
    }

    fn write_local(c: &WorkingCopy, data: &[u8], mtime_ms: i64) {
        std::fs::write(&c.local_path, data).unwrap();
        let f = std::fs::OpenOptions::new()
            .write(true)
            .open(&c.local_path)
            .unwrap();
        f.set_modified(ms_to_system_time(mtime_ms)).unwrap();
    }

    fn copy() -> WorkingCopy {
        WorkingCopy {
            id: 1,
            remote: uri("a"),
            local_path: PathBuf::new(),
            local_size: 5,
            local_mtime_ms: 10,
            fetched_ms: 1000,
        }
    }

    #[test]
    fn expiry_rules() {
        let c = copy();
        assert!(!is_expired(&c, false, 1000 + EXPIRY_MS - 1));
        assert!(is_expired(&c, false, 1000 + EXPIRY_MS));
        assert!(!is_expired(&c, true, 1000 + EXPIRY_MS), "changed copies are kept");
    }

    #[test]
    fn local_change_detection() {
        let c = copy();
        assert!(!locally_changed(&c, 5, 10));
        assert!(locally_changed(&c, 6, 10));
        assert!(locally_changed(&c, 5, 11));
    }

    #[tokio::test]
    async fn open_for_view_downloads_into_the_opened_folder() {
        let r = rig();
        r.remote.add_file("docs/notes.txt", b"hello", 5000);
        let c =
            r.wc.open_for_view(&r.remote, &uri("docs/notes.txt"))
                .await
                .unwrap();
        assert_eq!(std::fs::read(&c.local_path).unwrap(), b"hello");
        assert_eq!(c.local_path.file_name().unwrap(), "notes.txt");
        let opened = AppPaths::new(r.home.path()).opened_dir();
        assert_eq!(c.local_path.parent().unwrap().parent().unwrap(), opened);
        let (size, mtime) = local_stamp(&c.local_path).unwrap();
        assert_eq!((c.local_size, c.local_mtime_ms), (size, mtime));
        assert_eq!(c.fetched_ms, T0);
        assert_eq!(r.wc.get(c.id).await.unwrap(), c);
        assert_eq!(r.wc.find(&uri("docs/notes.txt")).await.unwrap(), Some(c.clone()));
        assert_eq!(r.wc.list().await.unwrap(), vec![c]);
    }

    #[tokio::test]
    async fn open_rejects_folders_and_roots_and_cleans_up_failures() {
        let r = rig();
        r.remote.add_dir("d");
        let e = r.wc.open_for_view(&r.remote, &uri("d")).await.unwrap_err();
        assert_eq!(e.kind, ErrorKind::IsADirectory);
        let e = r.wc.open_for_view(&r.remote, &uri("")).await.unwrap_err();
        assert_eq!(e.kind, ErrorKind::IsADirectory);
        r.remote.add_file("f", b"x", 1);
        r.remote
            .fail_next("download_into", "f", Error::kind(ErrorKind::TimedOut));
        let e = r.wc.open_for_view(&r.remote, &uri("f")).await.unwrap_err();
        assert_eq!(e.kind, ErrorKind::TimedOut);
        let opened = AppPaths::new(r.home.path()).opened_dir();
        assert_eq!(std::fs::read_dir(opened).unwrap().count(), 0, "no leftovers");
        assert!(r.wc.list().await.unwrap().is_empty());
    }

    #[tokio::test]
    async fn opening_again_downloads_again_and_restarts_the_clock() {
        let r = rig();
        r.remote.add_file("pic.jpg", b"v1", 1000);
        let v = r.wc.open_for_view(&r.remote, &uri("pic.jpg")).await.unwrap();
        r.remote.add_file("pic.jpg", b"version two", 2000);
        r.clock.advance(EXPIRY_MS - 10);
        let again = r.wc.open_for_view(&r.remote, &uri("pic.jpg")).await.unwrap();
        assert_eq!((again.id, &again.local_path), (v.id, &v.local_path));
        assert_eq!(std::fs::read(&again.local_path).unwrap(), b"version two");
        assert_eq!(again.local_size, 11);
        assert_eq!(again.fetched_ms, T0 + EXPIRY_MS - 10);
        r.clock.advance(20);
        assert!(r.wc.expire().await.unwrap().is_empty());
        r.clock.advance(EXPIRY_MS);
        assert_eq!(r.wc.expire().await.unwrap().len(), 1);
    }

    #[tokio::test]
    async fn expiry_removes_after_24_hours_unless_changed() {
        let r = rig();
        for n in ["a", "b"] {
            r.remote.add_file(n, b"x", 1000);
        }
        let a = r.wc.open_for_view(&r.remote, &uri("a")).await.unwrap();
        let b = r.wc.open_for_view(&r.remote, &uri("b")).await.unwrap();
        write_local(&b, b"changed by another app", 4000);
        r.clock.advance(EXPIRY_MS - 1);
        assert!(r.wc.expire().await.unwrap().is_empty());
        r.clock.advance(1);
        let gone: Vec<WorkingCopyId> = r.wc.expire().await.unwrap().iter().map(|c| c.id).collect();
        assert_eq!(gone, vec![a.id]);
        assert!(!a.local_path.exists() && !a.local_path.parent().unwrap().exists());
        assert!(b.local_path.exists());
        assert_eq!(r.wc.list().await.unwrap().len(), 1, "the changed copy stays");
    }

    #[tokio::test]
    async fn a_stale_row_without_its_file_is_replaced() {
        let r = rig();
        r.remote.add_file("f", b"x", 1);
        let c = r.wc.open_for_view(&r.remote, &uri("f")).await.unwrap();
        std::fs::remove_dir_all(c.local_path.parent().unwrap()).unwrap();
        let fresh = r.wc.open_for_view(&r.remote, &uri("f")).await.unwrap();
        assert!(fresh.local_path.exists(), "downloaded again");
        assert_eq!(r.wc.list().await.unwrap(), vec![fresh]);
    }

    #[tokio::test]
    async fn remove_deletes_everything() {
        let r = rig();
        r.remote.add_file("f", b"x", 1);
        let c = r.wc.open_for_view(&r.remote, &uri("f")).await.unwrap();
        r.wc.remove(c.id).await.unwrap();
        assert!(!c.local_path.exists());
        assert_eq!(r.wc.get(c.id).await.unwrap_err().kind, ErrorKind::NotFound);
    }
}
