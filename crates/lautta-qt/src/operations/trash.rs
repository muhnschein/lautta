// SPDX-License-Identifier: LGPL-2.1-or-later
//! `TrashModel`: *Recently deleted* (OPS-8): list, restore, delete now,
//! empty. Call `reload()` when the page opens.

use super::error_parts;
use crate::runtime::{core, spawn_then};
use lautta_core::app_operations::TrashEntry;
use lautta_core::{Error, Result};
use qmetaobject::prelude::*;
use qmetaobject::QPointer;
use std::collections::HashMap;

const ROLE_ID: i32 = 0x100;
const ROLE_NAME: i32 = 0x101;
const ROLE_ORIGINAL: i32 = 0x102;
const ROLE_FOLDER_URI: i32 = 0x103;
const ROLE_FOLDER_NAME: i32 = 0x104;
const ROLE_IS_DIR: i32 = 0x105;
const ROLE_SIZE: i32 = 0x106;
const ROLE_TRASHED_AT: i32 = 0x107;
const ROLE_DAYS_LEFT: i32 = 0x108;

fn now_secs() -> i64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_or(0, |d| i64::try_from(d.as_secs()).unwrap_or(i64::MAX))
}

/// Total bytes of the entries (unknown sizes count as zero).
fn total_size(entries: &[TrashEntry]) -> i64 {
    entries
        .iter()
        .map(|e| i64::try_from(e.size.unwrap_or(0)).unwrap_or(i64::MAX))
        .fold(0i64, i64::saturating_add)
}

#[derive(QObject, Default)]
pub struct TrashModel {
    base: qt_base_class!(trait QAbstractListModel),
    count: qt_property!(i32; NOTIFY changed),
    totalSize: qt_property!(i64; NOTIFY changed),
    loaded: qt_property!(bool; NOTIFY changed),
    busy: qt_property!(bool; NOTIFY changed),
    changed: qt_signal!(),

    reload: qt_method!(fn(&mut self)),
    restore: qt_method!(fn(&mut self, id: i64)),
    remove: qt_method!(fn(&mut self, id: i64)),
    empty: qt_method!(fn(&mut self)),

    restored: qt_signal!(id: i64, uri: QString),
    removedItem: qt_signal!(id: i64),
    emptied: qt_signal!(count: i32),
    failed: qt_signal!(kind: QString, message: QString),

    entries: Vec<TrashEntry>,
}

impl QAbstractListModel for TrashModel {
    fn row_count(&self) -> i32 {
        i32::try_from(self.entries.len()).unwrap_or(i32::MAX)
    }

    fn data(&self, index: QModelIndex, role: i32) -> QVariant {
        let Some(e) = usize::try_from(index.row())
            .ok()
            .and_then(|i| self.entries.get(i))
        else {
            return QVariant::default();
        };
        let text = |s: &str| -> QVariant { QString::from(s).into() };
        match role {
            ROLE_ID => e.id.into(),
            ROLE_NAME => text(&e.name),
            ROLE_ORIGINAL => text(&e.original_uri),
            ROLE_FOLDER_URI => text(&e.folder_uri),
            ROLE_FOLDER_NAME => text(&e.folder_name),
            ROLE_IS_DIR => e.is_dir.into(),
            ROLE_SIZE => e.size.and_then(|s| i64::try_from(s).ok()).unwrap_or(-1).into(),
            ROLE_TRASHED_AT => e.trashed_at.into(),
            ROLE_DAYS_LEFT => e.days_left.into(),
            _ => QVariant::default(),
        }
    }

    fn role_names(&self) -> HashMap<i32, QByteArray> {
        let mut names = HashMap::new();
        names.insert(ROLE_ID, "trashId".into());
        names.insert(ROLE_NAME, "name".into());
        names.insert(ROLE_ORIGINAL, "originalUri".into());
        names.insert(ROLE_FOLDER_URI, "folderUri".into());
        names.insert(ROLE_FOLDER_NAME, "folderName".into());
        names.insert(ROLE_IS_DIR, "isDir".into());
        names.insert(ROLE_SIZE, "size".into());
        names.insert(ROLE_TRASHED_AT, "trashedAt".into());
        names.insert(ROLE_DAYS_LEFT, "daysLeft".into());
        names
    }
}

impl TrashModel {
    fn reload(&mut self) {
        let Some(core) = core() else { return };
        let me = QPointer::from(&*self);
        spawn_then(
            async move {
                let now = now_secs();
                tokio::task::spawn_blocking(move || core.trash_entries(now))
                    .await
                    .unwrap_or_else(|e| Err(Error::new(lautta_core::ErrorKind::Internal, e.to_string())))
            },
            move |res| {
                let Some(p) = me.as_pinned() else { return };
                match res {
                    Ok(entries) => {
                        p.borrow_mut().replace(entries);
                        p.borrow().changed();
                    }
                    Err(e) => p.borrow().report(&e),
                }
            },
        );
    }

    fn replace(&mut self, entries: Vec<TrashEntry>) {
        self.begin_reset_model();
        self.entries = entries;
        self.end_reset_model();
        self.count = i32::try_from(self.entries.len()).unwrap_or(i32::MAX);
        self.totalSize = total_size(&self.entries);
        self.loaded = true;
    }

    fn report(&self, e: &Error) {
        let (kind, message) = error_parts(e);
        self.failed(kind, message);
    }

    fn set_busy(&mut self, busy: bool) {
        self.busy = busy;
        self.changed();
    }

    /// Runs `work` off the GUI thread, then tells the page and reloads.
    fn run<T, F>(&mut self, work: F, done: impl FnOnce(&TrashModel, T) + 'static)
    where
        T: Send + 'static,
        F: FnOnce(std::sync::Arc<lautta_core::app::Core>) -> Result<T> + Send + 'static,
    {
        let Some(core) = core() else { return };
        self.set_busy(true);
        let me = QPointer::from(&*self);
        spawn_then(
            async move {
                tokio::task::spawn_blocking(move || work(core))
                    .await
                    .unwrap_or_else(|e| Err(Error::new(lautta_core::ErrorKind::Internal, e.to_string())))
            },
            move |res| {
                let Some(p) = me.as_pinned() else { return };
                p.borrow_mut().busy = false;
                match res {
                    Ok(v) => done(p.borrow(), v),
                    Err(e) => p.borrow().report(&e),
                }
                p.borrow().changed();
                p.borrow_mut().reload();
            },
        );
    }

    fn restore(&mut self, id: i64) {
        let Some(core) = core() else { return };
        self.set_busy(true);
        let me = QPointer::from(&*self);
        spawn_then(async move { core.restore(id).await }, move |res| {
            let Some(p) = me.as_pinned() else { return };
            p.borrow_mut().busy = false;
            match res {
                Ok(uri) => p.borrow().restored(id, QString::from(uri.to_string().as_str())),
                Err(e) => p.borrow().report(&e),
            }
            p.borrow().changed();
            p.borrow_mut().reload();
        });
    }

    /// "Delete now" for one item (after its remorse).
    fn remove(&mut self, id: i64) {
        self.run(
            move |core| core.trash.delete(id),
            move |this, ()| this.removedItem(id),
        );
    }

    /// Empties Recently deleted (after the remorse popup).
    fn empty(&mut self) {
        self.run(
            |core| core.trash.empty(),
            |this, n| this.emptied(i32::try_from(n).unwrap_or(i32::MAX)),
        );
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn entry(size: Option<u64>) -> TrashEntry {
        TrashEntry {
            id: 1,
            name: "n".into(),
            original_uri: "lautta://user-documents/n".into(),
            folder_uri: "lautta://user-documents/".into(),
            folder_name: "Documents".into(),
            is_dir: false,
            size,
            trashed_at: 0,
            days_left: 30,
        }
    }

    #[test]
    fn sizes_add_up_and_unknown_counts_as_zero() {
        assert_eq!(total_size(&[]), 0);
        assert_eq!(total_size(&[entry(Some(5)), entry(None), entry(Some(7))]), 12);
    }
}
