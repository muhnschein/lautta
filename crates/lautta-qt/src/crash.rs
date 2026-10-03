// SPDX-License-Identifier: LGPL-2.1-or-later
//! Crash reports from earlier runs, shown at start with *Copy report* (RS-4).

use lautta_core::crash::CrashReports;
use lautta_core::paths::AppPaths;

fn reports(paths: &AppPaths) -> CrashReports {
    CrashReports::new(paths.crash_dir())
}

pub fn pending_count(paths: &AppPaths) -> usize {
    reports(paths).pending_reports().map(|r| r.len()).unwrap_or(0)
}

/// The newest report's text, or empty.
pub fn latest(paths: &AppPaths) -> String {
    let r = reports(paths);
    r.pending_reports()
        .ok()
        .and_then(|list| list.last().cloned())
        .and_then(|name| r.read(&name).ok())
        .unwrap_or_default()
}

pub fn dismiss_all(paths: &AppPaths) {
    let _ = reports(paths).dismiss_all();
}
