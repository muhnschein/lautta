// SPDX-License-Identifier: LGPL-2.1-or-later
#![allow(unsafe_code)]
//! The only module with unsafe code (SPEC RS-5): renameat2, statx, FICLONE, fd helpers.
//!
//! rustix 0.38 wraps every syscall Lautta needs behind a safe API, so this
//! module currently contains no `unsafe` block at all; it stays the single
//! place where one may be added. Everything returns `std::io::Result` so the
//! callers map errors with `Error::from` and keep the errno.

use rustix::fs::{
    copy_file_range, fadvise, ioctl_ficlone, renameat_with, statvfs as rx_statvfs, utimensat, Advice,
    AtFlags, RenameFlags, StatxFlags, Timespec, Timestamps, CWD, UTIME_OMIT,
};
use rustix::io::Errno;
use std::fs::File;
use std::io;
use std::os::unix::fs::MetadataExt;
use std::path::Path;
use std::time::{Duration, SystemTime};

/// Largest single `copy_file_range` request. The kernel may copy less.
const COPY_CHUNK: usize = 8 << 20;

/// `S_IFMT` and the file type values (not exported by `std`).
const S_IFMT: u32 = 0o170_000;
const S_IFDIR: u32 = 0o040_000;
const S_IFREG: u32 = 0o100_000;
const S_IFLNK: u32 = 0o120_000;

/// What `stat` returns, whichever syscall produced it.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct FileStat {
    pub mode: u32,
    pub size: u64,
    pub uid: u32,
    pub gid: u32,
    pub mtime: SystemTime,
    /// Birth time; `None` when the filesystem or kernel does not record it.
    pub btime: Option<SystemTime>,
    pub dev: u64,
    pub ino: u64,
    pub nlink: u64,
}

impl FileStat {
    pub fn is_dir(&self) -> bool {
        self.mode & S_IFMT == S_IFDIR
    }

    pub fn is_file(&self) -> bool {
        self.mode & S_IFMT == S_IFREG
    }

    pub fn is_symlink(&self) -> bool {
        self.mode & S_IFMT == S_IFLNK
    }

    /// Permission bits including setuid/setgid/sticky.
    pub fn permissions(&self) -> u32 {
        self.mode & 0o7777
    }

    /// Fallback for kernels without `statx`: no birth time.
    pub fn from_metadata(m: &std::fs::Metadata) -> FileStat {
        FileStat {
            mode: m.mode(),
            size: m.size(),
            uid: m.uid(),
            gid: m.gid(),
            mtime: timestamp(m.mtime(), u32::try_from(m.mtime_nsec()).unwrap_or(0)),
            btime: m.created().ok(),
            dev: m.dev(),
            ino: m.ino(),
            nlink: m.nlink(),
        }
    }
}

/// Seconds (possibly negative) plus nanoseconds to a `SystemTime`.
pub fn timestamp(sec: i64, nsec: u32) -> SystemTime {
    let nanos = Duration::from_nanos(u64::from(nsec));
    if sec >= 0 {
        SystemTime::UNIX_EPOCH + Duration::from_secs(sec.unsigned_abs()) + nanos
    } else {
        SystemTime::UNIX_EPOCH - Duration::from_secs(sec.unsigned_abs()) + nanos
    }
}

/// `statx` in one syscall: type, size, owner, times and the birth time
/// (SPEC BRW-1 "created"). `follow` selects stat vs lstat.
pub fn stat(path: &Path, follow: bool) -> io::Result<FileStat> {
    let flags = if follow {
        AtFlags::empty()
    } else {
        AtFlags::SYMLINK_NOFOLLOW
    };
    match rustix::fs::statx(CWD, path, flags, StatxFlags::BASIC_STATS | StatxFlags::BTIME) {
        Ok(sx) => {
            let btime = (sx.stx_mask & StatxFlags::BTIME.bits() != 0)
                .then(|| timestamp(sx.stx_btime.tv_sec, sx.stx_btime.tv_nsec));
            Ok(FileStat {
                mode: u32::from(sx.stx_mode),
                size: sx.stx_size,
                uid: sx.stx_uid,
                gid: sx.stx_gid,
                mtime: timestamp(sx.stx_mtime.tv_sec, sx.stx_mtime.tv_nsec),
                btime,
                dev: (u64::from(sx.stx_dev_major) << 32) | u64::from(sx.stx_dev_minor),
                ino: sx.stx_ino,
                nlink: u64::from(sx.stx_nlink),
            })
        }
        // Old kernels and some seccomp profiles refuse statx; the plain
        // stat answers the same question minus the birth time.
        Err(e) if statx_unavailable(e) => {
            let meta = if follow {
                std::fs::metadata(path)?
            } else {
                std::fs::symlink_metadata(path)?
            };
            Ok(FileStat::from_metadata(&meta))
        }
        Err(e) => Err(e.into()),
    }
}

fn statx_unavailable(e: Errno) -> bool {
    e == Errno::NOSYS || e == Errno::PERM || e == Errno::INVAL
}

/// True when `a` and `b` are on the same device (rename-based trash, OPS-8).
pub fn same_device(a: &Path, b: &Path) -> io::Result<bool> {
    Ok(stat(a, true)?.dev == stat(b, true)?.dev)
}

/// `renameat2(RENAME_NOREPLACE)`: atomic "fail if the destination exists".
/// `EEXIST` comes back as `AlreadyExists`; `EINVAL`/`ENOSYS`/`EOPNOTSUPP`
/// mean the filesystem cannot do it (see [`is_flag_unsupported`]).
pub fn rename_noreplace(from: &Path, to: &Path) -> io::Result<()> {
    renameat_with(CWD, from, CWD, to, RenameFlags::NOREPLACE).map_err(Into::into)
}

/// `renameat2(RENAME_EXCHANGE)`: atomically swaps two existing paths.
pub fn rename_exchange(a: &Path, b: &Path) -> io::Result<()> {
    renameat_with(CWD, a, CWD, b, RenameFlags::EXCHANGE).map_err(Into::into)
}

/// The errors a filesystem or kernel returns for renameat2 flags it lacks.
pub fn is_flag_unsupported(e: &io::Error) -> bool {
    matches!(e.raw_os_error(), Some(code)
        if [Errno::INVAL, Errno::NOSYS, Errno::OPNOTSUPP].iter().any(|k| k.raw_os_error() == code))
}

/// `FICLONE`: makes `dst` share `src`'s extents (reflink). Fails with
/// `EOPNOTSUPP`/`EXDEV`/`EINVAL`/`ENOTTY` when the filesystem cannot.
pub fn clone_file(dst: &File, src: &File) -> io::Result<()> {
    ioctl_ficlone(dst, src).map_err(Into::into)
}

/// Copies from the current position of `src` to the current position of
/// `dst` until end of file with `copy_file_range`, calling `progress` with
/// the running total of bytes. On error the positions stay consistent, so the
/// caller can continue with a buffered copy.
pub fn copy_range_all(src: &File, dst: &File, progress: &mut dyn FnMut(u64)) -> io::Result<u64> {
    let mut total = 0u64;
    loop {
        match copy_file_range(src, None, dst, None, COPY_CHUNK) {
            Ok(0) => return Ok(total),
            Ok(n) => {
                total += n as u64;
                progress(total);
            }
            Err(Errno::INTR) => {}
            Err(e) => return Err(e.into()),
        }
    }
}

/// The `statfs` `f_type` magic of the filesystem holding `path`.
pub fn fs_magic(path: &Path) -> io::Result<u64> {
    let st = rustix::fs::statfs(path)?;
    // f_type is signed on some targets; the magic is a 32-bit value.
    Ok(st.f_type as u64 & 0xFFFF_FFFF)
}

/// Space and mount flags from `statvfs`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct VfsInfo {
    pub free: u64,
    pub total: u64,
    pub used: u64,
    pub read_only: bool,
    pub name_max: u64,
}

pub fn statvfs(path: &Path) -> io::Result<VfsInfo> {
    let v = rx_statvfs(path)?;
    let total = v.f_blocks.saturating_mul(v.f_frsize);
    let free_all = v.f_bfree.saturating_mul(v.f_frsize);
    Ok(VfsInfo {
        free: v.f_bavail.saturating_mul(v.f_frsize),
        total,
        used: total.saturating_sub(free_all),
        read_only: v.f_flag.contains(rustix::fs::StatVfsMountFlags::RDONLY),
        name_max: v.f_namemax,
    })
}

/// Sets the modification time (`utimensat`), leaving the access time alone.
pub fn set_mtime(path: &Path, mtime: SystemTime) -> io::Result<()> {
    let (sec, nsec) = match mtime.duration_since(SystemTime::UNIX_EPOCH) {
        Ok(d) => (
            i64::try_from(d.as_secs()).unwrap_or(i64::MAX),
            i64::from(d.subsec_nanos()),
        ),
        Err(e) => {
            let d = e.duration();
            let whole = i64::try_from(d.as_secs()).unwrap_or(i64::MAX);
            match d.subsec_nanos() {
                0 => (-whole, 0),
                n => (-whole - 1, 1_000_000_000 - i64::from(n)),
            }
        }
    };
    let times = Timestamps {
        last_access: Timespec {
            tv_sec: 0,
            tv_nsec: UTIME_OMIT,
        },
        last_modification: Timespec {
            tv_sec: sec,
            tv_nsec: nsec,
        },
    };
    utimensat(CWD, path, &times, AtFlags::empty()).map_err(Into::into)
}

/// Read-ahead hint (`posix_fadvise(WILLNEED)`, PRV-8). Best effort.
pub fn advise_willneed(file: &File, offset: u64, len: u64) {
    // Only a hint: failure changes nothing.
    let _ = fadvise(file, offset, len, Advice::WillNeed);
}

/// Effective user and group id of this process.
pub fn effective_ids() -> (u32, u32) {
    (
        rustix::process::geteuid().as_raw(),
        rustix::process::getegid().as_raw(),
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::{Read, Seek, SeekFrom, Write};
    use std::os::unix::ffi::OsStrExt;

    fn tmp() -> tempfile::TempDir {
        tempfile::tempdir().unwrap()
    }

    #[test]
    fn noreplace_refuses_existing_destination() {
        let d = tmp();
        let (a, b) = (d.path().join("a"), d.path().join("b"));
        std::fs::write(&a, "A").unwrap();
        std::fs::write(&b, "B").unwrap();
        let err = rename_noreplace(&a, &b).unwrap_err();
        assert_eq!(err.kind(), io::ErrorKind::AlreadyExists);
        assert_eq!(std::fs::read(&a).unwrap(), b"A");
        assert_eq!(std::fs::read(&b).unwrap(), b"B");
    }

    #[test]
    fn noreplace_moves_when_free() {
        let d = tmp();
        let (a, c) = (d.path().join("a"), d.path().join("c"));
        std::fs::write(&a, "A").unwrap();
        rename_noreplace(&a, &c).unwrap();
        assert!(!a.exists());
        assert_eq!(std::fs::read(&c).unwrap(), b"A");
    }

    #[test]
    fn noreplace_handles_non_utf8_names() {
        let d = tmp();
        let a = d.path().join(std::ffi::OsStr::from_bytes(b"caf\xe9"));
        let b = d.path().join(std::ffi::OsStr::from_bytes(b"\xff\xfe"));
        std::fs::write(&a, "x").unwrap();
        rename_noreplace(&a, &b).unwrap();
        assert!(b.exists() && !a.exists());
    }

    #[test]
    fn exchange_swaps_contents() {
        let d = tmp();
        let (a, b) = (d.path().join("a"), d.path().join("b"));
        std::fs::write(&a, "A").unwrap();
        std::fs::write(&b, "B").unwrap();
        rename_exchange(&a, &b).unwrap();
        assert_eq!(std::fs::read(&a).unwrap(), b"B");
        assert_eq!(std::fs::read(&b).unwrap(), b"A");
        let missing = d.path().join("missing");
        assert_eq!(
            rename_exchange(&a, &missing).unwrap_err().kind(),
            io::ErrorKind::NotFound
        );
    }

    #[test]
    fn flag_unsupported_classification() {
        let e = io::Error::from_raw_os_error(Errno::INVAL.raw_os_error());
        assert!(is_flag_unsupported(&e));
        let e = io::Error::from_raw_os_error(Errno::OPNOTSUPP.raw_os_error());
        assert!(is_flag_unsupported(&e));
        let e = io::Error::from_raw_os_error(Errno::EXIST.raw_os_error());
        assert!(!is_flag_unsupported(&e));
        assert!(!is_flag_unsupported(&io::Error::new(io::ErrorKind::Other, "x")));
    }

    #[test]
    fn clone_either_works_or_reports_unsupported() {
        let d = tmp();
        let (a, b) = (d.path().join("a"), d.path().join("b"));
        std::fs::write(&a, "reflink me").unwrap();
        let src = File::open(&a).unwrap();
        let dst = File::create(&b).unwrap();
        match clone_file(&dst, &src) {
            Ok(()) => assert_eq!(std::fs::read(&b).unwrap(), b"reflink me"),
            Err(e) => {
                // ext4/tmpfs have no reflinks: the error must be one the
                // copy code treats as "fall back".
                assert!(
                    matches!(e.raw_os_error(), Some(c) if [
                        Errno::OPNOTSUPP, Errno::XDEV, Errno::INVAL, Errno::NOTTY, Errno::NOSYS, Errno::BADF
                    ].iter().any(|k| k.raw_os_error() == c)),
                    "unexpected error {e}"
                );
                assert_eq!(std::fs::read(&b).unwrap(), b"");
            }
        }
    }

    #[test]
    fn copy_range_copies_everything_from_the_current_position() {
        let d = tmp();
        let (a, b) = (d.path().join("a"), d.path().join("b"));
        let data: Vec<u8> = (0..300_000u32).map(|i| (i % 251) as u8).collect();
        std::fs::write(&a, &data).unwrap();
        let mut src = File::open(&a).unwrap();
        src.seek(SeekFrom::Start(1000)).unwrap();
        let dst = File::create(&b).unwrap();
        let mut last = 0;
        let n = copy_range_all(&src, &dst, &mut |t| last = t).unwrap();
        assert_eq!(n, 299_000);
        assert_eq!(last, 299_000);
        assert_eq!(std::fs::read(&b).unwrap(), &data[1000..]);
    }

    #[test]
    fn copy_range_rejects_directories() {
        let d = tmp();
        let src = File::open(d.path()).unwrap();
        let dst = File::create(d.path().join("o")).unwrap();
        assert!(copy_range_all(&src, &dst, &mut |_| {}).is_err());
    }

    #[test]
    fn stat_reports_type_size_and_times() {
        let d = tmp();
        let f = d.path().join("f");
        std::fs::write(&f, "12345").unwrap();
        let st = stat(&f, true).unwrap();
        assert!(st.is_file() && !st.is_dir() && !st.is_symlink());
        assert_eq!(st.size, 5);
        assert!(st.nlink >= 1);
        let age = SystemTime::now().duration_since(st.mtime).unwrap_or_default();
        assert!(age < Duration::from_secs(60));
        assert!(stat(d.path(), true).unwrap().is_dir());
        if let Some(b) = st.btime {
            let age = SystemTime::now().duration_since(b).unwrap_or_default();
            assert!(age < Duration::from_secs(60));
        }
    }

    #[test]
    fn stat_follow_selects_link_or_target() {
        let d = tmp();
        let f = d.path().join("f");
        let l = d.path().join("l");
        std::fs::write(f, "123456789").unwrap();
        std::os::unix::fs::symlink("f", &l).unwrap();
        let lst = stat(&l, false).unwrap();
        assert!(lst.is_symlink());
        assert_eq!(lst.size, 1);
        let fst = stat(&l, true).unwrap();
        assert!(fst.is_file());
        assert_eq!(fst.size, 9);
        std::os::unix::fs::symlink("nowhere", d.path().join("dangling")).unwrap();
        assert_eq!(
            stat(&d.path().join("dangling"), true).unwrap_err().kind(),
            io::ErrorKind::NotFound
        );
        assert!(stat(&d.path().join("dangling"), false).unwrap().is_symlink());
    }

    #[test]
    fn stat_matches_metadata_fallback() {
        let d = tmp();
        let f = d.path().join("f");
        std::fs::write(&f, "abc").unwrap();
        let a = stat(&f, true).unwrap();
        let b = FileStat::from_metadata(&std::fs::metadata(&f).unwrap());
        assert_eq!(a.mode, b.mode);
        assert_eq!(a.size, b.size);
        assert_eq!(a.uid, b.uid);
        assert_eq!(a.mtime, b.mtime);
        assert_eq!(a.permissions(), b.permissions());
    }

    #[test]
    fn stat_handles_non_utf8_path() {
        let d = tmp();
        let f = d.path().join(std::ffi::OsStr::from_bytes(b"\xc3\x28"));
        std::fs::write(&f, "x").unwrap();
        assert_eq!(stat(&f, true).unwrap().size, 1);
    }

    #[test]
    fn timestamp_handles_negative_seconds() {
        let t = timestamp(-10, 500_000_000);
        assert_eq!(
            SystemTime::UNIX_EPOCH.duration_since(t).unwrap(),
            Duration::from_millis(9500)
        );
        let t = timestamp(10, 5);
        assert_eq!(
            t.duration_since(SystemTime::UNIX_EPOCH).unwrap(),
            Duration::new(10, 5)
        );
    }

    #[test]
    fn fs_magic_of_temp_dir_is_known() {
        let d = tmp();
        let magic = fs_magic(d.path()).unwrap();
        assert_ne!(magic, 0);
        assert_eq!(
            fs_magic(&d.path().join("nope")).unwrap_err().kind(),
            io::ErrorKind::NotFound
        );
    }

    #[test]
    fn statvfs_is_consistent() {
        let d = tmp();
        let v = statvfs(d.path()).unwrap();
        assert!(v.total > 0);
        assert!(v.free <= v.total);
        assert!(v.used <= v.total);
        assert!(!v.read_only);
        assert!(v.name_max >= 100);
    }

    #[test]
    fn set_mtime_round_trips_including_before_epoch() {
        let d = tmp();
        let f = d.path().join("f");
        std::fs::write(&f, "x").unwrap();
        let t = SystemTime::UNIX_EPOCH + Duration::new(1_600_000_000, 123_000_000);
        set_mtime(&f, t).unwrap();
        assert_eq!(stat(&f, true).unwrap().mtime, t);
        let old = SystemTime::UNIX_EPOCH - Duration::new(86_400, 250_000_000);
        set_mtime(&f, old).unwrap();
        assert_eq!(stat(&f, true).unwrap().mtime, old);
        assert_eq!(
            set_mtime(&d.path().join("missing"), t).unwrap_err().kind(),
            io::ErrorKind::NotFound
        );
    }

    #[test]
    fn set_mtime_leaves_content_alone() {
        let d = tmp();
        let f = d.path().join("f");
        std::fs::write(&f, "keep").unwrap();
        set_mtime(&f, SystemTime::UNIX_EPOCH + Duration::from_secs(5)).unwrap();
        let mut s = String::new();
        File::open(&f).unwrap().read_to_string(&mut s).unwrap();
        assert_eq!(s, "keep");
    }

    #[test]
    fn same_device_for_siblings() {
        let d = tmp();
        let f = d.path().join("f");
        std::fs::write(&f, "x").unwrap();
        assert!(same_device(&f, d.path()).unwrap());
        assert!(same_device(&f, &d.path().join("missing")).is_err());
    }

    #[test]
    fn effective_ids_match_metadata_of_new_file() {
        let d = tmp();
        let f = d.path().join("f");
        let mut h = File::create(&f).unwrap();
        h.write_all(b"x").unwrap();
        let (uid, _gid) = effective_ids();
        assert_eq!(stat(&f, true).unwrap().uid, uid);
    }

    #[test]
    fn advise_does_not_panic_on_regular_file() {
        let d = tmp();
        let f = d.path().join("f");
        std::fs::write(&f, "x").unwrap();
        advise_willneed(&File::open(&f).unwrap(), 0, 1);
    }
}
