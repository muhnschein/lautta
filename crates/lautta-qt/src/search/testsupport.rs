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
    /// `$HOME`.
    home: qt_method!(fn(&self) -> QString),
    /// True when the binary runs on an Arm target (the SDK check runs it
    /// under emulation, where local file copies fail).
    emulated: qt_method!(fn(&self) -> bool),
    /// State, failures and error of every transfer, for failure messages.
    transferStates: qt_method!(fn(&self) -> QString),
    /// Makes sure the user folders tests refer to exist and are locations
    /// (the SDK check runs with an empty home inside the build sandbox).
    prepare: qt_method!(fn(&self)),
    /// Writes `text` to `rel` below `$HOME`, creating folders; `mtimeSecs`
    /// above zero sets the modification time.
    write: qt_method!(fn(&self, rel: QString, text: QString, mtime_secs: i64) -> bool),
    mkdir: qt_method!(fn(&self, rel: QString) -> bool),
    remove: qt_method!(fn(&self, rel: QString) -> bool),
    exists: qt_method!(fn(&self, rel: QString) -> bool),
    read: qt_method!(fn(&self, rel: QString) -> QString),
    /// Size of the files below `rel` in bytes, -1 when missing.
    size: qt_method!(fn(&self, rel: QString) -> i64),
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

    fn transferStates(&self) -> QString {
        let text = crate::runtime::core()
            .map(|c| {
                c.engine
                    .list()
                    .iter()
                    .map(|t| format!("{:?} failed={} error={:?}", t.state, t.items_failed, t.error))
                    .collect::<Vec<_>>()
                    .join("; ")
            })
            .unwrap_or_default();
        QString::from(text.as_str())
    }

    fn emulated(&self) -> bool {
        cfg!(any(target_arch = "aarch64", target_arch = "arm"))
    }

    fn home(&self) -> QString {
        QString::from(std::env::var("HOME").unwrap_or_default().as_str())
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

    fn mkdir(&self, rel: QString) -> bool {
        path_of(&rel).is_some_and(|p| std::fs::create_dir_all(p).is_ok())
    }

    fn remove(&self, rel: QString) -> bool {
        path_of(&rel).is_some_and(|p| std::fs::remove_dir_all(&p).is_ok() || std::fs::remove_file(p).is_ok())
    }

    fn exists(&self, rel: QString) -> bool {
        path_of(&rel).is_some_and(|p| p.exists())
    }

    fn read(&self, rel: QString) -> QString {
        let text = path_of(&rel)
            .and_then(|p| std::fs::read_to_string(p).ok())
            .unwrap_or_default();
        QString::from(text.as_str())
    }

    fn size(&self, rel: QString) -> i64 {
        fn total(p: &std::path::Path) -> u64 {
            match std::fs::metadata(p) {
                Ok(m) if m.is_dir() => std::fs::read_dir(p)
                    .map(|d| d.filter_map(Result::ok).map(|e| total(&e.path())).sum())
                    .unwrap_or(0),
                Ok(m) => m.len(),
                Err(_) => 0,
            }
        }
        match path_of(&rel) {
            Some(p) if p.exists() => total(&p) as i64,
            _ => -1,
        }
    }
}
