// SPDX-License-Identifier: LGPL-2.1-or-later

use crate::error::{Error, ErrorKind, Result};
use crate::sys::{fstatvfs, VfsInfo};
use std::fmt;
use std::io::{self, Write};
use std::os::fd::{AsFd, BorrowedFd};
use std::sync::Arc;

pub const KEEP_FREE: u64 = 256 << 20;
pub const RECHECK_EVERY: u64 = 4 << 20;
const KEEP_FREE_SHARE: u64 = 10;

pub type SpaceProbe = Arc<dyn Fn(BorrowedFd<'_>) -> io::Result<VfsInfo> + Send + Sync>;

#[derive(Clone)]
pub struct Room {
    keep_free: u64,
    probe: SpaceProbe,
}

impl Default for Room {
    fn default() -> Self {
        Room::new(KEEP_FREE, Arc::new(fstatvfs))
    }
}

impl fmt::Debug for Room {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("Room")
            .field("keep_free", &self.keep_free)
            .finish()
    }
}

impl Room {
    pub fn new(keep_free: u64, probe: SpaceProbe) -> Room {
        Room { keep_free, probe }
    }

    pub fn ensure(&self, fd: BorrowedFd<'_>, need: u64) -> Result<()> {
        let v = (self.probe)(fd)?;
        let keep = self.keep_free.min(v.total / KEEP_FREE_SHARE);
        if v.free >= need.saturating_add(keep) {
            return Ok(());
        }
        Err(Error::new(
            ErrorKind::NoSpace,
            format!("{need} bytes do not fit: {} free, {keep} must stay free", v.free),
        ))
    }

    pub fn meter(&self) -> Meter {
        Meter {
            room: self.clone(),
            unchecked: RECHECK_EVERY,
        }
    }

    pub fn guard<W: Write + AsFd>(&self, inner: W, need: u64) -> Result<Guarded<W>> {
        self.ensure(inner.as_fd(), need)?;
        Ok(Guarded {
            inner,
            meter: Meter {
                room: self.clone(),
                unchecked: 0,
            },
        })
    }
}

pub struct Meter {
    room: Room,
    unchecked: u64,
}

impl Meter {
    pub fn before_write(&mut self, fd: BorrowedFd<'_>, len: u64) -> Result<()> {
        if self.unchecked >= RECHECK_EVERY {
            self.room.ensure(fd, len)?;
            self.unchecked = 0;
        }
        self.unchecked = self.unchecked.saturating_add(len);
        Ok(())
    }
}

pub struct Guarded<W> {
    inner: W,
    meter: Meter,
}

impl<W: Write + AsFd> Write for Guarded<W> {
    fn write(&mut self, buf: &[u8]) -> io::Result<usize> {
        self.meter
            .before_write(self.inner.as_fd(), buf.len() as u64)
            .map_err(|e| io::Error::new(io::ErrorKind::StorageFull, e))?;
        self.inner.write(buf)
    }

    fn flush(&mut self) -> io::Result<()> {
        self.inner.flush()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs::File;
    use std::sync::atomic::{AtomicU64, Ordering};

    fn info(free: u64, total: u64) -> VfsInfo {
        VfsInfo {
            free,
            total,
            used: total - free,
            read_only: false,
            name_max: 255,
        }
    }

    fn fixed(free: u64, total: u64) -> Room {
        Room::new(100, Arc::new(move |_| Ok(info(free, total))))
    }

    fn scratch() -> File {
        tempfile::tempfile().unwrap()
    }

    fn kind(r: Result<()>) -> Option<ErrorKind> {
        r.err().map(|e| e.kind)
    }

    #[test]
    fn ensure_keeps_the_reserve_free() {
        let f = scratch();
        assert_eq!(
            kind(fixed(150, 10_000).ensure(f.as_fd(), 50)),
            None,
            "exactly fits"
        );
        assert_eq!(
            kind(fixed(149, 10_000).ensure(f.as_fd(), 50)),
            Some(ErrorKind::NoSpace)
        );
        assert_eq!(
            kind(fixed(1000, 10_000).ensure(f.as_fd(), u64::MAX)),
            Some(ErrorKind::NoSpace)
        );
    }

    #[test]
    fn small_filesystems_keep_a_tenth_free() {
        let f = scratch();
        assert_eq!(kind(fixed(60, 500).ensure(f.as_fd(), 10)), None);
        assert_eq!(
            kind(fixed(59, 500).ensure(f.as_fd(), 10)),
            Some(ErrorKind::NoSpace)
        );
    }

    #[test]
    fn default_keeps_256_mib_free() {
        let r = Room::default();
        assert_eq!(r.keep_free, KEEP_FREE);
        assert_eq!(KEEP_FREE, 256 << 20);
        assert!(r.ensure(scratch().as_fd(), u64::MAX / 2).is_err());
        assert!(format!("{r:?}").contains(&KEEP_FREE.to_string()));
    }

    #[test]
    fn probe_failure_is_an_error() {
        let r = Room::new(
            0,
            Arc::new(|_| Err(io::Error::from(io::ErrorKind::PermissionDenied))),
        );
        assert_eq!(
            kind(r.ensure(scratch().as_fd(), 0)),
            Some(ErrorKind::PermissionDenied)
        );
    }

    #[test]
    fn guard_refuses_before_writing_when_the_whole_file_cannot_fit() {
        let f = scratch();
        let err = fixed(149, 10_000).guard(&f, 50).err();
        assert_eq!(err.map(|e| e.kind), Some(ErrorKind::NoSpace));
    }

    #[test]
    fn guard_stops_writing_once_free_space_runs_out() {
        let free = Arc::new(AtomicU64::new(1 << 30));
        let calls = Arc::new(AtomicU64::new(0));
        let (f2, c2) = (free.clone(), calls.clone());
        let room = Room::new(
            1 << 20,
            Arc::new(move |_| {
                c2.fetch_add(1, Ordering::SeqCst);
                Ok(info(f2.load(Ordering::SeqCst), 1 << 40))
            }),
        );
        let f = scratch();
        let mut w = room.guard(&f, 0).unwrap();
        let chunk = vec![1u8; 1 << 20];
        for _ in 0..(RECHECK_EVERY >> 20) {
            w.write_all(&chunk).unwrap();
        }
        assert_eq!(
            calls.load(Ordering::SeqCst),
            1,
            "no probe before {RECHECK_EVERY} bytes"
        );
        free.store((1 << 20) + (1 << 20) - 1, Ordering::SeqCst);
        let err = w.write_all(&chunk).unwrap_err();
        assert_eq!(err.kind(), io::ErrorKind::StorageFull);
        let inner = err
            .get_ref()
            .and_then(|e| e.downcast_ref::<Error>())
            .map(|e| e.kind);
        assert_eq!(inner, Some(ErrorKind::NoSpace));
        assert_eq!(
            f.metadata().unwrap().len(),
            RECHECK_EVERY,
            "nothing written past the check"
        );
        free.store(1 << 30, Ordering::SeqCst);
        w.write_all(&chunk).unwrap();
        w.flush().unwrap();
        assert_eq!(f.metadata().unwrap().len(), RECHECK_EVERY + (1 << 20));
    }

    #[test]
    fn meter_checks_on_first_write_then_every_step() {
        let calls = Arc::new(AtomicU64::new(0));
        let c2 = calls.clone();
        let room = Room::new(
            0,
            Arc::new(move |_| {
                c2.fetch_add(1, Ordering::SeqCst);
                Ok(info(1 << 30, 1 << 40))
            }),
        );
        let f = scratch();
        let mut m = room.meter();
        m.before_write(f.as_fd(), 1).unwrap();
        assert_eq!(calls.load(Ordering::SeqCst), 1);
        m.before_write(f.as_fd(), RECHECK_EVERY - 2).unwrap();
        m.before_write(f.as_fd(), 1).unwrap();
        assert_eq!(calls.load(Ordering::SeqCst), 1, "below the step");
        m.before_write(f.as_fd(), 1).unwrap();
        assert_eq!(calls.load(Ordering::SeqCst), 2, "step reached");
    }
}
