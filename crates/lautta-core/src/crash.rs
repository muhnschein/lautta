// SPDX-License-Identifier: LGPL-2.1-or-later
//! Crash reports (RS-4): release builds abort on panic, so a panic hook
//! writes a plain-text report first. Reports hold the time, version, thread,
//! panic message, source location and backtrace, never file contents or
//! paths from the user's data, and live in the private crash folder (0700,
//! files 0600). The next start offers them to the user; nothing is sent
//! anywhere.

use crate::error::{Error, ErrorKind, Result};
use std::any::Any;
use std::fs::OpenOptions;
use std::io::Write;
use std::os::unix::fs::{DirBuilderExt, OpenOptionsExt};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};

/// Older reports beyond this many are deleted when a new one is written.
pub const MAX_REPORTS: usize = 10;
/// Panic messages can embed data; keep only the start.
const MAX_MESSAGE_BYTES: usize = 2_000;
const PREFIX: &str = "crash-";
const SUFFIX: &str = ".txt";

static SEQUENCE: AtomicU64 = AtomicU64::new(0);

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CrashInfo {
    pub timestamp: String,
    pub version: String,
    pub thread: String,
    pub message: String,
    pub location: Option<String>,
    pub backtrace: String,
}

/// The text of the panic payload (`&str` or `String`), truncated on a
/// character boundary.
pub fn payload_message(payload: &(dyn Any + Send)) -> String {
    let raw = payload
        .downcast_ref::<&str>()
        .map(|s| (*s).to_owned())
        .or_else(|| payload.downcast_ref::<String>().cloned())
        .unwrap_or_else(|| "panic with a non-text payload".to_owned());
    truncate(&raw, MAX_MESSAGE_BYTES)
}

fn truncate(s: &str, max: usize) -> String {
    if s.len() <= max {
        return s.to_owned();
    }
    let mut end = max;
    while !s.is_char_boundary(end) {
        end -= 1;
    }
    format!("{}\u{2026}", &s[..end])
}

impl CrashInfo {
    /// Collects the facts about the current panic. The backtrace is forced
    /// even without `RUST_BACKTRACE`.
    pub fn capture(
        version: &str,
        payload: &(dyn Any + Send),
        location: Option<(&str, u32, u32)>,
    ) -> CrashInfo {
        let thread = std::thread::current();
        CrashInfo {
            timestamp: chrono::Utc::now().to_rfc3339_opts(chrono::SecondsFormat::Secs, true),
            version: version.to_owned(),
            thread: thread.name().unwrap_or("<unnamed>").to_owned(),
            message: payload_message(payload),
            location: location.map(|(file, line, col)| format!("{file}:{line}:{col}")),
            backtrace: std::backtrace::Backtrace::force_capture().to_string(),
        }
    }

    /// The report file's text.
    pub fn format(&self) -> String {
        format!(
            "Lautta crash report\n\
             Time: {}\n\
             Version: {}\n\
             Thread: {}\n\
             Message: {}\n\
             Location: {}\n\
             \n\
             Backtrace:\n{}\n",
            self.timestamp,
            self.version,
            self.thread,
            self.message,
            self.location.as_deref().unwrap_or("unknown"),
            self.backtrace
        )
    }
}

/// Writes `info` as a new report in `dir` and prunes old ones. Returns the
/// report's file name.
pub fn write_report(dir: &Path, info: &CrashInfo) -> Result<String> {
    std::fs::DirBuilder::new()
        .recursive(true)
        .mode(0o700)
        .create(dir)?;
    let stamp = chrono::Utc::now().format("%Y%m%dT%H%M%S");
    let seq = SEQUENCE.fetch_add(1, Ordering::Relaxed);
    let name = format!("{PREFIX}{stamp}-{}-{seq:04}{SUFFIX}", std::process::id());
    let mut file = OpenOptions::new()
        .write(true)
        .create_new(true)
        .mode(0o600)
        .open(dir.join(&name))?;
    file.write_all(info.format().as_bytes())?;
    file.sync_all()?;
    prune(dir, MAX_REPORTS);
    Ok(name)
}

/// Keeps the newest `keep` reports. Failures are ignored: pruning is a
/// courtesy and must not hide the report just written.
fn prune(dir: &Path, keep: usize) {
    let Ok(names) = list_names(dir) else {
        return;
    };
    for old in names.iter().skip(keep) {
        let _ = std::fs::remove_file(dir.join(old));
    }
}

/// Report names, newest first (names embed a sortable timestamp).
fn list_names(dir: &Path) -> Result<Vec<String>> {
    let rd = match std::fs::read_dir(dir) {
        Ok(rd) => rd,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(Vec::new()),
        Err(e) => return Err(e.into()),
    };
    let mut names: Vec<String> = rd
        .filter_map(Result::ok)
        .filter_map(|e| e.file_name().into_string().ok())
        .filter(|n| is_report_name(n))
        .collect();
    names.sort_unstable_by(|a, b| b.cmp(a));
    Ok(names)
}

/// Only plain names of our own pattern: this keeps `read` and `dismiss`
/// from reaching outside the crash folder.
fn is_report_name(name: &str) -> bool {
    name.starts_with(PREFIX)
        && name.ends_with(SUFFIX)
        && name
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || matches!(b, b'-' | b'_' | b'.'))
        && !name.contains("..")
}

fn checked_name(name: &str) -> Result<()> {
    if is_report_name(name) {
        Ok(())
    } else {
        Err(Error::new(ErrorKind::InvalidName, "not a crash report name"))
    }
}

/// The crash folder, as the UI sees it at start-up (RS-4).
#[derive(Debug, Clone)]
pub struct CrashReports {
    dir: PathBuf,
}

impl CrashReports {
    pub fn new(dir: impl Into<PathBuf>) -> CrashReports {
        CrashReports { dir: dir.into() }
    }

    /// Reports not yet dismissed, newest first.
    pub fn pending_reports(&self) -> Result<Vec<String>> {
        list_names(&self.dir)
    }

    pub fn read(&self, report: &str) -> Result<String> {
        checked_name(report)?;
        Ok(std::fs::read_to_string(self.dir.join(report))?)
    }

    /// Deletes one report; a missing one is fine.
    pub fn dismiss(&self, report: &str) -> Result<()> {
        checked_name(report)?;
        match std::fs::remove_file(self.dir.join(report)) {
            Err(e) if e.kind() != std::io::ErrorKind::NotFound => Err(e.into()),
            _ => Ok(()),
        }
    }

    pub fn dismiss_all(&self) -> Result<usize> {
        let names = self.pending_reports()?;
        for name in &names {
            self.dismiss(name)?;
        }
        Ok(names.len())
    }
}

/// Installs the panic hook. The previous hook (the default one prints to
/// stderr) still runs afterwards. Writing is best effort: a failure inside a
/// panic hook cannot be reported anywhere useful.
pub fn install(dir: PathBuf, version: &'static str) {
    let previous = std::panic::take_hook();
    std::panic::set_hook(Box::new(move |info| {
        let location = info.location().map(|l| (l.file(), l.line(), l.column()));
        let report = CrashInfo::capture(version, info.payload(), location);
        let _ = write_report(&dir, &report);
        previous(info);
    }));
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::os::unix::fs::PermissionsExt;

    fn info(message: &str) -> CrashInfo {
        CrashInfo {
            timestamp: "2026-10-03T12:00:00Z".into(),
            version: "0.1.0".into(),
            thread: "main".into(),
            message: message.into(),
            location: Some("src/x.rs:1:2".into()),
            backtrace: "0: frame".into(),
        }
    }

    #[test]
    fn format_has_every_field() {
        let text = info("boom").format();
        assert!(text.starts_with("Lautta crash report\n"));
        for needle in [
            "Time: 2026-10-03T12:00:00Z",
            "Version: 0.1.0",
            "Thread: main",
            "Message: boom",
            "Location: src/x.rs:1:2",
            "Backtrace:\n0: frame",
        ] {
            assert!(text.contains(needle), "missing {needle}");
        }
        let mut unknown = info("x");
        unknown.location = None;
        assert!(unknown.format().contains("Location: unknown"));
    }

    #[test]
    fn payloads_are_text_and_truncated() {
        assert_eq!(payload_message(&"static"), "static");
        assert_eq!(payload_message(&String::from("owned")), "owned");
        assert_eq!(payload_message(&42u32), "panic with a non-text payload");
        let long = "é".repeat(2_000);
        let msg = payload_message(&long);
        assert!(msg.ends_with('\u{2026}'));
        assert!(msg.len() <= MAX_MESSAGE_BYTES + 3);
        assert!(msg.starts_with("éé"));
    }

    #[test]
    fn capture_records_thread_and_location() {
        let handle = std::thread::Builder::new()
            .name("worker-7".into())
            .spawn(|| CrashInfo::capture("9.9", &"msg", Some(("a.rs", 3, 4))))
            .unwrap();
        let got = handle.join().unwrap();
        assert_eq!(got.thread, "worker-7");
        assert_eq!(got.version, "9.9");
        assert_eq!(got.message, "msg");
        assert_eq!(got.location.as_deref(), Some("a.rs:3:4"));
        assert!(got.timestamp.ends_with('Z'));
        assert!(!got.backtrace.is_empty());
    }

    #[test]
    fn reports_are_private_and_round_trip() {
        let tmp = tempfile::tempdir().unwrap();
        let dir = tmp.path().join("cache/crash");
        let name = write_report(&dir, &info("boom")).unwrap();
        assert!(is_report_name(&name));
        let mode = |p: &Path| std::fs::metadata(p).unwrap().permissions().mode() & 0o777;
        assert_eq!(mode(&dir), 0o700);
        assert_eq!(mode(&dir.join(&name)), 0o600);
        let reports = CrashReports::new(&dir);
        assert_eq!(reports.pending_reports().unwrap(), [name.clone()]);
        assert!(reports.read(&name).unwrap().contains("Message: boom"));
        reports.dismiss(&name).unwrap();
        reports.dismiss(&name).unwrap();
        assert!(reports.pending_reports().unwrap().is_empty());
    }

    #[test]
    fn missing_folder_means_no_reports() {
        let tmp = tempfile::tempdir().unwrap();
        let reports = CrashReports::new(tmp.path().join("nope"));
        assert!(reports.pending_reports().unwrap().is_empty());
        assert_eq!(reports.read("crash-x.txt").unwrap_err().kind, ErrorKind::NotFound);
    }

    #[test]
    fn names_cannot_escape_the_folder() {
        let tmp = tempfile::tempdir().unwrap();
        std::fs::write(tmp.path().join("secret.txt"), "s").unwrap();
        let reports = CrashReports::new(tmp.path().join("crash"));
        for bad in [
            "../secret.txt",
            "secret.txt",
            "crash-/../x.txt",
            "crash-a/b.txt",
            "crash-..txt",
            "",
        ] {
            assert_eq!(
                reports.read(bad).unwrap_err().kind,
                ErrorKind::InvalidName,
                "{bad}"
            );
            assert_eq!(
                reports.dismiss(bad).unwrap_err().kind,
                ErrorKind::InvalidName,
                "{bad}"
            );
        }
        assert!(tmp.path().join("secret.txt").exists());
    }

    #[test]
    fn only_the_newest_reports_are_kept_and_listed_newest_first() {
        let tmp = tempfile::tempdir().unwrap();
        let dir = tmp.path().join("crash");
        for _ in 0..MAX_REPORTS + 3 {
            write_report(&dir, &info("x")).unwrap();
        }
        let reports = CrashReports::new(&dir);
        let names = reports.pending_reports().unwrap();
        assert_eq!(names.len(), MAX_REPORTS);
        let mut sorted = names.clone();
        sorted.sort_unstable_by(|a, b| b.cmp(a));
        assert_eq!(names, sorted);
        // Sequence numbers only grow within this folder, so the first name
        // (newest) carries a higher one than the last.
        let seq = |n: &str| {
            n.trim_end_matches(SUFFIX)
                .rsplit('-')
                .next()
                .unwrap()
                .parse::<u64>()
                .unwrap()
        };
        assert!(seq(&names[0]) > seq(&names[MAX_REPORTS - 1]));
        std::fs::write(dir.join("notes.txt"), "keep").unwrap();
        assert_eq!(reports.dismiss_all().unwrap(), MAX_REPORTS);
        assert!(dir.join("notes.txt").exists());
    }

    #[test]
    fn the_hook_writes_a_report_for_a_real_panic() {
        let tmp = tempfile::tempdir().unwrap();
        let dir = tmp.path().join("crash");
        install(dir.clone(), "test-version");
        let outcome = std::panic::catch_unwind(|| {
            panic!("hook test panic");
        });
        // Restore the default hook before any assertion can fail.
        let _ = std::panic::take_hook();
        assert!(outcome.is_err());
        let reports = CrashReports::new(&dir);
        let all: Vec<String> = reports
            .pending_reports()
            .unwrap()
            .iter()
            .map(|n| reports.read(n).unwrap())
            .collect();
        assert!(
            all.iter()
                .any(|t| t.contains("Message: hook test panic") && t.contains("Version: test-version")),
            "reports: {all:?}"
        );
    }
}
