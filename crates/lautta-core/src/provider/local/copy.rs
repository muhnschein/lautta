// SPDX-License-Identifier: LGPL-2.1-or-later
//! Local file copy strategy (SPEC XFR-3): `FICLONE`, then `copy_file_range`,
//! then a buffered loop. Each step falls back to the next on any error and
//! the last one reports the real error.

use crate::provider::ProgressSink;
use crate::sys;
use std::fs::File;
use std::io::{self, Read, Write};

const BUF: usize = 256 * 1024;

/// Copies from the current position of `src` to the current position of
/// `dst`. `base` is the number of bytes already in `dst` (resume) and only
/// shifts the progress numbers; `total` is the final size when known.
/// `try_clone` allows a whole-file reflink and is only valid when both
/// positions are 0. Returns the bytes written by this call.
pub fn copy_fd(
    src: &File,
    dst: &File,
    base: u64,
    total: Option<u64>,
    try_clone: bool,
    progress: &ProgressSink,
) -> io::Result<u64> {
    if try_clone && sys::clone_file(dst, src).is_ok() {
        let size = src.metadata()?.len();
        progress(size, Some(size));
        return Ok(size);
    }
    let mut copied = 0u64;
    let kernel = sys::copy_range_all(src, dst, &mut |t| {
        copied = t;
        progress(base + t, total);
    });
    if kernel.is_ok() {
        return Ok(copied);
    }
    let done = copied;
    let rest = copy_buffered(src, dst, &mut |t| progress(base + done + t, total))?;
    Ok(done + rest)
}

/// Plain read/write loop from the current positions until end of file.
pub fn copy_buffered(mut src: &File, mut dst: &File, progress: &mut dyn FnMut(u64)) -> io::Result<u64> {
    let mut buf = vec![0u8; BUF];
    let mut total = 0u64;
    loop {
        let n = match src.read(&mut buf) {
            Ok(0) => return Ok(total),
            Ok(n) => n,
            Err(e) if e.kind() == io::ErrorKind::Interrupted => continue,
            Err(e) => return Err(e),
        };
        dst.write_all(&buf[..n])?;
        total += n as u64;
        progress(total);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::provider::no_progress;
    use std::io::{Seek, SeekFrom};
    use std::sync::{Arc, Mutex};

    fn pattern(len: usize) -> Vec<u8> {
        (0..len).map(|i| (i * 7 % 253) as u8).collect()
    }

    type ProgressLog = Arc<Mutex<Vec<(u64, Option<u64>)>>>;

    fn recorder() -> (ProgressSink, ProgressLog) {
        let log = Arc::new(Mutex::new(Vec::new()));
        let l2 = log.clone();
        (Arc::new(move |d, t| l2.lock().unwrap().push((d, t))), log)
    }

    #[test]
    fn copies_whole_file_and_reports_progress() {
        let d = tempfile::tempdir().unwrap();
        let data = pattern(1_000_000);
        std::fs::write(d.path().join("s"), &data).unwrap();
        let src = File::open(d.path().join("s")).unwrap();
        let dst = File::create(d.path().join("d")).unwrap();
        let (sink, log) = recorder();
        let n = copy_fd(&src, &dst, 0, Some(1_000_000), true, &sink).unwrap();
        assert_eq!(n, 1_000_000);
        assert_eq!(std::fs::read(d.path().join("d")).unwrap(), data);
        let log = log.lock().unwrap();
        assert_eq!(log.last(), Some(&(1_000_000, Some(1_000_000))));
        assert!(log.windows(2).all(|w| w[0].0 <= w[1].0));
    }

    #[test]
    fn empty_file_copies() {
        let d = tempfile::tempdir().unwrap();
        std::fs::write(d.path().join("s"), "").unwrap();
        let src = File::open(d.path().join("s")).unwrap();
        let dst = File::create(d.path().join("d")).unwrap();
        assert_eq!(copy_fd(&src, &dst, 0, Some(0), true, &no_progress()).unwrap(), 0);
        assert_eq!(std::fs::metadata(d.path().join("d")).unwrap().len(), 0);
    }

    #[test]
    fn progress_is_shifted_by_the_resume_base() {
        let d = tempfile::tempdir().unwrap();
        let data = pattern(5000);
        std::fs::write(d.path().join("s"), &data).unwrap();
        std::fs::write(d.path().join("d"), &data[..2000]).unwrap();
        let mut src = File::open(d.path().join("s")).unwrap();
        src.seek(SeekFrom::Start(2000)).unwrap();
        let mut dst = std::fs::OpenOptions::new()
            .write(true)
            .open(d.path().join("d"))
            .unwrap();
        dst.seek(SeekFrom::Start(2000)).unwrap();
        let (sink, log) = recorder();
        let n = copy_fd(&src, &dst, 2000, Some(5000), false, &sink).unwrap();
        assert_eq!(n, 3000);
        assert_eq!(std::fs::read(d.path().join("d")).unwrap(), data);
        assert_eq!(log.lock().unwrap().last(), Some(&(5000, Some(5000))));
    }

    #[test]
    fn buffered_copy_works_on_its_own() {
        let d = tempfile::tempdir().unwrap();
        let data = pattern(600_000);
        std::fs::write(d.path().join("s"), &data).unwrap();
        let src = File::open(d.path().join("s")).unwrap();
        let dst = File::create(d.path().join("d")).unwrap();
        let mut last = 0;
        assert_eq!(copy_buffered(&src, &dst, &mut |t| last = t).unwrap(), 600_000);
        assert_eq!(last, 600_000);
        assert_eq!(std::fs::read(d.path().join("d")).unwrap(), data);
    }

    #[test]
    fn pipes_fall_back_to_the_buffered_path() {
        let d = tempfile::tempdir().unwrap();
        let (r, w) = rustix::pipe::pipe().unwrap();
        let data = pattern(40_000);
        let mut wf = File::from(w);
        wf.write_all(&data).unwrap();
        drop(wf);
        let src = File::from(r);
        let dst = File::create(d.path().join("d")).unwrap();
        let (sink, log) = recorder();
        let n = copy_fd(&src, &dst, 0, None, true, &sink).unwrap();
        assert_eq!(n, 40_000);
        assert_eq!(std::fs::read(d.path().join("d")).unwrap(), data);
        assert_eq!(log.lock().unwrap().last(), Some(&(40_000, None)));
    }

    #[test]
    fn read_errors_are_reported() {
        let d = tempfile::tempdir().unwrap();
        let src = File::open(d.path()).unwrap();
        let dst = File::create(d.path().join("d")).unwrap();
        assert!(copy_fd(&src, &dst, 0, None, true, &no_progress()).is_err());
    }
}
