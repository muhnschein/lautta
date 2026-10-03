// SPDX-License-Identifier: LGPL-2.1-or-later
//! The provider for local folders (SPEC §3.3, §10.1, XFR-3). A `VPath` maps
//! under a root directory; it can never contain `..`, so it cannot leave the
//! root lexically. Symlinks are not rewritten: one that points outside the
//! sandbox shows up as an entry flagged `NOT_ACCESSIBLE` (BRW-10).
//!
//! Blocking calls run on `spawn_blocking`; heavy ones (listing, copying,
//! hashing) first take a permit from a semaphore that all local providers
//! share (ARC-6, 4 permits).

mod copy;
mod entries;
mod fskind;
mod names;

pub use entries::{is_readonly, lexical_normalize};
pub use fskind::{fs_kind, FsKind};
pub use names::Names;

use super::{
    AttributeChanges, CopyOptions, Disposition, Lane, ProgressSink, Provider, ReadHandle, ReadOptions,
    RenameMode, SpaceInfo, WriteOptions,
};
use crate::entry::{cap, Capabilities, Entry};
use crate::error::{Error, ErrorKind, Result};
use crate::sys;
use crate::vpath::VPath;
use async_trait::async_trait;
use entries::EntryBuilder;
use std::ffi::OsStr;
use std::fs::{File, OpenOptions};
use std::io::{Read, Seek, SeekFrom};
use std::os::fd::OwnedFd;
use std::os::unix::ffi::{OsStrExt, OsStringExt};
use std::os::unix::fs::{FileExt, OpenOptionsExt, PermissionsExt};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;
use tokio::sync::{mpsc, OwnedSemaphorePermit, Semaphore};

/// Entries per listing batch (BRW-8).
pub const LIST_BATCH: usize = 256;
/// Concurrent heavy blocking tasks across all local providers (ARC-6).
pub const IO_PERMITS: usize = 4;

/// A fresh semaphore with [`IO_PERMITS`] permits, to share between providers.
pub fn new_io_semaphore() -> Arc<Semaphore> {
    Arc::new(Semaphore::new(IO_PERMITS))
}

#[derive(Clone)]
pub struct LocalProvider {
    root: PathBuf,
    sem: Arc<Semaphore>,
    caps: Capabilities,
    builder: Arc<EntryBuilder>,
    batch: usize,
}

impl LocalProvider {
    /// Capabilities come from the filesystem under `root` (`statfs`), minus
    /// `Write` on a read-only mount.
    pub fn new(root: PathBuf) -> LocalProvider {
        let mut caps = fs_kind(&root).capabilities();
        if sys::statvfs(&root).map(|v| v.read_only).unwrap_or(false) {
            caps.raw.remove(cap::WRITE);
        }
        let builder = Arc::new(EntryBuilder::new(root.clone(), Arc::new(Names::default())));
        LocalProvider {
            root,
            sem: new_io_semaphore(),
            caps,
            builder,
            batch: LIST_BATCH,
        }
    }

    /// Shares the process-wide blocking-task semaphore (ARC-6).
    pub fn with_semaphore(mut self, sem: Arc<Semaphore>) -> LocalProvider {
        self.sem = sem;
        self
    }

    /// Adds a capability flag, for example [`cap::TRASH`] on the home filesystem.
    pub fn with_capability(mut self, flag: &str) -> LocalProvider {
        self.caps.raw.insert(flag.to_owned());
        self
    }

    /// Replaces the capabilities (tests, forced policies).
    pub fn with_capabilities(mut self, caps: Capabilities) -> LocalProvider {
        self.caps = caps;
        self
    }

    pub fn with_names(mut self, names: Arc<Names>) -> LocalProvider {
        self.builder = Arc::new(EntryBuilder::new(self.root.clone(), names));
        self
    }

    /// Listing batch size (tests).
    pub fn with_batch(mut self, batch: usize) -> LocalProvider {
        self.batch = batch.max(1);
        self
    }

    pub fn root(&self) -> &Path {
        &self.root
    }

    /// The real path of `path` under the root.
    pub fn real_path(&self, path: &VPath) -> PathBuf {
        if path.is_root() {
            self.root.clone()
        } else {
            self.root.join(OsStr::from_bytes(path.as_bytes()))
        }
    }

    fn require_write(&self) -> Result<()> {
        if self.caps.writable() {
            Ok(())
        } else {
            Err(Error::new(ErrorKind::ReadOnlyFilesystem, "read-only location"))
        }
    }

    fn require_cap(&self, flag: &str) -> Result<()> {
        if self.caps.has(flag) {
            Ok(())
        } else {
            Err(Error::new(
                ErrorKind::Unsupported,
                format!("{flag} is not available here"),
            ))
        }
    }

    async fn permit(&self) -> Result<OwnedSemaphorePermit> {
        self.sem
            .clone()
            .acquire_owned()
            .await
            .map_err(|_| Error::new(ErrorKind::Internal, "io semaphore closed"))
    }
}

/// Runs blocking file system work off the async threads (ARC-6).
async fn blocking<T, F>(f: F) -> Result<T>
where
    T: Send + 'static,
    F: FnOnce() -> Result<T> + Send + 'static,
{
    tokio::task::spawn_blocking(f)
        .await
        .map_err(|e| Error::new(ErrorKind::Internal, format!("blocking task failed: {e}")))?
}

fn list_blocking(
    real: &Path,
    builder: &EntryBuilder,
    batch: usize,
    out: &mpsc::Sender<Vec<Entry>>,
) -> Result<()> {
    let mut buf = Vec::with_capacity(batch);
    for item in std::fs::read_dir(real)? {
        if let Some(entry) = builder.entry_from_dirent(&item?) {
            buf.push(entry);
        }
        if buf.len() >= batch {
            send_batch(out, std::mem::take(&mut buf))?;
        }
    }
    if buf.is_empty() {
        Ok(())
    } else {
        send_batch(out, buf)
    }
}

fn send_batch(out: &mpsc::Sender<Vec<Entry>>, batch: Vec<Entry>) -> Result<()> {
    out.blocking_send(batch)
        .map_err(|_| Error::kind(ErrorKind::Canceled))
}

/// A hidden sibling name for intermediate steps (case-only rename, copy temp).
fn unique_sibling(path: &Path) -> PathBuf {
    static COUNTER: AtomicU64 = AtomicU64::new(0);
    let n = COUNTER.fetch_add(1, Ordering::Relaxed);
    let name = format!(".lautta-tmp-{}-{n}", std::process::id());
    path.with_file_name(name)
}

/// Lower-cases for the case-only comparison: Unicode when valid UTF-8, ASCII
/// otherwise.
fn fold_case(name: &OsStr) -> Vec<u8> {
    match name.to_str() {
        Some(s) => s.to_lowercase().into_bytes(),
        None => name.as_bytes().to_ascii_lowercase(),
    }
}

/// True when `to` is `from` in the same folder with different letter case.
fn is_case_only_change(from: &Path, to: &Path) -> bool {
    match (from.file_name(), to.file_name()) {
        (Some(a), Some(b)) => from.parent() == to.parent() && a != b && fold_case(a) == fold_case(b),
        _ => false,
    }
}

/// Renames without replacing: `renameat2(RENAME_NOREPLACE)` where the
/// filesystem has it, otherwise [`rename_noreplace_fallback`].
fn rename_noreplace_compat(from: &Path, to: &Path) -> Result<()> {
    match sys::rename_noreplace(from, to) {
        Ok(()) => Ok(()),
        Err(e) if sys::is_flag_unsupported(&e) => rename_noreplace_fallback(from, to),
        Err(e) => Err(e.into()),
    }
}

/// For filesystems without `RENAME_NOREPLACE`: link + unlink is atomic with
/// respect to the destination (the link fails if it exists); directories and
/// filesystems without hard links get a plain exists-check.
fn rename_noreplace_fallback(from: &Path, to: &Path) -> Result<()> {
    if !sys::stat(from, false)?.is_dir() {
        match std::fs::hard_link(from, to) {
            Ok(()) => return unlink_source(from, to),
            Err(e) if e.kind() == std::io::ErrorKind::AlreadyExists => return Err(e.into()),
            Err(_) => {}
        }
    }
    if sys::stat(to, false).is_ok() {
        return Err(Error::kind(ErrorKind::AlreadyExists));
    }
    std::fs::rename(from, to).map_err(Into::into)
}

fn unlink_source(from: &Path, to: &Path) -> Result<()> {
    if let Err(e) = std::fs::remove_file(from) {
        // Leave exactly one name behind.
        let _ = std::fs::remove_file(to);
        return Err(e.into());
    }
    Ok(())
}

/// OPS-3: on a case-insensitive filesystem `to` resolves to `from` itself, so
/// the move goes through an intermediate name.
fn rename_case_only(from: &Path, to: &Path, mode: RenameMode) -> Result<()> {
    if let Ok(existing) = sys::stat(to, false) {
        let me = sys::stat(from, false)?;
        if (existing.dev, existing.ino) != (me.dev, me.ino) {
            // A different file already has the target name.
            return match mode {
                RenameMode::NoReplace => Err(Error::kind(ErrorKind::AlreadyExists)),
                RenameMode::Replace => std::fs::rename(from, to).map_err(Into::into),
            };
        }
    }
    let tmp = unique_sibling(from);
    rename_noreplace_compat(from, &tmp)?;
    if let Err(e) = rename_noreplace_compat(&tmp, to) {
        let _ = std::fs::rename(&tmp, from);
        return Err(e);
    }
    Ok(())
}

fn rename_blocking(from: &Path, to: &Path, mode: RenameMode, case_insensitive: bool) -> Result<()> {
    if case_insensitive && is_case_only_change(from, to) {
        return rename_case_only(from, to, mode);
    }
    match mode {
        RenameMode::Replace => std::fs::rename(from, to).map_err(Into::into),
        RenameMode::NoReplace => rename_noreplace_compat(from, to),
    }
}

fn open_for_upload(dst: &Path, opts: &WriteOptions) -> std::io::Result<File> {
    let mut oo = OpenOptions::new();
    oo.write(true).mode(opts.mode.map_or(0o644, |m| m & 0o7777));
    match opts.disposition {
        Disposition::Create => oo.create_new(true),
        Disposition::Truncate => oo.create(true).truncate(true),
        Disposition::Resume => oo.create(true),
    };
    oo.open(dst)
}

/// Positions the destination for a resume (XFR-12): the partial file must be
/// at least `offset` long; anything beyond is discarded.
fn position_for_resume(dst: &mut File, offset: u64) -> Result<()> {
    if dst.metadata()?.len() < offset {
        return Err(Error::new(
            ErrorKind::InvalidArgument,
            "partial file is shorter than the resume offset",
        ));
    }
    dst.set_len(offset)?;
    dst.seek(SeekFrom::Start(offset))?;
    Ok(())
}

fn upload_blocking(src: OwnedFd, dst: &Path, opts: &WriteOptions, progress: &ProgressSink) -> Result<()> {
    let mut srcf = File::from(src);
    let mut dstf = open_for_upload(dst, opts)?;
    let base = if opts.disposition == Disposition::Resume {
        position_for_resume(&mut dstf, opts.offset)?;
        opts.offset
    } else {
        0
    };
    if opts.offset > 0 {
        srcf.seek(SeekFrom::Start(opts.offset))?;
    }
    let total = opts.size.or_else(|| {
        let meta = srcf.metadata().ok()?;
        meta.is_file()
            .then(|| meta.len().saturating_sub(opts.offset) + base)
    });
    copy::copy_fd(&srcf, &dstf, base, total, base == 0 && opts.offset == 0, progress)?;
    if let Some(t) = opts.modified {
        dstf.set_modified(t)?;
    }
    Ok(())
}

fn download_blocking(src: &Path, dst: OwnedFd, offset: u64, progress: &ProgressSink) -> Result<()> {
    let mut srcf = File::open(src)?;
    let meta = srcf.metadata()?;
    if meta.is_dir() {
        return Err(Error::kind(ErrorKind::IsADirectory));
    }
    let mut dstf = File::from(dst);
    if offset > 0 {
        srcf.seek(SeekFrom::Start(offset))?;
        dstf.seek(SeekFrom::Start(offset))?;
    }
    copy::copy_fd(&srcf, &dstf, offset, Some(meta.len()), offset == 0, progress)?;
    Ok(())
}

/// Server-side copy of one file or symlink: written to a temporary sibling
/// and renamed into place, so a failure never leaves a partial destination.
fn copy_blocking(from: &Path, to: &Path, opts: &CopyOptions) -> Result<()> {
    let st = sys::stat(from, false)?;
    if st.is_dir() {
        return Err(Error::kind(ErrorKind::IsADirectory));
    }
    if !st.is_file() && !st.is_symlink() {
        return Err(Error::new(ErrorKind::Unsupported, "not a regular file"));
    }
    if !opts.replace && sys::stat(to, false).is_ok() {
        return Err(Error::kind(ErrorKind::AlreadyExists));
    }
    let tmp = unique_sibling(to);
    let result = write_copy(from, &tmp, &st, opts).and_then(|()| {
        if opts.replace {
            std::fs::rename(&tmp, to).map_err(Into::into)
        } else {
            rename_noreplace_compat(&tmp, to)
        }
    });
    if result.is_err() {
        let _ = std::fs::remove_file(&tmp);
    }
    result
}

fn write_copy(from: &Path, tmp: &Path, st: &sys::FileStat, opts: &CopyOptions) -> Result<()> {
    if st.is_symlink() {
        std::os::unix::fs::symlink(std::fs::read_link(from)?, tmp)?;
        return Ok(());
    }
    let src = File::open(from)?;
    let dst = OpenOptions::new()
        .write(true)
        .create_new(true)
        .mode(st.permissions() & 0o777)
        .open(tmp)?;
    copy::copy_fd(&src, &dst, 0, Some(st.size), true, &super::no_progress())?;
    if opts.preserve_mtime {
        dst.set_modified(st.mtime)?;
    }
    Ok(())
}

fn hash_file(path: &Path, algorithm: &str) -> Result<Vec<u8>> {
    let file = File::open(path)?;
    if file.metadata()?.is_dir() {
        return Err(Error::kind(ErrorKind::IsADirectory));
    }
    match algorithm {
        "sha256" => Ok(hash_reader::<sha2::Sha256>(file)?),
        "sha1" => Ok(hash_reader::<sha1::Sha1>(file)?),
        "md5" => Ok(hash_reader::<md5::Md5>(file)?),
        _ => Err(Error::kind(ErrorKind::Unsupported)),
    }
}

fn hash_reader<D: sha2::Digest>(mut file: File) -> std::io::Result<Vec<u8>> {
    let mut hasher = D::new();
    let mut buf = vec![0u8; 1 << 20];
    loop {
        match file.read(&mut buf) {
            Ok(0) => return Ok(hasher.finalize().to_vec()),
            Ok(n) => hasher.update(&buf[..n]),
            Err(e) if e.kind() == std::io::ErrorKind::Interrupted => {}
            Err(e) => return Err(e),
        }
    }
}

/// "SHA-256", "sha256" and "Sha256" all name the same algorithm.
fn normalise_algorithm(name: &str) -> String {
    name.chars()
        .filter(|c| *c != '-')
        .flat_map(char::to_lowercase)
        .collect()
}

struct LocalReader {
    file: Arc<File>,
    size: u64,
}

#[async_trait]
impl ReadHandle for LocalReader {
    fn size(&self) -> Option<u64> {
        Some(self.size)
    }

    async fn read_at(&self, offset: u64, max: usize) -> Result<Vec<u8>> {
        let file = self.file.clone();
        blocking(move || {
            let mut buf = vec![0u8; max.min(16 << 20)];
            loop {
                match file.read_at(&mut buf, offset) {
                    Ok(n) => {
                        buf.truncate(n);
                        return Ok(buf);
                    }
                    Err(e) if e.kind() == std::io::ErrorKind::Interrupted => {}
                    Err(e) => return Err(e.into()),
                }
            }
        })
        .await
    }

    async fn read_ahead(&self, offset: u64, bytes: u64) -> Result<()> {
        sys::advise_willneed(&self.file, offset, bytes);
        Ok(())
    }
}

#[async_trait]
impl Provider for LocalProvider {
    fn capabilities(&self) -> Capabilities {
        self.caps.clone()
    }

    async fn list(&self, dir: &VPath, _lane: Lane, out: mpsc::Sender<Vec<Entry>>) -> Result<()> {
        let real = self.real_path(dir);
        let permit = self.permit().await?;
        let (builder, batch) = (self.builder.clone(), self.batch);
        blocking(move || {
            let _permit = permit;
            list_blocking(&real, &builder, batch, &out)
        })
        .await
    }

    async fn stat(&self, path: &VPath, follow: bool, _lane: Lane) -> Result<Entry> {
        let real = self.real_path(path);
        let name = path.name().unwrap_or(b"").to_vec();
        let builder = self.builder.clone();
        blocking(move || Ok(builder.stat_entry(&real, &name, follow)?)).await
    }

    async fn read_link(&self, path: &VPath) -> Result<Vec<u8>> {
        let real = self.real_path(path);
        blocking(move || Ok(std::fs::read_link(real)?.into_os_string().into_vec())).await
    }

    async fn make_dir(&self, path: &VPath, exclusive: bool) -> Result<()> {
        self.require_write()?;
        let real = self.real_path(path);
        blocking(move || match std::fs::create_dir(&real) {
            Err(e) if e.kind() == std::io::ErrorKind::AlreadyExists && !exclusive && real.is_dir() => Ok(()),
            other => Ok(other?),
        })
        .await
    }

    async fn make_file(&self, path: &VPath) -> Result<()> {
        self.require_write()?;
        let real = self.real_path(path);
        blocking(move || {
            OpenOptions::new().write(true).create_new(true).open(real)?;
            Ok(())
        })
        .await
    }

    async fn remove_file(&self, path: &VPath) -> Result<()> {
        self.require_write()?;
        if path.is_root() {
            return Err(Error::kind(ErrorKind::IsADirectory));
        }
        let real = self.real_path(path);
        blocking(move || Ok(std::fs::remove_file(real)?)).await
    }

    async fn remove_dir(&self, path: &VPath) -> Result<()> {
        self.require_write()?;
        if path.is_root() {
            return Err(Error::kind(ErrorKind::PermissionDenied));
        }
        let real = self.real_path(path);
        blocking(move || Ok(std::fs::remove_dir(real)?)).await
    }

    async fn rename(&self, from: &VPath, to: &VPath, mode: RenameMode) -> Result<()> {
        self.require_write()?;
        if from.is_root() || to.is_root() {
            return Err(Error::kind(ErrorKind::InvalidArgument));
        }
        let (a, b) = (self.real_path(from), self.real_path(to));
        let ci = self.caps.has(cap::CASE_INSENSITIVE);
        blocking(move || rename_blocking(&a, &b, mode, ci)).await
    }

    async fn set_attributes(&self, path: &VPath, changes: AttributeChanges) -> Result<()> {
        self.require_write()?;
        if changes.mode.is_some() {
            self.require_cap(cap::PERMISSIONS)?;
        }
        if changes.modified.is_some() {
            self.require_cap(cap::SET_MTIME)?;
        }
        let real = self.real_path(path);
        blocking(move || {
            if sys::stat(&real, false)?.is_symlink() {
                return Err(Error::kind(ErrorKind::Unsupported));
            }
            if let Some(mode) = changes.mode {
                std::fs::set_permissions(&real, std::fs::Permissions::from_mode(mode & 0o7777))?;
            }
            if let Some(t) = changes.modified {
                sys::set_mtime(&real, t)?;
            }
            Ok(())
        })
        .await
    }

    async fn make_symlink(&self, target: &[u8], link: &VPath) -> Result<()> {
        self.require_write()?;
        self.require_cap(cap::SYMLINKS)?;
        if target.is_empty() {
            return Err(Error::kind(ErrorKind::InvalidArgument));
        }
        let target = OsStr::from_bytes(target).to_owned();
        let real = self.real_path(link);
        blocking(move || Ok(std::os::unix::fs::symlink(target, real)?)).await
    }

    async fn make_hardlink(&self, existing: &VPath, new_path: &VPath) -> Result<()> {
        self.require_write()?;
        self.require_cap(cap::HARDLINKS)?;
        let (a, b) = (self.real_path(existing), self.real_path(new_path));
        blocking(move || Ok(std::fs::hard_link(a, b)?)).await
    }

    async fn open_read(&self, path: &VPath, _lane: Lane) -> Result<Box<dyn ReadHandle>> {
        let real = self.real_path(path);
        blocking(move || {
            let file = File::open(real)?;
            let meta = file.metadata()?;
            if meta.is_dir() {
                return Err(Error::kind(ErrorKind::IsADirectory));
            }
            Ok(Box::new(LocalReader {
                file: Arc::new(file),
                size: meta.len(),
            }) as Box<dyn ReadHandle>)
        })
        .await
    }

    async fn upload_from(
        &self,
        src: OwnedFd,
        dst: &VPath,
        opts: WriteOptions,
        progress: ProgressSink,
    ) -> Result<()> {
        self.require_write()?;
        let real = self.real_path(dst);
        let permit = self.permit().await?;
        blocking(move || {
            let _permit = permit;
            upload_blocking(src, &real, &opts, &progress)
        })
        .await
    }

    async fn download_into(
        &self,
        src: &VPath,
        dst: OwnedFd,
        opts: ReadOptions,
        progress: ProgressSink,
    ) -> Result<()> {
        let real = self.real_path(src);
        let permit = self.permit().await?;
        blocking(move || {
            let _permit = permit;
            download_blocking(&real, dst, opts.offset, &progress)
        })
        .await
    }

    async fn server_copy(&self, from: &VPath, to: &VPath, opts: CopyOptions) -> Result<()> {
        self.require_write()?;
        if from == to || from.is_root() || to.is_root() {
            return Err(Error::kind(ErrorKind::InvalidArgument));
        }
        let (a, b) = (self.real_path(from), self.real_path(to));
        let permit = self.permit().await?;
        blocking(move || {
            let _permit = permit;
            copy_blocking(&a, &b, &opts)
        })
        .await
    }

    async fn checksum(&self, path: &VPath, algorithm: &str) -> Result<Vec<u8>> {
        let algorithm = normalise_algorithm(algorithm);
        if !matches!(algorithm.as_str(), "sha256" | "sha1" | "md5") {
            return Err(Error::kind(ErrorKind::Unsupported));
        }
        let real = self.real_path(path);
        let permit = self.permit().await?;
        blocking(move || {
            let _permit = permit;
            hash_file(&real, &algorithm)
        })
        .await
    }

    async fn space(&self, dir: &VPath) -> Result<SpaceInfo> {
        let real = self.real_path(dir);
        blocking(move || {
            let v = sys::statvfs(&real)?;
            Ok(SpaceInfo {
                free: v.free,
                total: v.total,
                used: v.used,
            })
        })
        .await
    }
}

#[cfg(test)]
mod tests;
