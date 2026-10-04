// SPDX-License-Identifier: LGPL-2.1-or-later
//! Executes one plan item: chooses the data path (XFR-3), moves the bytes
//! through a scratch fd (NVB-9), names temporaries (NVB-11), resumes
//! (XFR-12), verifies (XFR-4) and finishes moves (OPS-4).
//!
//! Nothing here touches queue state; the engine feeds in an [`ItemEnv`] and a
//! [`ItemJob`] and acts on the [`Outcome`].

use super::conflict::{build_conflict, decide, numbered_name, Decision, WriteMode};
use super::model::TransferOptions;
use super::Trasher;
use crate::entry::{cap, ms_to_system_time, Capabilities, Entry, Kind};
use crate::error::{Error, ErrorKind, Result};
use crate::ops::{Conflict, OperationKind, PlanItem};
use crate::provider::{
    list_all, AttributeChanges, CopyOptions, Disposition, Lane, ProgressSink, Provider, ReadOptions,
    RenameMode, WriteOptions,
};
use crate::uri::Uri;
use crate::vpath::VPath;
use std::fs::File;
use std::os::fd::OwnedFd;
use std::path::PathBuf;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;

/// Longest temp name prefix taken from the original name; names are limited
/// to 255 bytes on most file systems and the suffix needs room.
const MAX_TEMP_STEM: usize = 200;

/// Everything an item needs from the outside.
#[derive(Clone)]
pub struct ItemEnv {
    pub src: Arc<dyn Provider>,
    pub dst: Arc<dyn Provider>,
    pub same_location: bool,
    pub op: OperationKind,
    pub options: TransferOptions,
    /// Where anonymous scratch files live.
    pub scratch_dir: PathBuf,
    pub trasher: Option<Arc<dyn Trasher>>,
    /// Bytes of this item done so far.
    pub progress: ProgressSink,
}

#[derive(Debug, Clone)]
pub struct ItemJob {
    pub plan: PlanItem,
    /// Name of the destination temporary (NVB-11), fixed before the first
    /// attempt so a later run finds the partial file.
    pub temp_name: Vec<u8>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Outcome {
    Done,
    Skipped,
    /// The item and everything below it is finished (renamed folder,
    /// trashed or recursively deleted subtree).
    Subtree,
    /// A conflict needs an answer.
    Ask(Conflict),
    /// *Keep both*: use this destination instead and run again.
    KeepBoth(Uri),
}

/// `.<name>.lautta-<shortid>.part` (NVB-11). The short id is derived from the
/// transfer and item so the same item always finds its partial file.
pub fn temp_name_for(name: &[u8], transfer: i64, seq: u32) -> Vec<u8> {
    let t = u32::try_from(transfer & 0xffff_ffff).unwrap_or(0);
    let short = t.wrapping_mul(0x9E37_79B1) ^ seq;
    let stem = &name[..name.len().min(MAX_TEMP_STEM)];
    let mut out = vec![b'.'];
    out.extend_from_slice(stem);
    out.extend_from_slice(format!(".lautta-{short:08x}.part").as_bytes());
    out
}

/// The checksum algorithm both sides can compute, if any (XFR-4). A provider
/// that advertises `Checksums` without listing algorithms is assumed to do
/// SHA-256.
pub fn common_algorithm(a: &Capabilities, b: &Capabilities) -> Option<String> {
    const PREFERRED: [&str; 3] = ["sha256", "sha1", "md5"];
    if !a.has(cap::CHECKSUMS) || !b.has(cap::CHECKSUMS) {
        return None;
    }
    let (la, lb) = (&a.checksum_algorithms, &b.checksum_algorithms);
    if la.is_empty() || lb.is_empty() {
        return Some("sha256".to_owned());
    }
    PREFERRED
        .iter()
        .map(|p| (*p).to_owned())
        .find(|p| la.contains(p) && lb.contains(p))
        .or_else(|| la.iter().find(|x| lb.contains(x)).cloned())
}

pub async fn run_item(env: &ItemEnv, job: &ItemJob) -> Result<Outcome> {
    if env.op == OperationKind::Delete {
        return run_delete(env, job).await;
    }
    match job.plan.kind {
        Kind::Dir => run_dir(env, job).await,
        Kind::Symlink => run_symlink(env, job).await,
        Kind::File => run_file(env, job).await,
        _ => Err(Error::new(
            ErrorKind::Unsupported,
            "special files cannot be copied",
        )),
    }
}

async fn stat_opt(p: &dyn Provider, path: &VPath) -> Result<Option<Entry>> {
    match p.stat(path, false, Lane::Bulk).await {
        Ok(e) => Ok(Some(e)),
        Err(e) if e.kind == ErrorKind::NotFound => Ok(None),
        Err(e) => Err(e),
    }
}

async fn remove_if_exists(p: &dyn Provider, path: &VPath) -> Result<()> {
    match p.remove_file(path).await {
        Err(e) if e.kind != ErrorKind::NotFound => Err(e),
        _ => Ok(()),
    }
}

fn is_move(env: &ItemEnv) -> bool {
    env.op == OperationKind::Move
}

fn rename_mode(mode: WriteMode) -> RenameMode {
    if mode.replace {
        RenameMode::Replace
    } else {
        RenameMode::NoReplace
    }
}

/// Rename failures that only mean "not possible here": fall back to copying.
fn rename_unavailable(e: &Error) -> bool {
    matches!(e.kind, ErrorKind::CrossesDevice | ErrorKind::Unsupported)
}

fn temp_path(dst: &Uri, temp_name: &[u8]) -> Result<VPath> {
    dst.path
        .parent()
        .ok_or_else(|| Error::new(ErrorKind::InvalidArgument, "destination has no parent"))?
        .join(temp_name)
}

/// Removes a leftover temporary (best effort).
pub async fn cleanup_temp(dst: &dyn Provider, job: &ItemJob) {
    if let Ok(p) = temp_path(&job.plan.dst, &job.temp_name) {
        if let Err(e) = remove_if_exists(dst, &p).await {
            log::debug!("could not remove temporary: {e}");
        }
    }
}

// ---------------------------------------------------------------- files

async fn run_file(env: &ItemEnv, job: &ItemJob) -> Result<Outcome> {
    let plan = &job.plan;
    let can_resume = env.dst.capabilities().has(cap::RESUME_UPLOAD);
    let existing = stat_opt(&*env.dst, &plan.dst.path).await?;
    let mode = match decide(plan, existing.as_ref(), env.options.resolve_all, can_resume) {
        Decision::Proceed(m) => m,
        Decision::Skip => return Ok(Outcome::Skipped),
        Decision::KeepBoth => return keep_both(env, plan).await,
        Decision::Ask(c) => return Ok(Outcome::Ask(c)),
    };
    match place_file(env, job, mode).await {
        Err(e) if e.kind == ErrorKind::AlreadyExists && !mode.replace => {
            match stat_opt(&*env.dst, &plan.dst.path).await? {
                Some(d) => Ok(Outcome::Ask(build_conflict(plan, &d, can_resume))),
                None => Err(e),
            }
        }
        other => other.map(|()| Outcome::Done),
    }
}

async fn place_file(env: &ItemEnv, job: &ItemJob, mode: WriteMode) -> Result<()> {
    let plan = &job.plan;
    let src_entry = env.src.stat(&plan.src.path, true, Lane::Bulk).await?;
    if src_entry.is_dir() {
        return Err(Error::kind(ErrorKind::IsADirectory));
    }
    if env.same_location && is_move(env) && !mode.resume {
        match env
            .dst
            .rename(&plan.src.path, &plan.dst.path, rename_mode(mode))
            .await
        {
            Ok(()) => return Ok(()),
            Err(e) if rename_unavailable(&e) => {}
            Err(e) => return Err(e),
        }
    }
    let caps = env.dst.capabilities();
    // AtomicPut backends are written directly; so is an in-place resume.
    let temp_used = !(caps.has(cap::ATOMIC_PUT) || mode.resume);
    let write_path = if temp_used {
        temp_path(&plan.dst, &job.temp_name)?
    } else {
        plan.dst.path.clone()
    };
    let total = src_entry.size;
    if env.same_location && caps.has(cap::SERVER_COPY) && !mode.resume {
        server_copy_to(env, plan, &write_path, temp_used, mode).await?;
    } else {
        let offset = committed_offset(env, &write_path, total, mode, temp_used).await?;
        stream_to(env, job, &write_path, offset, total, mode, &src_entry).await?;
    }
    finish_placement(env, job, &write_path, total, mode, temp_used, &src_entry).await?;
    if is_move(env) {
        remove_if_exists(&*env.src, &plan.src.path).await?;
    }
    Ok(())
}

async fn server_copy_to(
    env: &ItemEnv,
    plan: &PlanItem,
    write_path: &VPath,
    temp_used: bool,
    mode: WriteMode,
) -> Result<()> {
    if temp_used {
        remove_if_exists(&*env.dst, write_path).await?;
    }
    env.dst
        .server_copy(
            &plan.src.path,
            write_path,
            CopyOptions {
                replace: !temp_used && mode.replace,
                preserve_mtime: env.options.preserve_mtime,
            },
        )
        .await
}

/// XFR-12: the committed offset is the size of the partial file at the
/// destination. Providers without `ResumeUpload` restart the file.
async fn committed_offset(
    env: &ItemEnv,
    write_path: &VPath,
    total: Option<u64>,
    mode: WriteMode,
    temp_used: bool,
) -> Result<u64> {
    if !temp_used && !mode.resume {
        return Ok(0);
    }
    let Some(existing) = stat_opt(&*env.dst, write_path).await? else {
        return Ok(0);
    };
    if existing.kind != Kind::File {
        return Err(Error::kind(ErrorKind::AlreadyExists));
    }
    let size = existing.size.unwrap_or(0);
    let can = env.dst.capabilities().has(cap::RESUME_UPLOAD) && size > 0 && total.is_some_and(|t| size <= t);
    if can {
        return Ok(size);
    }
    if mode.resume {
        return Err(Error::new(
            ErrorKind::InvalidArgument,
            "the partial file cannot be resumed",
        ));
    }
    env.dst.remove_file(write_path).await?;
    Ok(0)
}

fn clamp_to(done: u64, total: Option<u64>) -> u64 {
    total.map_or(done, |t| done.min(t))
}

/// Download and upload both count for half of the item's progress.
fn phase_sinks(report: ProgressSink, base: u64, total: Option<u64>) -> (ProgressSink, ProgressSink) {
    let down = Arc::new(AtomicU64::new(base));
    let up = Arc::new(AtomicU64::new(base));
    let make = |mine: Arc<AtomicU64>, other: Arc<AtomicU64>| -> ProgressSink {
        let report = report.clone();
        Arc::new(move |done, _| {
            mine.store(clamp_to(done.max(base), total), Ordering::Relaxed);
            let sum = mine.load(Ordering::Relaxed) + other.load(Ordering::Relaxed);
            report(sum / 2, total);
        })
    };
    (make(down.clone(), up.clone()), make(up, down))
}

async fn blocking<T: Send + 'static>(f: impl FnOnce() -> std::io::Result<T> + Send + 'static) -> Result<T> {
    tokio::task::spawn_blocking(f)
        .await
        .map_err(|e| Error::new(ErrorKind::Internal, e.to_string()))?
        .map_err(Error::from)
}

/// download_into(src -> scratch fd), then upload_from(scratch fd -> dst). A
/// scratch file rather than a pipe: providers write with `pwrite` at explicit
/// offsets (NVB-9), which a pipe cannot take, and resuming needs a seekable
/// source.
async fn stream_to(
    env: &ItemEnv,
    job: &ItemJob,
    write_path: &VPath,
    offset: u64,
    total: Option<u64>,
    mode: WriteMode,
    src_entry: &Entry,
) -> Result<()> {
    (env.progress)(offset, total);
    if offset > 0 && total == Some(offset) {
        return Ok(());
    }
    let plan = &job.plan;
    let dir = env.scratch_dir.clone();
    let scratch = blocking(move || {
        std::fs::create_dir_all(&dir)?; // NOSONAR: runs on the blocking pool
        tempfile::tempfile_in(dir)
    })
    .await?;
    let (down, up) = phase_sinks(env.progress.clone(), offset, total);
    let fd = OwnedFd::from(scratch.try_clone().map_err(Error::from)?);
    env.src
        .download_into(&plan.src.path, fd, ReadOptions { offset }, down)
        .await?;
    let len = file_len(&scratch).await?;
    if total.is_some_and(|t| t != len) {
        return Err(Error::new(
            ErrorKind::Io,
            "the source changed or ended early while reading",
        ));
    }
    let disposition = if offset > 0 {
        Disposition::Resume
    } else if mode.replace && !write_path_is_temp(job, write_path) {
        Disposition::Truncate
    } else {
        Disposition::Create
    };
    let opts = WriteOptions {
        disposition,
        offset,
        size: Some(len),
        modified: wanted_mtime(env, plan, src_entry),
        mode: wanted_mode(env, src_entry),
    };
    env.dst
        .upload_from(OwnedFd::from(scratch), write_path, opts, up)
        .await
}

fn write_path_is_temp(job: &ItemJob, write_path: &VPath) -> bool {
    write_path.name() == Some(job.temp_name.as_slice())
}

async fn file_len(f: &File) -> Result<u64> {
    let f = f.try_clone().map_err(Error::from)?;
    blocking(move || f.metadata().map(|m| m.len())).await // NOSONAR: runs on the blocking pool
}

fn wanted_mtime(env: &ItemEnv, plan: &PlanItem, src: &Entry) -> Option<std::time::SystemTime> {
    if !env.options.preserve_mtime {
        return None;
    }
    src.modified.or_else(|| plan.mtime_ms.map(ms_to_system_time))
}

/// OPS-5: modes only when the user enabled it and both sides support them.
fn wanted_mode(env: &ItemEnv, src: &Entry) -> Option<u32> {
    let both = env.src.capabilities().has(cap::PERMISSIONS) && env.dst.capabilities().has(cap::PERMISSIONS);
    (env.options.preserve_mode && both).then_some(src.mode).flatten()
}

fn digests_match(a: &[u8], b: &[u8]) -> Result<()> {
    if a == b {
        Ok(())
    } else {
        Err(Error::new(ErrorKind::Io, "verification failed: checksums differ"))
    }
}

/// XFR-4: compares checksums when both sides can compute one. A provider
/// that turns out not to support it just skips the check.
async fn verify_checksum(env: &ItemEnv, src: &VPath, dst: &VPath) -> Result<()> {
    let Some(alg) = common_algorithm(&env.src.capabilities(), &env.dst.capabilities()) else {
        return Ok(());
    };
    let (a, b) = tokio::join!(env.src.checksum(src, &alg), env.dst.checksum(dst, &alg));
    match (a, b) {
        (Ok(x), Ok(y)) => digests_match(&x, &y),
        (Err(e), _) | (_, Err(e)) if e.kind == ErrorKind::Unsupported => Ok(()),
        (Err(e), _) | (_, Err(e)) => Err(e),
    }
}

async fn apply_attributes(
    env: &ItemEnv,
    path: &VPath,
    got: &Entry,
    plan: &PlanItem,
    src: &Entry,
) -> Result<()> {
    let modified = wanted_mtime(env, plan, src).filter(|t| got.modified != Some(*t));
    let mode = wanted_mode(env, src).filter(|m| got.mode != Some(*m));
    if modified.is_none() && mode.is_none() {
        return Ok(());
    }
    match env
        .dst
        .set_attributes(path, AttributeChanges { mode, modified })
        .await
    {
        Err(e) if !matches!(e.kind, ErrorKind::Unsupported | ErrorKind::PermissionDenied) => Err(e),
        _ => Ok(()),
    }
}

/// Size check (always), checksum (when asked or moving), attributes, and the
/// final rename out of the temporary name.
async fn finish_placement(
    env: &ItemEnv,
    job: &ItemJob,
    write_path: &VPath,
    expected: Option<u64>,
    mode: WriteMode,
    temp_used: bool,
    src_entry: &Entry,
) -> Result<()> {
    let plan = &job.plan;
    let got = env.dst.stat(write_path, false, Lane::Bulk).await?;
    let checked = check_written(env, plan, write_path, &got, expected).await;
    if let Err(e) = checked {
        if e.kind == ErrorKind::Io && temp_used {
            cleanup_temp(&*env.dst, job).await;
        }
        return Err(e);
    }
    apply_attributes(env, write_path, &got, plan, src_entry).await?;
    if temp_used {
        env.dst
            .rename(write_path, &plan.dst.path, rename_mode(mode))
            .await?;
    }
    Ok(())
}

async fn check_written(
    env: &ItemEnv,
    plan: &PlanItem,
    write_path: &VPath,
    got: &Entry,
    expected: Option<u64>,
) -> Result<()> {
    if expected.is_some() && got.size != expected {
        return Err(Error::new(
            ErrorKind::Io,
            "verification failed: size differs after writing",
        ));
    }
    if env.options.verify_checksums || is_move(env) {
        verify_checksum(env, &plan.src.path, write_path).await?;
    }
    Ok(())
}

async fn keep_both(env: &ItemEnv, plan: &PlanItem) -> Result<Outcome> {
    let (Some(parent), Some(name)) = (plan.dst.parent(), plan.dst.name()) else {
        return Err(Error::new(ErrorKind::InvalidArgument, "destination has no name"));
    };
    let is_dir = plan.kind == Kind::Dir;
    for n in 2..1000 {
        let candidate = parent.join(&numbered_name(name, n, is_dir))?;
        if stat_opt(&*env.dst, &candidate.path).await?.is_none() {
            return Ok(Outcome::KeepBoth(candidate));
        }
    }
    Err(Error::new(ErrorKind::AlreadyExists, "no free name for the copy"))
}

// ------------------------------------------------------ folders, links

async fn run_dir(env: &ItemEnv, job: &ItemJob) -> Result<Outcome> {
    let plan = &job.plan;
    let can_resume = env.dst.capabilities().has(cap::RESUME_UPLOAD);
    let existing = stat_opt(&*env.dst, &plan.dst.path).await?;
    let mode = match decide(plan, existing.as_ref(), env.options.resolve_all, can_resume) {
        Decision::Proceed(m) => m,
        Decision::Skip => return Ok(Outcome::Skipped),
        Decision::KeepBoth => return keep_both(env, plan).await,
        Decision::Ask(c) => return Ok(Outcome::Ask(c)),
    };
    if env.same_location && is_move(env) && !mode.merge {
        match env
            .dst
            .rename(&plan.src.path, &plan.dst.path, RenameMode::NoReplace)
            .await
        {
            Ok(()) => return Ok(Outcome::Subtree),
            Err(e) if rename_unavailable(&e) => {}
            Err(e) => return Err(e),
        }
    }
    match env.dst.make_dir(&plan.dst.path, !mode.merge).await {
        Ok(()) => Ok(Outcome::Done),
        Err(e) if e.kind == ErrorKind::AlreadyExists => match stat_opt(&*env.dst, &plan.dst.path).await? {
            Some(d) => Ok(Outcome::Ask(build_conflict(plan, &d, can_resume))),
            None => Err(e),
        },
        Err(e) => Err(e),
    }
}

async fn run_symlink(env: &ItemEnv, job: &ItemJob) -> Result<Outcome> {
    let plan = &job.plan;
    let existing = stat_opt(&*env.dst, &plan.dst.path).await?;
    let mode = match decide(plan, existing.as_ref(), env.options.resolve_all, false) {
        Decision::Proceed(m) => m,
        Decision::Skip => return Ok(Outcome::Skipped),
        Decision::KeepBoth => return keep_both(env, plan).await,
        Decision::Ask(c) => return Ok(Outcome::Ask(c)),
    };
    let target = match &plan.link_target {
        Some(t) => t.clone(),
        None => env.src.read_link(&plan.src.path).await?,
    };
    if mode.replace {
        env.dst.remove_file(&plan.dst.path).await?;
    }
    env.dst.make_symlink(&target, &plan.dst.path).await?;
    if is_move(env) {
        remove_if_exists(&*env.src, &plan.src.path).await?;
    }
    Ok(Outcome::Done)
}

// --------------------------------------------------------------- deletes

async fn run_delete(env: &ItemEnv, job: &ItemJob) -> Result<Outcome> {
    let plan = &job.plan;
    if env.options.trash {
        let trashed = match &env.trasher {
            Some(t) => t.trash(&plan.src).await?,
            None => false,
        };
        if !trashed {
            remove_tree(&*env.src, &plan.src.path, plan.kind == Kind::Dir).await?;
        }
        return Ok(Outcome::Subtree);
    }
    let removed = if plan.kind == Kind::Dir {
        env.src.remove_dir(&plan.src.path).await
    } else {
        env.src.remove_file(&plan.src.path).await
    };
    match removed {
        Err(e) if e.kind != ErrorKind::NotFound => Err(e),
        _ => Ok(Outcome::Done),
    }
}

/// Deletes a folder with everything in it. Files go as they are found;
/// folders afterwards, deepest first.
pub async fn remove_tree(p: &dyn Provider, root: &VPath, is_dir: bool) -> Result<()> {
    if !is_dir {
        return remove_if_exists(p, root).await;
    }
    let mut dirs = vec![root.clone()];
    let mut next = 0;
    while next < dirs.len() {
        let dir = dirs[next].clone();
        next += 1;
        for e in list_all(p, &dir, Lane::Bulk).await? {
            let child = dir.join(&e.name)?;
            if e.kind == Kind::Dir {
                dirs.push(child);
            } else {
                remove_if_exists(p, &child).await?;
            }
        }
    }
    for dir in dirs.iter().rev() {
        match p.remove_dir(dir).await {
            Err(e) if e.kind != ErrorKind::NotFound => return Err(e),
            _ => {}
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ops::{ConflictChoice, PlanItem};
    use crate::provider::memory::MemoryProvider;
    use crate::provider::no_progress;

    fn vp(s: &str) -> VPath {
        VPath::parse(s.as_bytes()).unwrap()
    }

    fn item(src: &str, dst: &str, kind: Kind, size: u64) -> PlanItem {
        PlanItem {
            src: Uri::new("a", vp(src)),
            dst: Uri::new("b", vp(dst)),
            kind,
            size: Some(size),
            mtime_ms: Some(5000),
            mode: None,
            link_target: None,
            conflict: None,
            proposed_name: None,
            resolution: None,
        }
    }

    fn job(plan: PlanItem) -> ItemJob {
        let name = plan.dst.name().unwrap_or(b"x").to_vec();
        ItemJob {
            temp_name: temp_name_for(&name, 1, 0),
            plan,
        }
    }

    fn env(src: &MemoryProvider, dst: &MemoryProvider, same: bool, op: OperationKind) -> ItemEnv {
        ItemEnv {
            src: Arc::new(src.clone()),
            dst: Arc::new(dst.clone()),
            same_location: same,
            op,
            options: TransferOptions::default(),
            scratch_dir: std::env::temp_dir(),
            trasher: None,
            progress: no_progress(),
        }
    }

    fn plain() -> MemoryProvider {
        MemoryProvider::new(Capabilities::with(&[cap::WRITE, cap::SET_MTIME]))
    }

    #[test]
    fn temp_names() {
        let n = temp_name_for(b"movie.mkv", 7, 3);
        let s = String::from_utf8(n.clone()).unwrap();
        assert!(s.starts_with(".movie.mkv.lautta-") && s.ends_with(".part"), "{s}");
        assert_eq!(n, temp_name_for(b"movie.mkv", 7, 3));
        assert_ne!(n, temp_name_for(b"movie.mkv", 7, 4));
        assert_ne!(n, temp_name_for(b"movie.mkv", 8, 3));
        let long = temp_name_for(&[b'a'; 300], 1, 1);
        assert!(long.len() < 255);
    }

    #[test]
    fn algorithm_negotiation() {
        let both = Capabilities::with(&[cap::CHECKSUMS]);
        assert_eq!(common_algorithm(&both, &both).as_deref(), Some("sha256"));
        assert_eq!(common_algorithm(&both, &Capabilities::default()), None);
        let mut a = Capabilities::with(&[cap::CHECKSUMS]);
        let mut b = a.clone();
        a.checksum_algorithms = vec!["md5".into(), "sha1".into()];
        b.checksum_algorithms = vec!["sha1".into(), "md5".into(), "sha256".into()];
        assert_eq!(common_algorithm(&a, &b).as_deref(), Some("sha1"));
        a.checksum_algorithms = vec!["crc32".into(), "xxh".into()];
        b.checksum_algorithms = vec!["xxh".into(), "crc32".into()];
        assert_eq!(common_algorithm(&a, &b).as_deref(), Some("crc32"));
        b.checksum_algorithms = vec!["blake3".into()];
        assert_eq!(common_algorithm(&a, &b), None);
    }

    #[test]
    fn digest_comparison() {
        assert!(digests_match(b"ab", b"ab").is_ok());
        assert_eq!(digests_match(b"ab", b"ac").unwrap_err().kind, ErrorKind::Io);
    }

    #[test]
    fn progress_halves_and_clamps() {
        let seen = Arc::new(AtomicU64::new(0));
        let s2 = seen.clone();
        let report: ProgressSink = Arc::new(move |d, _| s2.store(d, Ordering::SeqCst));
        let (down, up) = phase_sinks(report, 10, Some(100));
        down(100, Some(100));
        assert_eq!(seen.load(Ordering::SeqCst), 55);
        up(50, Some(100));
        assert_eq!(seen.load(Ordering::SeqCst), 75);
        up(500, Some(100));
        assert_eq!(seen.load(Ordering::SeqCst), 100);
        down(0, None);
        assert_eq!(seen.load(Ordering::SeqCst), 55, "never below the committed base");
        assert_eq!(clamp_to(7, None), 7);
    }

    #[tokio::test]
    async fn generic_copy_goes_through_a_temp_name_and_preserves_mtime() {
        let (src, dst) = (MemoryProvider::default(), plain());
        src.add_file("f", b"hello", 5000);
        let j = job(item("f", "out/f", Kind::File, 5));
        dst.add_dir("out");
        let e = env(&src, &dst, false, OperationKind::Copy);
        assert_eq!(run_item(&e, &j).await.unwrap(), Outcome::Done);
        assert_eq!(dst.read_file("out/f").unwrap(), b"hello");
        assert!(src.exists("f"));
        let calls = dst.calls().join("|");
        let temp = String::from_utf8(j.temp_name.clone()).unwrap();
        assert!(calls.contains(&format!("upload_from out/{temp}")), "{calls}");
        assert!(calls.contains(&format!("rename out/{temp}")), "{calls}");
        assert!(!dst.exists(&format!("out/{temp}")));
        let m = dst.stat(&vp("out/f"), false, Lane::Bulk).await.unwrap();
        assert_eq!(m.modified_ms(), Some(5000));
    }

    #[tokio::test]
    async fn atomic_put_writes_directly() {
        let src = MemoryProvider::default();
        let dst = MemoryProvider::new(Capabilities::with(&[cap::WRITE, cap::ATOMIC_PUT]));
        src.add_file("f", b"abc", 1);
        let e = env(&src, &dst, false, OperationKind::Copy);
        run_item(&e, &job(item("f", "f", Kind::File, 3))).await.unwrap();
        assert_eq!(dst.read_file("f").unwrap(), b"abc");
        assert!(
            !dst.calls().iter().any(|c| c.starts_with("rename")),
            "{:?}",
            dst.calls()
        );
    }

    #[tokio::test]
    async fn same_location_uses_server_copy_when_capable_and_streams_otherwise() {
        let p = MemoryProvider::default();
        p.add_file("a", b"data", 1);
        let e = env(&p, &p, true, OperationKind::Copy);
        run_item(&e, &job(item("a", "b", Kind::File, 4))).await.unwrap();
        assert_eq!(p.read_file("b").unwrap(), b"data");
        let calls = p.calls().join("|");
        assert!(calls.contains("server_copy a"), "{calls}");
        assert!(!calls.contains("upload_from"), "{calls}");

        let q = plain();
        q.add_file("a", b"data", 1);
        let e = env(&q, &q, true, OperationKind::Copy);
        run_item(&e, &job(item("a", "b", Kind::File, 4))).await.unwrap();
        assert_eq!(q.read_file("b").unwrap(), b"data");
        assert!(q.calls().join("|").contains("upload_from"));
    }

    #[tokio::test]
    async fn same_location_move_is_a_rename() {
        let p = MemoryProvider::default();
        p.add_file("a", b"data", 1);
        let e = env(&p, &p, true, OperationKind::Move);
        run_item(&e, &job(item("a", "b", Kind::File, 4))).await.unwrap();
        assert!(!p.exists("a"));
        assert_eq!(p.read_file("b").unwrap(), b"data");
        let calls = p.calls().join("|");
        assert!(
            !calls.contains("server_copy") && !calls.contains("upload_from"),
            "{calls}"
        );
    }

    #[tokio::test]
    async fn rename_that_crosses_devices_falls_back_to_copy_then_delete() {
        let p = MemoryProvider::default();
        p.add_file("a", b"data", 1);
        p.fail_next("rename", "a", Error::kind(ErrorKind::CrossesDevice));
        let e = env(&p, &p, true, OperationKind::Move);
        run_item(&e, &job(item("a", "b", Kind::File, 4))).await.unwrap();
        assert!(!p.exists("a"));
        assert_eq!(p.read_file("b").unwrap(), b"data");
    }

    #[tokio::test]
    async fn cross_location_move_copies_verifies_and_deletes_the_source() {
        let (src, dst) = (MemoryProvider::default(), MemoryProvider::default());
        src.add_file("a", b"data", 1);
        let e = env(&src, &dst, false, OperationKind::Move);
        run_item(&e, &job(item("a", "a", Kind::File, 4))).await.unwrap();
        assert!(!src.exists("a"));
        assert_eq!(dst.read_file("a").unwrap(), b"data");
        let sums: Vec<String> = src
            .calls()
            .into_iter()
            .chain(dst.calls())
            .filter(|c| c.starts_with("checksum"))
            .collect();
        assert_eq!(sums.len(), 2, "both sides were summed: {sums:?}");
    }

    #[tokio::test]
    async fn copy_verifies_checksums_only_when_enabled() {
        let (src, dst) = (MemoryProvider::default(), MemoryProvider::default());
        src.add_file("a", b"data", 1);
        let mut e = env(&src, &dst, false, OperationKind::Copy);
        run_item(&e, &job(item("a", "a", Kind::File, 4))).await.unwrap();
        assert!(!dst.calls().iter().any(|c| c.starts_with("checksum")));
        e.options.verify_checksums = true;
        run_item(&e, &job(item("a", "c", Kind::File, 4))).await.unwrap();
        assert!(dst.calls().iter().any(|c| c.starts_with("checksum")));
    }

    #[tokio::test]
    async fn checksum_is_skipped_when_a_side_cannot_do_it() {
        let src = MemoryProvider::default();
        let dst = plain();
        src.add_file("a", b"data", 1);
        let mut e = env(&src, &dst, false, OperationKind::Move);
        e.options.verify_checksums = true;
        run_item(&e, &job(item("a", "a", Kind::File, 4))).await.unwrap();
        assert!(!src.exists("a"));
        assert!(!src.calls().iter().any(|c| c.starts_with("checksum")));
    }

    #[tokio::test]
    async fn a_checksum_error_fails_the_item_and_keeps_the_source() {
        let (src, dst) = (MemoryProvider::default(), MemoryProvider::default());
        src.add_file("a", b"data", 1);
        src.fail_next("checksum", "a", Error::kind(ErrorKind::TimedOut));
        let e = env(&src, &dst, false, OperationKind::Move);
        let err = run_item(&e, &job(item("a", "a", Kind::File, 4)))
            .await
            .unwrap_err();
        assert_eq!(err.kind, ErrorKind::TimedOut);
        assert!(src.exists("a"), "never delete before verification");
    }

    #[tokio::test]
    async fn resumes_from_the_partial_temp_file() {
        let (src, dst) = (MemoryProvider::default(), MemoryProvider::default());
        src.add_file("big", b"0123456789", 1);
        let j = job(item("big", "big", Kind::File, 10));
        let temp = String::from_utf8(j.temp_name.clone()).unwrap();
        dst.add_file(&temp, b"0123", 1);
        let e = env(&src, &dst, false, OperationKind::Copy);
        run_item(&e, &j).await.unwrap();
        assert_eq!(dst.read_file("big").unwrap(), b"0123456789");
        assert!(!dst.exists(&temp));
    }

    #[tokio::test]
    async fn restarts_when_the_destination_cannot_resume() {
        let src = MemoryProvider::default();
        let dst = MemoryProvider::new(Capabilities::with(&[cap::WRITE]));
        src.add_file("big", b"0123456789", 1);
        let j = job(item("big", "big", Kind::File, 10));
        let temp = String::from_utf8(j.temp_name.clone()).unwrap();
        dst.add_file(&temp, b"XXXX", 1);
        let e = env(&src, &dst, false, OperationKind::Copy);
        run_item(&e, &j).await.unwrap();
        assert_eq!(dst.read_file("big").unwrap(), b"0123456789");
    }

    #[tokio::test]
    async fn an_oversized_or_complete_partial_is_handled() {
        let (src, dst) = (MemoryProvider::default(), MemoryProvider::default());
        src.add_file("f", b"0123", 1);
        let j = job(item("f", "f", Kind::File, 4));
        let temp = String::from_utf8(j.temp_name.clone()).unwrap();
        dst.add_file(&temp, b"0123456", 1);
        let e = env(&src, &dst, false, OperationKind::Copy);
        run_item(&e, &j).await.unwrap();
        assert_eq!(dst.read_file("f").unwrap(), b"0123");

        let j = job(item("f", "g", Kind::File, 4));
        let temp = String::from_utf8(j.temp_name.clone()).unwrap();
        dst.add_file(&temp, b"0123", 1);
        run_item(&e, &j).await.unwrap();
        assert_eq!(dst.read_file("g").unwrap(), b"0123");
        let uploads_after = dst
            .calls()
            .iter()
            .filter(|c| c.starts_with("upload_from"))
            .count();
        assert_eq!(uploads_after, 1, "a complete partial is not uploaded again");
    }

    #[tokio::test]
    async fn existing_destination_asks_and_never_overwrites() {
        let (src, dst) = (MemoryProvider::default(), MemoryProvider::default());
        src.add_file("f", b"new", 1);
        dst.add_file("f", b"old", 1);
        let e = env(&src, &dst, false, OperationKind::Copy);
        let out = run_item(&e, &job(item("f", "f", Kind::File, 3))).await.unwrap();
        assert!(matches!(out, Outcome::Ask(_)));
        assert_eq!(dst.read_file("f").unwrap(), b"old");
    }

    #[tokio::test]
    async fn resolutions_replace_skip_keep_both() {
        let (src, dst) = (MemoryProvider::default(), MemoryProvider::default());
        src.add_file("f", b"new", 1);
        dst.add_file("f", b"old", 1);
        let e = env(&src, &dst, false, OperationKind::Copy);
        let with = |c| {
            let mut p = item("f", "f", Kind::File, 3);
            p.resolution = Some(c);
            job(p)
        };
        assert_eq!(
            run_item(&e, &with(ConflictChoice::Skip)).await.unwrap(),
            Outcome::Skipped
        );
        assert_eq!(dst.read_file("f").unwrap(), b"old");
        let Outcome::KeepBoth(u) = run_item(&e, &with(ConflictChoice::KeepBoth)).await.unwrap() else {
            panic!("keep both");
        };
        assert_eq!(u.path.display(), "f 2");
        dst.add_file("f 2", b"", 1);
        let Outcome::KeepBoth(u) = run_item(&e, &with(ConflictChoice::KeepBoth)).await.unwrap() else {
            panic!("keep both");
        };
        assert_eq!(u.path.display(), "f 3");
        assert_eq!(
            run_item(&e, &with(ConflictChoice::Replace)).await.unwrap(),
            Outcome::Done
        );
        assert_eq!(dst.read_file("f").unwrap(), b"new");
    }

    #[tokio::test]
    async fn resume_in_place_continues_the_destination_file() {
        let (src, dst) = (MemoryProvider::default(), MemoryProvider::default());
        src.add_file("f", b"0123456789", 1);
        dst.add_file("f", b"0123", 1);
        let e = env(&src, &dst, false, OperationKind::Copy);
        let mut p = item("f", "f", Kind::File, 10);
        p.resolution = Some(ConflictChoice::Resume);
        assert_eq!(run_item(&e, &job(p)).await.unwrap(), Outcome::Done);
        assert_eq!(dst.read_file("f").unwrap(), b"0123456789");
    }

    #[tokio::test]
    async fn a_race_at_the_final_rename_becomes_a_question() {
        let (src, dst) = (MemoryProvider::default(), MemoryProvider::default());
        src.add_file("f", b"new", 1);
        let j = job(item("f", "f", Kind::File, 3));
        let temp = String::from_utf8(j.temp_name.clone()).unwrap();
        dst.fail_next("rename", &temp, Error::kind(ErrorKind::AlreadyExists));
        dst.add_file("f", b"appeared", 1);
        let e = env(&src, &dst, false, OperationKind::Copy);
        // The stat before writing already sees the file, so the question comes first.
        assert!(matches!(run_item(&e, &j).await.unwrap(), Outcome::Ask(_)));
    }

    #[tokio::test]
    async fn already_exists_from_the_final_rename_without_a_file_is_an_error() {
        let (src, dst) = (MemoryProvider::default(), MemoryProvider::default());
        src.add_file("f", b"new", 1);
        let j = job(item("f", "f", Kind::File, 3));
        let temp = String::from_utf8(j.temp_name.clone()).unwrap();
        dst.fail_next("rename", &temp, Error::kind(ErrorKind::AlreadyExists));
        let e = env(&src, &dst, false, OperationKind::Copy);
        let err = run_item(&e, &j).await.unwrap_err();
        assert_eq!(err.kind, ErrorKind::AlreadyExists);
    }

    #[tokio::test]
    async fn folders_symlinks_and_merge() {
        let (src, dst) = (MemoryProvider::default(), MemoryProvider::default());
        let e = env(&src, &dst, false, OperationKind::Copy);
        assert_eq!(
            run_item(&e, &job(item("d", "d", Kind::Dir, 0))).await.unwrap(),
            Outcome::Done
        );
        assert!(dst.exists("d"));
        assert!(matches!(
            run_item(&e, &job(item("d", "d", Kind::Dir, 0))).await.unwrap(),
            Outcome::Ask(_)
        ));
        let mut merge = item("d", "d", Kind::Dir, 0);
        merge.resolution = Some(ConflictChoice::Merge);
        assert_eq!(run_item(&e, &job(merge)).await.unwrap(), Outcome::Done);

        src.add_symlink("l", "target");
        let mut link = item("l", "d/l", Kind::Symlink, 0);
        link.link_target = Some(b"target".to_vec());
        assert_eq!(run_item(&e, &job(link)).await.unwrap(), Outcome::Done);
        assert_eq!(
            dst.stat(&vp("d/l"), false, Lane::Bulk).await.unwrap().kind,
            Kind::Symlink
        );
        let read = job(item("l", "d/l2", Kind::Symlink, 0));
        run_item(&e, &read).await.unwrap();
        assert!(dst.exists("d/l2"));

        let special = item("s", "s", Kind::Special, 0);
        assert_eq!(
            run_item(&e, &job(special)).await.unwrap_err().kind,
            ErrorKind::Unsupported
        );
    }

    #[tokio::test]
    async fn same_location_folder_move_is_one_rename() {
        let p = MemoryProvider::default();
        p.add_file("d/f", b"x", 1);
        let e = env(&p, &p, true, OperationKind::Move);
        let mut it = item("d", "e", Kind::Dir, 0);
        it.dst = Uri::new("a", vp("e"));
        assert_eq!(run_item(&e, &job(it)).await.unwrap(), Outcome::Subtree);
        assert!(p.exists("e/f") && !p.exists("d"));
    }

    #[tokio::test]
    async fn symlink_move_removes_the_source_link() {
        let p = MemoryProvider::default();
        let q = MemoryProvider::default();
        p.add_symlink("l", "t");
        let e = env(&p, &q, false, OperationKind::Move);
        run_item(&e, &job(item("l", "l", Kind::Symlink, 0)))
            .await
            .unwrap();
        assert!(!p.exists("l") && q.exists("l"));
    }

    #[tokio::test]
    async fn deletes_children_first_and_tolerates_missing() {
        let p = MemoryProvider::default();
        p.add_file("d/f", b"x", 1);
        let e = env(&p, &p, true, OperationKind::Delete);
        let file = job(item("d/f", "d/f", Kind::File, 1));
        assert_eq!(run_item(&e, &file).await.unwrap(), Outcome::Done);
        assert_eq!(run_item(&e, &file).await.unwrap(), Outcome::Done);
        let dir = job(item("d", "d", Kind::Dir, 0));
        assert_eq!(run_item(&e, &dir).await.unwrap(), Outcome::Done);
        assert!(!p.exists("d"));
        p.add_file("d/f", b"x", 1);
        assert_eq!(
            run_item(&e, &dir).await.unwrap_err().kind,
            ErrorKind::DirectoryNotEmpty
        );
    }

    struct FakeTrash(std::sync::Mutex<Vec<String>>, bool);

    #[async_trait::async_trait]
    impl Trasher for FakeTrash {
        async fn trash(&self, uri: &Uri) -> Result<bool> {
            self.0.lock().unwrap().push(uri.path.display());
            Ok(self.1)
        }
    }

    #[tokio::test]
    async fn trash_mode_uses_the_trasher_or_deletes_the_tree() {
        let p = MemoryProvider::default();
        p.add_file("d/sub/f", b"x", 1);
        p.add_file("keep", b"x", 1);
        let mut e = env(&p, &p, true, OperationKind::Delete);
        e.options.trash = true;
        let trash = Arc::new(FakeTrash(Default::default(), true));
        e.trasher = Some(trash.clone());
        let dir = job(item("d", "d", Kind::Dir, 0));
        assert_eq!(run_item(&e, &dir).await.unwrap(), Outcome::Subtree);
        assert_eq!(*trash.0.lock().unwrap(), vec!["d".to_owned()]);
        assert!(p.exists("d"), "the fake trasher moved nothing");

        e.trasher = Some(Arc::new(FakeTrash(Default::default(), false)));
        assert_eq!(run_item(&e, &dir).await.unwrap(), Outcome::Subtree);
        assert!(!p.exists("d") && p.exists("keep"));

        p.add_file("d2/f", b"x", 1);
        e.trasher = None;
        let dir2 = job(item("d2", "d2", Kind::Dir, 0));
        run_item(&e, &dir2).await.unwrap();
        assert!(!p.exists("d2"));
    }

    #[tokio::test]
    async fn cleanup_removes_the_partial_file() {
        let dst = MemoryProvider::default();
        let j = job(item("f", "out/f", Kind::File, 1));
        let temp = String::from_utf8(j.temp_name.clone()).unwrap();
        dst.add_file(&format!("out/{temp}"), b"x", 1);
        cleanup_temp(&dst, &j).await;
        assert!(!dst.exists(&format!("out/{temp}")));
        cleanup_temp(&dst, &j).await;
    }

    #[tokio::test]
    async fn modes_are_copied_only_when_enabled_and_supported() {
        let (src, dst) = (MemoryProvider::default(), MemoryProvider::default());
        src.add_file("f", b"x", 1);
        src.set_attributes(
            &vp("f"),
            AttributeChanges {
                mode: Some(0o600),
                modified: None,
            },
        )
        .await
        .unwrap();
        let mut e = env(&src, &dst, false, OperationKind::Copy);
        run_item(&e, &job(item("f", "plain", Kind::File, 1)))
            .await
            .unwrap();
        let m = dst.stat(&vp("plain"), false, Lane::Bulk).await.unwrap();
        assert_eq!(m.mode, Some(0o644));
        e.options.preserve_mode = true;
        run_item(&e, &job(item("f", "kept", Kind::File, 1)))
            .await
            .unwrap();
        let m = dst.stat(&vp("kept"), false, Lane::Bulk).await.unwrap();
        assert_eq!(m.mode, Some(0o600));
    }

    #[tokio::test]
    async fn mtime_is_not_preserved_when_disabled() {
        let (src, dst) = (MemoryProvider::default(), plain());
        src.add_file("f", b"x", 12345);
        let mut e = env(&src, &dst, false, OperationKind::Copy);
        e.options.preserve_mtime = false;
        run_item(&e, &job(item("f", "f", Kind::File, 1))).await.unwrap();
        let m = dst.stat(&vp("f"), false, Lane::Bulk).await.unwrap();
        assert_ne!(m.modified_ms(), Some(12345));
    }

    #[tokio::test]
    async fn the_plan_size_is_a_hint_and_the_source_stat_is_the_truth() {
        let (src, dst) = (MemoryProvider::default(), MemoryProvider::default());
        src.add_file("f", b"ab", 1);
        let e = env(&src, &dst, false, OperationKind::Copy);
        let j = job(item("f", "f", Kind::File, 3));
        assert_eq!(run_item(&e, &j).await.unwrap(), Outcome::Done);
        assert_eq!(dst.read_file("f").unwrap(), b"ab");
    }

    #[tokio::test]
    async fn a_short_read_fails_without_leaving_a_partial() {
        let (src, dst) = (MemoryProvider::default(), MemoryProvider::default());
        src.add_file("f", b"abc", 1);
        let e = env(&src, &dst, false, OperationKind::Copy);
        let j = job(item("f", "f", Kind::File, 3));
        src.fail_next("download_into", "f", Error::kind(ErrorKind::PermissionDenied));
        let err = run_item(&e, &j).await.unwrap_err();
        assert_eq!(err.kind, ErrorKind::PermissionDenied);
        assert!(dst.paths().is_empty(), "{:?}", dst.paths());
    }
}
