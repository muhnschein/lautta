// SPDX-License-Identifier: LGPL-2.1-or-later
//! `SearchTestSupport`: what host QML tests need to exercise models against
//! real folders: files below `$HOME` and a way to let queued results arrive
//! (the test runner only spins the event loop briefly). Harmless in the app;
//! no page uses it.

use cpp::cpp;
use qmetaobject::prelude::*;
use std::path::PathBuf;

cpp! {{
    #include <QtCore/QCoreApplication>
    #include <QtCore/QElapsedTimer>
    #include <QtCore/QThread>
}}

#[derive(QObject, Default)]
pub struct SearchTestSupport {
    base: qt_base_class!(trait QObject),
    /// Lets queued results arrive for about `ms` milliseconds.
    spin: qt_method!(fn(&self, ms: i32)),
    /// Makes sure the user folders tests refer to exist and are locations
    /// (the SDK check runs with an empty home inside the build sandbox).
    prepare: qt_method!(fn(&self)),
    /// Writes `text` to `rel` below `$HOME`, creating folders; `mtimeSecs`
    /// above zero sets the modification time.
    write: qt_method!(fn(&self, rel: QString, text: QString, mtime_secs: i64) -> bool),
    exists: qt_method!(fn(&self, rel: QString) -> bool),
}

fn path_of(rel: &QString) -> Option<PathBuf> {
    let home = PathBuf::from(std::env::var_os("HOME")?);
    let rel = rel.to_string();
    // Tests stay below $HOME.
    if rel.split('/').any(|c| c == "..") {
        return None;
    }
    Some(home.join(rel.trim_start_matches('/')))
}

impl SearchTestSupport {
    fn spin(&self, ms: i32) {
        // SAFETY: plain Qt calls on the thread that owns the event loop.
        unsafe {
            cpp!([ms as "int"] {
                QElapsedTimer t;
                t.start();
                while (t.elapsed() < ms) {
                    QCoreApplication::processEvents(QEventLoop::AllEvents, 10);
                    QThread::msleep(2);
                }
            })
        }
    }

    fn prepare(&self) {
        for dir in ["Documents", "Downloads", "Pictures"] {
            if let Some(path) = path_of(&QString::from(dir)) {
                let _ = std::fs::create_dir_all(path);
            }
        }
        if let Some(core) = crate::runtime::core() {
            core.locations.refresh();
        }
    }

    fn write(&self, rel: QString, text: QString, mtime_secs: i64) -> bool {
        let Some(path) = path_of(&rel) else {
            return false;
        };
        if let Some(parent) = path.parent() {
            if std::fs::create_dir_all(parent).is_err() {
                return false;
            }
        }
        if std::fs::write(&path, text.to_string()).is_err() {
            return false;
        }
        match u64::try_from(mtime_secs).ok().filter(|s| *s > 0) {
            Some(secs) => std::fs::File::options()
                .write(true)
                .open(&path)
                .and_then(|f| f.set_modified(std::time::UNIX_EPOCH + std::time::Duration::from_secs(secs)))
                .is_ok(),
            None => true,
        }
    }

    fn exists(&self, rel: QString) -> bool {
        path_of(&rel).is_some_and(|p| p.exists())
    }
}
