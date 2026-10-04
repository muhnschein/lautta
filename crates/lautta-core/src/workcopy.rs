// SPDX-License-Identifier: LGPL-2.1-or-later
//! Working copies for edit-in-place with write-back (EDT-1..3) and for remote
//! files opened in other apps (PRV-6).
//!
//! A working copy is a local copy under `~/Downloads/Lautta/` (SEC-4: readable
//! by other apps while it exists, so every copy expires). The table row keeps
//! the baseline of the remote file when the copy was made or last uploaded
//! (size, mtime, etag); a change of the remote since then is a conflict.
//! `purpose` (Edit or Open) lives in the `state` column.

use crate::db::Db;
use crate::entry::{cap, ms_to_system_time, system_time_to_ms, Entry, Kind};
use crate::error::{Error, ErrorKind, Result};
use crate::paths::AppPaths;
use crate::provider::{no_progress, Disposition, Lane, Provider, ReadOptions, RenameMode, WriteOptions};
use crate::transfer::clock::Clock;
use crate::transfer::conflict::numbered_name;
use crate::transfer::exec::temp_name_for;
use crate::uri::Uri;
use rusqlite::{params, Connection, OptionalExtension, Row};
use std::ffi::OsStr;
use std::os::fd::OwnedFd;
use std::os::unix::ffi::{OsStrExt, OsStringExt};
use std::path::{Path, PathBuf};
use std::sync::Arc;

/// Copies are removed this long after the last successful upload (EDT-3) or,
/// for copies opened in other apps, after they were made (PRV-6).
pub const EXPIRY_MS: i64 = 24 * 60 * 60 * 1000;

pub type WorkingCopyId = i64;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Purpose {
    /// *Edit*: watched and written back (EDT-1).
    Edit,
    /// Opened read-only in another app (PRV-6).
    Open,
}

impl Purpose {
    fn db(self) -> &'static str {
        match self {
            Purpose::Edit => "edit",
            Purpose::Open => "open",
        }
    }

    fn from_db(s: &str) -> Purpose {
        if s == "open" {
            Purpose::Open
        } else {
            Purpose::Edit
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct WorkingCopy {
    pub id: WorkingCopyId,
    pub remote: Uri,
    pub local_path: PathBuf,
    pub base_size: Option<u64>,
    pub base_mtime_ms: Option<i64>,
    pub base_etag: Option<Vec<u8>>,
    /// Local mtime at the last download or upload.
    pub local_mtime_ms: Option<i64>,
    pub pinned: bool,
    /// Last successful upload; the creation time until the first upload.
    pub last_upload_ms: Option<i64>,
    pub purpose: Purpose,
}

/// What the user can answer when the remote changed meanwhile (EDT-2).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum EditConflictChoice {
    UploadMineAndReplace,
    SaveMineAsCopy,
    DiscardMine,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct EditConflict {
    pub id: WorkingCopyId,
    pub remote: Uri,
    pub local_size: u64,
    /// `None` when the remote file is gone.
    pub remote_size: Option<u64>,
    pub remote_mtime_ms: Option<i64>,
    pub choices: Vec<EditConflictChoice>,
}

/// An upload to run at high priority (EDT-2).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct UploadRequest {
    pub id: WorkingCopyId,
    pub remote: Uri,
    pub local_path: PathBuf,
    pub size: u64,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Decision {
    /// The local file did not change since the baseline.
    Unchanged,
    Upload(UploadRequest),
    Conflict(EditConflict),
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Resolution {
    /// Uploaded over the remote; the new baseline is in the record.
    Uploaded(WorkingCopy),
    /// Saved next to the original; the working copy is gone.
    SavedCopy(Uri),
    Discarded,
}

/// The state of the remote file when comparing with a baseline.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RemoteStamp {
    pub size: Option<u64>,
    pub mtime_ms: Option<i64>,
    pub etag: Option<Vec<u8>>,
}

impl RemoteStamp {
    fn of(e: &Entry) -> RemoteStamp {
        RemoteStamp {
            size: e.size,
            mtime_ms: e.modified_ms(),
            etag: e.etag.clone(),
        }
    }
}

/// True when the remote differs from the recorded baseline. ETags decide when
/// both sides have one; otherwise size and mtime.
pub fn remote_changed(c: &WorkingCopy, now: &RemoteStamp) -> bool {
    if let (Some(a), Some(b)) = (&c.base_etag, &now.etag) {
        return a != b;
    }
    c.base_size != now.size || c.base_mtime_ms != now.mtime_ms
}

/// True when the local file changed since the baseline (EDT-3 start scan).
pub fn locally_changed(c: &WorkingCopy, size: u64, mtime_ms: i64) -> bool {
    c.local_mtime_ms != Some(mtime_ms) || c.base_size != Some(size)
}

/// True when a copy may be removed now (EDT-3, PRV-6).
pub fn is_expired(c: &WorkingCopy, dirty: bool, now_ms: i64) -> bool {
    !c.pinned && !dirty && c.last_upload_ms.is_some_and(|t| now_ms - t >= EXPIRY_MS)
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

    /// EDT-1: downloads `remote` into `~/Downloads/Lautta/Editing/<unique>/name`
    /// and records the baseline. An existing copy of the same file is returned
    /// as it is, so unsent edits survive a second *Edit*.
    pub async fn open_for_edit(&self, provider: &dyn Provider, remote: &Uri) -> Result<WorkingCopy> {
        self.open_in(Purpose::Edit, provider, remote, &self.paths.editing_dir())
            .await
    }

    /// PRV-6: a copy for another app, in `~/Downloads/Lautta/Opened/`.
    pub async fn open_for_view(&self, provider: &dyn Provider, remote: &Uri) -> Result<WorkingCopy> {
        self.open_in(Purpose::Open, provider, remote, &self.paths.opened_dir())
            .await
    }

    pub async fn open_in(
        &self,
        purpose: Purpose,
        provider: &dyn Provider,
        remote: &Uri,
        base: &Path,
    ) -> Result<WorkingCopy> {
        if let Some(existing) = self.reuse(purpose, remote).await? {
            if purpose == Purpose::Edit || existing.purpose == Purpose::Edit {
                return Ok(existing);
            }
            return self.refresh(provider, existing).await;
        }
        let entry = provider.stat(&remote.path, true, Lane::Interactive).await?;
        if entry.kind == Kind::Dir {
            return Err(Error::kind(ErrorKind::IsADirectory));
        }
        let name = name_of(remote)?;
        let now = self.clock.now_ms();
        let base = base.to_owned();
        let (dir, file) = blocking(move || {
            let dir = unique_dir(&base, now)?;
            let path = dir.join(OsStr::from_bytes(&name));
            let file = std::fs::OpenOptions::new() // NOSONAR: runs on the blocking pool
                .write(true)
                .create_new(true)
                .open(&path)?;
            Ok((path, file))
        })
        .await?;
        if let Err(e) = download(provider, remote, file, &dir, entry.size).await {
            self.discard_files(&dir).await;
            return Err(e);
        }
        self.insert(purpose, remote, &dir, &entry).await
    }

    async fn reuse(&self, purpose: Purpose, remote: &Uri) -> Result<Option<WorkingCopy>> {
        let Some(c) = self.find(remote).await? else {
            return Ok(None);
        };
        let path = c.local_path.clone();
        if !blocking(move || Ok(path.exists())).await? {
            self.delete_row(c.id).await?;
            return Ok(None);
        }
        if purpose == Purpose::Edit && c.purpose == Purpose::Open {
            return self.set_purpose(c.id, Purpose::Edit).await.map(Some);
        }
        Ok(Some(c))
    }

    /// Downloads an *Open* copy again over the old file.
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
        self.rebaseline(c.id, &entry).await
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

    /// Lists copies, optionally of one purpose (the *Edited files* group).
    pub async fn list(&self, purpose: Option<Purpose>) -> Result<Vec<WorkingCopy>> {
        let db = self.db.clone();
        let all = blocking(move || query_all(&db.lock())).await?;
        Ok(all
            .into_iter()
            .filter(|c| purpose.map_or(true, |p| c.purpose == p))
            .collect())
    }

    pub async fn pin(&self, id: WorkingCopyId, pinned: bool) -> Result<()> {
        let db = self.db.clone();
        blocking(move || {
            let n = db.lock().execute(
                "UPDATE working_copies SET pinned=? WHERE id=?",
                params![pinned, id],
            )?;
            if n == 0 {
                return Err(Error::new(ErrorKind::NotFound, "no such working copy"));
            }
            Ok(())
        })
        .await
    }

    /// EDT-2: the local file changed (close-after-write, debounced by the
    /// caller). Stats the remote and decides.
    pub async fn on_local_change(&self, provider: &dyn Provider, id: WorkingCopyId) -> Result<Decision> {
        let c = self.get(id).await?;
        if c.purpose != Purpose::Edit {
            return Err(Error::new(ErrorKind::InvalidArgument, "not an edit copy"));
        }
        let path = c.local_path.clone();
        let (size, mtime) = blocking(move || local_stamp(&path)).await?;
        if !locally_changed(&c, size, mtime) {
            return Ok(Decision::Unchanged);
        }
        let remote = match provider.stat(&c.remote.path, true, Lane::Interactive).await {
            Ok(e) => Some(RemoteStamp::of(&e)),
            Err(e) if e.kind == ErrorKind::NotFound => None,
            Err(e) => return Err(e),
        };
        match remote {
            Some(stamp) if !remote_changed(&c, &stamp) => Ok(Decision::Upload(UploadRequest {
                id,
                remote: c.remote.clone(),
                local_path: c.local_path.clone(),
                size,
            })),
            other => Ok(Decision::Conflict(EditConflict {
                id,
                remote: c.remote.clone(),
                local_size: size,
                remote_size: other.as_ref().and_then(|s| s.size),
                remote_mtime_ms: other.as_ref().and_then(|s| s.mtime_ms),
                choices: vec![
                    EditConflictChoice::UploadMineAndReplace,
                    EditConflictChoice::SaveMineAsCopy,
                    EditConflictChoice::DiscardMine,
                ],
            })),
        }
    }

    /// Uploads the working copy over the remote file and records the new
    /// baseline (EDT-2). Written through a temporary name unless the backend
    /// is `AtomicPut` (NVB-11).
    pub async fn upload(&self, provider: &dyn Provider, id: WorkingCopyId) -> Result<WorkingCopy> {
        let c = self.get(id).await?;
        let name = name_of(&c.remote)?;
        let temp = temp_name_for(&name, id, 0);
        upload_file(provider, &c.local_path, &c.remote, &temp).await?;
        let entry = provider.stat(&c.remote.path, true, Lane::Interactive).await?;
        self.rebaseline(id, &entry).await
    }

    /// Answers an edit conflict (EDT-2).
    pub async fn resolve(
        &self,
        provider: &dyn Provider,
        id: WorkingCopyId,
        choice: EditConflictChoice,
    ) -> Result<Resolution> {
        match choice {
            EditConflictChoice::UploadMineAndReplace => {
                self.upload(provider, id).await.map(Resolution::Uploaded)
            }
            EditConflictChoice::SaveMineAsCopy => {
                let c = self.get(id).await?;
                let copy = free_sibling(provider, &c.remote).await?;
                let temp = temp_name_for(&name_of(&copy)?, id, 1);
                upload_file(provider, &c.local_path, &copy, &temp).await?;
                self.remove(id).await?;
                Ok(Resolution::SavedCopy(copy))
            }
            EditConflictChoice::DiscardMine => {
                self.remove(id).await?;
                Ok(Resolution::Discarded)
            }
        }
    }

    /// EDT-3: at start, edit copies whose local file changed since the last
    /// upload (watching only happens while the app runs). Copies whose file
    /// vanished are dropped.
    pub async fn scan_at_start(&self) -> Result<Vec<WorkingCopy>> {
        let mut dirty = Vec::new();
        for c in self.list(Some(Purpose::Edit)).await? {
            let path = c.local_path.clone();
            match blocking(move || local_stamp(&path)).await {
                Ok((size, mtime)) if locally_changed(&c, size, mtime) => dirty.push(c),
                Ok(_) => {}
                Err(e) if e.kind == ErrorKind::NotFound => self.delete_row(c.id).await?,
                Err(e) => return Err(e),
            }
        }
        Ok(dirty)
    }

    /// Removes expired copies and their files (EDT-3, PRV-6). Copies with
    /// unsent changes stay. Returns what was removed.
    pub async fn expire(&self) -> Result<Vec<WorkingCopy>> {
        let now = self.clock.now_ms();
        let mut gone = Vec::new();
        for c in self.list(None).await? {
            let path = c.local_path.clone();
            let dirty = match blocking(move || local_stamp(&path)).await {
                Ok((size, mtime)) => locally_changed(&c, size, mtime),
                Err(_) => false,
            };
            if is_expired(&c, dirty, now) {
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
    /// app's own Downloads folders.
    async fn discard_files(&self, file: &Path) {
        let (file, roots) = (
            file.to_owned(),
            [self.paths.editing_dir(), self.paths.opened_dir()],
        );
        let removed = blocking(move || {
            let dir = file.parent().map(Path::to_owned);
            match dir {
                Some(d) if roots.iter().any(|r| d.parent() == Some(r.as_path())) => {
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

    async fn insert(
        &self,
        purpose: Purpose,
        remote: &Uri,
        local: &Path,
        entry: &Entry,
    ) -> Result<WorkingCopy> {
        let (db, uri, path) = (self.db.clone(), remote.to_string(), local.to_owned());
        let mtime = {
            let p = path.clone();
            blocking(move || local_stamp(&p)).await?.1
        };
        let now = self.clock.now_ms();
        let entry = entry.clone();
        blocking(move || {
            let conn = db.lock();
            conn.execute(
                "INSERT INTO working_copies(remote_uri,local_path,base_size,base_mtime_ms,\
                 base_etag,local_mtime_ms,pinned,last_upload_ms,state) \
                 VALUES (?,?,?,?,?,?,0,?,?)",
                params![
                    uri,
                    path.as_os_str().as_bytes(),
                    entry.size.map(|s| i64::try_from(s).unwrap_or(i64::MAX)),
                    entry.modified_ms(),
                    entry.etag,
                    mtime,
                    now,
                    purpose.db(),
                ],
            )?;
            let id = conn.last_insert_rowid();
            query_one(&conn, "WHERE id=?", params![id])?
                .ok_or_else(|| Error::new(ErrorKind::Internal, "row vanished"))
        })
        .await
    }

    /// New baseline from the remote's current state and the local file as it
    /// is now; also restarts the expiry clock (a fresh upload or download).
    async fn rebaseline(&self, id: WorkingCopyId, remote: &Entry) -> Result<WorkingCopy> {
        let c = self.get(id).await?;
        let path = c.local_path.clone();
        let (_, mtime) = blocking(move || local_stamp(&path)).await?;
        let (db, entry, now) = (self.db.clone(), remote.clone(), self.clock.now_ms());
        blocking(move || {
            let conn = db.lock();
            conn.execute(
                "UPDATE working_copies SET base_size=?,base_mtime_ms=?,base_etag=?,\
                 local_mtime_ms=?,last_upload_ms=? WHERE id=?",
                params![
                    entry.size.map(|s| i64::try_from(s).unwrap_or(i64::MAX)),
                    entry.modified_ms(),
                    entry.etag,
                    mtime,
                    now,
                    id,
                ],
            )?;
            query_one(&conn, "WHERE id=?", params![id])?
                .ok_or_else(|| Error::new(ErrorKind::Internal, "row vanished"))
        })
        .await
    }

    async fn set_purpose(&self, id: WorkingCopyId, purpose: Purpose) -> Result<WorkingCopy> {
        let db = self.db.clone();
        blocking(move || {
            let conn = db.lock();
            conn.execute(
                "UPDATE working_copies SET state=? WHERE id=?",
                params![purpose.db(), id],
            )?;
            query_one(&conn, "WHERE id=?", params![id])?
                .ok_or_else(|| Error::new(ErrorKind::NotFound, "no such working copy"))
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

const COLUMNS: &str = "id,remote_uri,local_path,base_size,base_mtime_ms,base_etag,\
                       local_mtime_ms,pinned,last_upload_ms,state";

fn read_row(r: &Row<'_>) -> Result<WorkingCopy> {
    let uri: String = r.get(1)?;
    let path: Vec<u8> = r.get(2)?;
    let state: String = r.get(9)?;
    Ok(WorkingCopy {
        id: r.get(0)?,
        remote: Uri::parse(&uri)?,
        local_path: PathBuf::from(std::ffi::OsString::from_vec(path)),
        base_size: r.get::<_, Option<i64>>(3)?.map(|v| u64::try_from(v).unwrap_or(0)),
        base_mtime_ms: r.get(4)?,
        base_etag: r.get(5)?,
        local_mtime_ms: r.get(6)?,
        pinned: r.get(7)?,
        last_upload_ms: r.get(8)?,
        purpose: Purpose::from_db(&state),
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

/// Uploads a local file to `remote`, replacing it, through a temporary name
/// unless the backend writes atomically (NVB-11).
async fn upload_file(provider: &dyn Provider, local: &Path, remote: &Uri, temp_name: &[u8]) -> Result<()> {
    let path = local.to_owned();
    let (file, size, mtime) = blocking(move || {
        let f = std::fs::File::open(&path)?; // NOSONAR: runs on the blocking pool
        let (size, mtime) = local_stamp(&path)?;
        Ok((f, size, mtime))
    })
    .await?;
    let modified = Some(ms_to_system_time(mtime));
    let atomic = provider.capabilities().has(cap::ATOMIC_PUT);
    let target = if atomic {
        remote.path.clone()
    } else {
        let parent = remote
            .path
            .parent()
            .ok_or_else(|| Error::new(ErrorKind::InvalidArgument, "no parent folder"))?;
        parent.join(temp_name)?
    };
    if !atomic {
        match provider.remove_file(&target).await {
            Err(e) if e.kind != ErrorKind::NotFound => return Err(e),
            _ => {}
        }
    }
    let opts = WriteOptions {
        disposition: if atomic {
            Disposition::Truncate
        } else {
            Disposition::Create
        },
        offset: 0,
        size: Some(size),
        modified,
        mode: None,
    };
    provider
        .upload_from(OwnedFd::from(file), &target, opts, no_progress())
        .await?;
    if !atomic {
        provider
            .rename(&target, &remote.path, RenameMode::Replace)
            .await?;
    }
    Ok(())
}

/// `name 2.ext`, `name 3.ext`… next to the original: the first free name.
async fn free_sibling(provider: &dyn Provider, remote: &Uri) -> Result<Uri> {
    let name = name_of(remote)?;
    for n in 2..1000 {
        let candidate = remote
            .parent()
            .ok_or_else(|| Error::new(ErrorKind::InvalidArgument, "no parent folder"))?
            .join(&numbered_name(&name, n, false))?;
        match provider.stat(&candidate.path, false, Lane::Interactive).await {
            Err(e) if e.kind == ErrorKind::NotFound => return Ok(candidate),
            Err(e) => return Err(e),
            Ok(_) => {}
        }
    }
    Err(Error::new(ErrorKind::AlreadyExists, "no free name for the copy"))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::entry::{ms_to_system_time, Capabilities};
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

    #[test]
    fn remote_comparison_prefers_etags() {
        let mut c = WorkingCopy {
            id: 1,
            remote: uri("a"),
            local_path: PathBuf::new(),
            base_size: Some(5),
            base_mtime_ms: Some(100),
            base_etag: Some(b"v1".to_vec()),
            local_mtime_ms: None,
            pinned: false,
            last_upload_ms: None,
            purpose: Purpose::Edit,
        };
        let same = RemoteStamp {
            size: Some(5),
            mtime_ms: Some(100),
            etag: Some(b"v1".to_vec()),
        };
        assert!(!remote_changed(&c, &same));
        let newer_tag = RemoteStamp {
            etag: Some(b"v2".to_vec()),
            ..same.clone()
        };
        assert!(
            remote_changed(&c, &newer_tag),
            "etag differs although size and mtime match"
        );
        let touched = RemoteStamp {
            mtime_ms: Some(999),
            ..same.clone()
        };
        assert!(!remote_changed(&c, &touched), "etags decide when both exist");
        c.base_etag = None;
        assert!(remote_changed(&c, &touched));
        assert!(remote_changed(
            &c,
            &RemoteStamp {
                size: Some(6),
                ..same.clone()
            }
        ));
        assert!(!remote_changed(&c, &RemoteStamp { etag: None, ..same }));
    }

    #[test]
    fn expiry_rules() {
        let mut c = WorkingCopy {
            id: 1,
            remote: uri("a"),
            local_path: PathBuf::new(),
            base_size: None,
            base_mtime_ms: None,
            base_etag: None,
            local_mtime_ms: None,
            pinned: false,
            last_upload_ms: Some(1000),
            purpose: Purpose::Edit,
        };
        assert!(!is_expired(&c, false, 1000 + EXPIRY_MS - 1));
        assert!(is_expired(&c, false, 1000 + EXPIRY_MS));
        assert!(!is_expired(&c, true, 1000 + EXPIRY_MS), "unsent edits are kept");
        c.pinned = true;
        assert!(!is_expired(&c, false, 1000 + EXPIRY_MS));
        c.pinned = false;
        c.last_upload_ms = None;
        assert!(!is_expired(&c, false, i64::MAX));
    }

    #[test]
    fn local_change_detection() {
        let mut c = WorkingCopy {
            id: 1,
            remote: uri("a"),
            local_path: PathBuf::new(),
            base_size: Some(5),
            base_mtime_ms: None,
            base_etag: None,
            local_mtime_ms: Some(10),
            pinned: false,
            last_upload_ms: None,
            purpose: Purpose::Edit,
        };
        assert!(!locally_changed(&c, 5, 10));
        assert!(locally_changed(&c, 6, 10));
        assert!(locally_changed(&c, 5, 11));
        c.local_mtime_ms = None;
        assert!(locally_changed(&c, 5, 10));
    }

    #[tokio::test]
    async fn open_for_edit_downloads_and_records_the_baseline() {
        let r = rig();
        r.remote.add_file("docs/notes.txt", b"hello", 5000);
        let c =
            r.wc.open_for_edit(&r.remote, &uri("docs/notes.txt"))
                .await
                .unwrap();
        assert_eq!(c.purpose, Purpose::Edit);
        assert_eq!(std::fs::read(&c.local_path).unwrap(), b"hello");
        assert_eq!(c.local_path.file_name().unwrap(), "notes.txt");
        let editing = AppPaths::new(r.home.path()).editing_dir();
        assert_eq!(c.local_path.parent().unwrap().parent().unwrap(), editing);
        assert_eq!((c.base_size, c.base_mtime_ms), (Some(5), Some(5000)));
        assert_eq!(c.last_upload_ms, Some(T0));
        assert!(!c.pinned);
        assert_eq!(r.wc.get(c.id).await.unwrap(), c);
        assert_eq!(r.wc.find(&uri("docs/notes.txt")).await.unwrap(), Some(c.clone()));
        assert_eq!(r.wc.list(Some(Purpose::Edit)).await.unwrap(), vec![c.clone()]);
        assert!(r.wc.list(Some(Purpose::Open)).await.unwrap().is_empty());

        let again =
            r.wc.open_for_edit(&r.remote, &uri("docs/notes.txt"))
                .await
                .unwrap();
        assert_eq!(again, c, "a second Edit keeps the copy");
    }

    #[tokio::test]
    async fn open_rejects_folders_and_roots_and_cleans_up_failures() {
        let r = rig();
        r.remote.add_dir("d");
        let e = r.wc.open_for_edit(&r.remote, &uri("d")).await.unwrap_err();
        assert_eq!(e.kind, ErrorKind::IsADirectory);
        let e = r.wc.open_for_edit(&r.remote, &uri("")).await.unwrap_err();
        assert_eq!(e.kind, ErrorKind::IsADirectory);
        r.remote.add_file("f", b"x", 1);
        r.remote
            .fail_next("download_into", "f", Error::kind(ErrorKind::TimedOut));
        let e = r.wc.open_for_edit(&r.remote, &uri("f")).await.unwrap_err();
        assert_eq!(e.kind, ErrorKind::TimedOut);
        let editing = AppPaths::new(r.home.path()).editing_dir();
        assert_eq!(std::fs::read_dir(editing).unwrap().count(), 0, "no leftovers");
        assert!(r.wc.list(None).await.unwrap().is_empty());
    }

    #[tokio::test]
    async fn unchanged_remote_means_upload_and_a_new_baseline() {
        let r = rig();
        r.remote.add_file("notes.txt", b"hello", 5000);
        let c = r.wc.open_for_edit(&r.remote, &uri("notes.txt")).await.unwrap();
        assert_eq!(
            r.wc.on_local_change(&r.remote, c.id).await.unwrap(),
            Decision::Unchanged,
            "nothing was written yet"
        );
        write_local(&c, b"hello world", 9000);
        let Decision::Upload(req) = r.wc.on_local_change(&r.remote, c.id).await.unwrap() else {
            panic!("expected an upload");
        };
        assert_eq!((req.id, req.size), (c.id, 11));
        assert_eq!(req.remote, uri("notes.txt"));

        r.clock.advance(1000);
        let done = r.wc.upload(&r.remote, c.id).await.unwrap();
        assert_eq!(r.remote.read_file("notes.txt").unwrap(), b"hello world");
        assert_eq!(done.base_size, Some(11));
        assert_eq!(done.base_mtime_ms, Some(9000), "mtime travels with the file");
        assert_eq!(done.local_mtime_ms, Some(9000));
        assert_eq!(done.last_upload_ms, Some(T0 + 1000));
        assert_eq!(
            r.wc.on_local_change(&r.remote, c.id).await.unwrap(),
            Decision::Unchanged,
            "the baseline moved with the upload"
        );
        let calls = r.remote.calls().join("|");
        assert!(
            calls.contains("rename"),
            "written through a temporary name: {calls}"
        );
        assert!(!r.remote.paths().iter().any(|p| p.contains(".lautta-")));
    }

    #[tokio::test]
    async fn atomic_put_backends_are_written_directly() {
        let r = rig();
        let remote = MemoryProvider::new(Capabilities::with(&[cap::WRITE, cap::ATOMIC_PUT, cap::SET_MTIME]));
        remote.add_file("f", b"abc", 5000);
        let c = r.wc.open_for_edit(&remote, &uri("f")).await.unwrap();
        write_local(&c, b"abcdef", 6000);
        r.wc.upload(&remote, c.id).await.unwrap();
        assert_eq!(remote.read_file("f").unwrap(), b"abcdef");
        assert!(!remote.calls().iter().any(|c| c.starts_with("rename")));
    }

    #[tokio::test]
    async fn a_changed_remote_is_a_conflict_with_three_choices() {
        let r = rig();
        r.remote.add_file("notes.txt", b"hello", 5000);
        let c = r.wc.open_for_edit(&r.remote, &uri("notes.txt")).await.unwrap();
        write_local(&c, b"mine!", 9000);
        r.remote.add_file("notes.txt", b"theirs!!", 7000);
        let Decision::Conflict(k) = r.wc.on_local_change(&r.remote, c.id).await.unwrap() else {
            panic!("expected a conflict");
        };
        assert_eq!(
            (k.local_size, k.remote_size, k.remote_mtime_ms),
            (5, Some(8), Some(7000))
        );
        assert_eq!(
            k.choices,
            vec![
                EditConflictChoice::UploadMineAndReplace,
                EditConflictChoice::SaveMineAsCopy,
                EditConflictChoice::DiscardMine
            ]
        );
        assert_eq!(
            r.remote.read_file("notes.txt").unwrap(),
            b"theirs!!",
            "nothing uploaded"
        );
    }

    #[tokio::test]
    async fn a_deleted_remote_is_a_conflict_too() {
        let r = rig();
        r.remote.add_file("f", b"x", 1);
        let c = r.wc.open_for_edit(&r.remote, &uri("f")).await.unwrap();
        write_local(&c, b"xy", 2);
        let other = MemoryProvider::default();
        let Decision::Conflict(k) = r.wc.on_local_change(&other, c.id).await.unwrap() else {
            panic!("expected a conflict");
        };
        assert_eq!(k.remote_size, None);
    }

    #[tokio::test]
    async fn resolving_a_conflict() {
        let r = rig();
        for (choice, name) in [
            (EditConflictChoice::UploadMineAndReplace, "a.txt"),
            (EditConflictChoice::SaveMineAsCopy, "b.txt"),
            (EditConflictChoice::DiscardMine, "c.txt"),
        ] {
            r.remote.add_file(name, b"orig", 1000);
            let c = r.wc.open_for_edit(&r.remote, &uri(name)).await.unwrap();
            write_local(&c, b"mine", 3000);
            r.remote.add_file(name, b"theirs", 2000);
            let res = r.wc.resolve(&r.remote, c.id, choice).await.unwrap();
            match choice {
                EditConflictChoice::UploadMineAndReplace => {
                    let Resolution::Uploaded(u) = res else { panic!() };
                    assert_eq!(r.remote.read_file(name).unwrap(), b"mine");
                    assert_eq!(u.base_size, Some(4));
                    assert!(c.local_path.exists());
                }
                EditConflictChoice::SaveMineAsCopy => {
                    let Resolution::SavedCopy(copy) = res else {
                        panic!()
                    };
                    assert_eq!(copy.path.display(), "b 2.txt");
                    assert_eq!(r.remote.read_file(name).unwrap(), b"theirs");
                    assert_eq!(r.remote.read_file("b 2.txt").unwrap(), b"mine");
                    assert!(!c.local_path.exists());
                    assert!(r.wc.get(c.id).await.is_err());
                }
                EditConflictChoice::DiscardMine => {
                    assert_eq!(res, Resolution::Discarded);
                    assert_eq!(r.remote.read_file(name).unwrap(), b"theirs");
                    assert!(!c.local_path.exists());
                    assert!(!c.local_path.parent().unwrap().exists(), "the folder goes too");
                }
            }
        }
    }

    #[tokio::test]
    async fn scan_at_start_finds_copies_edited_while_the_app_was_closed() {
        let r = rig();
        r.remote.add_file("edited", b"one", 1000);
        r.remote.add_file("untouched", b"two", 1000);
        r.remote.add_file("gone", b"three", 1000);
        let edited = r.wc.open_for_edit(&r.remote, &uri("edited")).await.unwrap();
        r.wc.open_for_edit(&r.remote, &uri("untouched")).await.unwrap();
        let gone = r.wc.open_for_edit(&r.remote, &uri("gone")).await.unwrap();
        write_local(&edited, b"one!", 4000);
        std::fs::remove_file(&gone.local_path).unwrap();
        let dirty = r.wc.scan_at_start().await.unwrap();
        assert_eq!(dirty.iter().map(|c| c.id).collect::<Vec<_>>(), vec![edited.id]);
        assert!(r.wc.get(gone.id).await.is_err(), "vanished files are dropped");
    }

    #[tokio::test]
    async fn expiry_removes_after_24_hours_unless_pinned_or_dirty() {
        let r = rig();
        for n in ["a", "b", "c", "d"] {
            r.remote.add_file(n, b"x", 1000);
        }
        let a = r.wc.open_for_edit(&r.remote, &uri("a")).await.unwrap();
        let b = r.wc.open_for_edit(&r.remote, &uri("b")).await.unwrap();
        let c = r.wc.open_for_edit(&r.remote, &uri("c")).await.unwrap();
        let d = r.wc.open_for_view(&r.remote, &uri("d")).await.unwrap();
        r.wc.pin(b.id, true).await.unwrap();
        write_local(&c, b"unsent edit", 4000);
        r.clock.advance(EXPIRY_MS - 1);
        assert!(r.wc.expire().await.unwrap().is_empty());
        r.clock.advance(1);
        let gone: Vec<WorkingCopyId> = r.wc.expire().await.unwrap().iter().map(|c| c.id).collect();
        assert_eq!(gone, vec![a.id, d.id]);
        assert!(!a.local_path.exists() && !d.local_path.exists());
        assert!(b.local_path.exists() && c.local_path.exists());
        assert!(!a.local_path.parent().unwrap().exists());
        let opened = AppPaths::new(r.home.path()).opened_dir();
        assert_eq!(d.local_path.parent().unwrap().parent().unwrap(), opened);
        r.wc.pin(b.id, false).await.unwrap();
        assert_eq!(r.wc.expire().await.unwrap().len(), 1);
        assert_eq!(r.wc.list(None).await.unwrap().len(), 1, "the dirty copy stays");
        assert_eq!(r.wc.pin(9999, true).await.unwrap_err().kind, ErrorKind::NotFound);
    }

    #[tokio::test]
    async fn an_upload_restarts_the_24_hour_clock() {
        let r = rig();
        r.remote.add_file("a", b"x", 1000);
        let a = r.wc.open_for_edit(&r.remote, &uri("a")).await.unwrap();
        r.clock.advance(EXPIRY_MS - 10);
        write_local(&a, b"xy", 4000);
        r.wc.upload(&r.remote, a.id).await.unwrap();
        r.clock.advance(20);
        assert!(r.wc.expire().await.unwrap().is_empty());
        r.clock.advance(EXPIRY_MS);
        assert_eq!(r.wc.expire().await.unwrap().len(), 1);
    }

    #[tokio::test]
    async fn opened_copies_refresh_and_upgrade() {
        let r = rig();
        r.remote.add_file("pic.jpg", b"v1", 1000);
        let v = r.wc.open_for_view(&r.remote, &uri("pic.jpg")).await.unwrap();
        assert_eq!(v.purpose, Purpose::Open);
        assert_eq!(
            v.local_path.parent().unwrap().parent().unwrap(),
            AppPaths::new(r.home.path()).opened_dir()
        );
        r.remote.add_file("pic.jpg", b"version two", 2000);
        let again = r.wc.open_for_view(&r.remote, &uri("pic.jpg")).await.unwrap();
        assert_eq!(again.id, v.id);
        assert_eq!(std::fs::read(&again.local_path).unwrap(), b"version two");
        assert_eq!(again.base_size, Some(11));
        let edit = r.wc.open_for_edit(&r.remote, &uri("pic.jpg")).await.unwrap();
        assert_eq!((edit.id, edit.purpose), (v.id, Purpose::Edit));
        let e = r.wc.on_local_change(&r.remote, v.id).await;
        assert_eq!(e.unwrap(), Decision::Unchanged);
    }

    #[tokio::test]
    async fn only_edit_copies_are_written_back() {
        let r = rig();
        r.remote.add_file("f", b"x", 1);
        let v = r.wc.open_for_view(&r.remote, &uri("f")).await.unwrap();
        let e = r.wc.on_local_change(&r.remote, v.id).await.unwrap_err();
        assert_eq!(e.kind, ErrorKind::InvalidArgument);
    }

    #[tokio::test]
    async fn a_stale_row_without_its_file_is_replaced() {
        let r = rig();
        r.remote.add_file("f", b"x", 1);
        let c = r.wc.open_for_edit(&r.remote, &uri("f")).await.unwrap();
        std::fs::remove_dir_all(c.local_path.parent().unwrap()).unwrap();
        let fresh = r.wc.open_for_edit(&r.remote, &uri("f")).await.unwrap();
        assert!(fresh.local_path.exists(), "downloaded again");
        assert_eq!(r.wc.list(None).await.unwrap(), vec![fresh]);
    }

    #[tokio::test]
    async fn remove_deletes_everything() {
        let r = rig();
        r.remote.add_file("f", b"x", 1);
        let c = r.wc.open_for_edit(&r.remote, &uri("f")).await.unwrap();
        r.wc.remove(c.id).await.unwrap();
        assert!(!c.local_path.exists());
        assert_eq!(r.wc.get(c.id).await.unwrap_err().kind, ErrorKind::NotFound);
    }
}
