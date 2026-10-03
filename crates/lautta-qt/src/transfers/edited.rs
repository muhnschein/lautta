// SPDX-License-Identifier: LGPL-2.1-or-later
//! `WorkingCopiesModel`: working copies of edited files (EDT-1..3): list,
//! pin, upload now, and resolving an edit conflict.

use super::events;
use crate::json::to_json;
use crate::runtime::{core, spawn_then};
use lautta_core::app::Core;
use lautta_core::app_transfers::{edit_conflict_json, parse_edit_choice, EditedFile, WriteBack};
use lautta_core::workcopy::{EditConflict, Purpose, Resolution};
use lautta_core::Error;
use qmetaobject::prelude::*;
use qmetaobject::QPointer;
use std::cell::Cell;
use std::collections::HashMap;

const ROLES: [&str; 12] = [
    "copyId",
    "name",
    "remote",
    "address",
    "locationName",
    "localPath",
    "pinned",
    "dirty",
    "localSize",
    "lastUploadMs",
    "purpose",
    "state",
];
const FIRST_ROLE: i32 = 257;

#[derive(Debug, Clone, PartialEq, Default)]
struct CopyData {
    id: i64,
    name: String,
    remote: String,
    address: String,
    location_name: String,
    local_path: String,
    pinned: bool,
    dirty: bool,
    local_size: i64,
    last_upload_ms: i64,
    purpose: &'static str,
}

impl CopyData {
    fn of(core: &Core, e: &EditedFile) -> CopyData {
        let c = &e.copy;
        CopyData {
            id: c.id,
            name: core.working_copy_name(c),
            remote: c.remote.to_string(),
            address: core.locations.display_address(&c.remote),
            location_name: core
                .location(&c.remote.location)
                .map(|l| l.name)
                .unwrap_or_default(),
            local_path: c.local_path.display().to_string(),
            pinned: c.pinned,
            dirty: e.dirty,
            local_size: i64::try_from(e.local_size).unwrap_or(i64::MAX),
            last_upload_ms: c.last_upload_ms.unwrap_or(-1),
            purpose: if c.purpose == Purpose::Edit {
                "edit"
            } else {
                "open"
            },
        }
    }

    fn value(&self, role: &str) -> QVariant {
        let s = |v: &str| QVariant::from(QString::from(v));
        match role {
            "copyId" => QVariant::from(self.id),
            "name" => s(&self.name),
            "remote" => s(&self.remote),
            "address" => s(&self.address),
            "locationName" => s(&self.location_name),
            "localPath" => s(&self.local_path),
            "pinned" => QVariant::from(self.pinned),
            "dirty" => QVariant::from(self.dirty),
            "localSize" => QVariant::from(self.local_size),
            "lastUploadMs" => QVariant::from(self.last_upload_ms),
            "purpose" => s(self.purpose),
            "state" => s(if self.dirty { "changed" } else { "watching" }),
            _ => QVariant::default(),
        }
    }
}

/// The edit conflict as JSON text for the dialog and the notification.
pub(super) fn conflict_text(core: &Core, c: &EditConflict) -> String {
    let name = c
        .remote
        .name()
        .map(lautta_core::vpath::display_name)
        .unwrap_or_default();
    let address = core.locations.display_address(&c.remote);
    to_json(&edit_conflict_json(c, &name, &address))
}

#[derive(QObject, Default)]
pub struct WorkingCopiesModel {
    base: qt_base_class!(trait QAbstractListModel),
    count: qt_property!(i32; NOTIFY countChanged),
    countChanged: qt_signal!(),
    refresh: qt_method!(fn(&mut self)),
    pin: qt_method!(fn(&self, copy_id: i64, pinned: bool)),
    uploadNow: qt_method!(fn(&self, copy_id: i64)),
    loadConflict: qt_method!(fn(&self, copy_id: i64)),
    resolve: qt_method!(fn(&self, copy_id: i64, choice: QString)),

    uploaded: qt_signal!(copyId: i64, name: QString),
    conflict: qt_signal!(copyId: i64, conflictJson: QString),
    /// Answer to `loadConflict`: the conflict as JSON, or empty when there is
    /// none any more.
    conflictLoaded: qt_signal!(copyId: i64, conflictJson: QString),
    /// `result` is `uploaded`, `savedCopy` (then `detail` is the copy's URI)
    /// or `discarded`.
    resolved: qt_signal!(copyId: i64, result: QString, detail: QString),
    failed: qt_signal!(copyId: i64, kind: QString, message: QString),

    rows: Vec<CopyData>,
    registered: Cell<bool>,
}

impl QAbstractListModel for WorkingCopiesModel {
    fn row_count(&self) -> i32 {
        self.rows.len() as i32
    }

    fn data(&self, index: QModelIndex, role: i32) -> QVariant {
        let row = usize::try_from(index.row()).ok().and_then(|i| self.rows.get(i));
        let name = usize::try_from(role - FIRST_ROLE).ok().and_then(|i| ROLES.get(i));
        match (row, name) {
            (Some(r), Some(n)) => r.value(n),
            _ => QVariant::default(),
        }
    }

    fn role_names(&self) -> HashMap<i32, QByteArray> {
        self.ensure_registered();
        ROLES
            .iter()
            .enumerate()
            .map(|(i, n)| (FIRST_ROLE + i as i32, QByteArray::from(*n)))
            .collect()
    }
}

fn report_error(me: &QPointer<WorkingCopiesModel>, id: i64, e: &Error) {
    if let Some(p) = me.as_pinned() {
        p.borrow().failed(
            id,
            QString::from(e.kind.name()),
            QString::from(e.message.as_str()),
        );
    }
}

impl WorkingCopiesModel {
    fn ensure_registered(&self) {
        if self.registered.replace(true) {
            return;
        }
        events::ensure_forwarder();
        let me = QPointer::from(self);
        events::listen(move |ev| match me.as_pinned() {
            Some(p) => {
                if events::is_reload(ev) {
                    p.borrow_mut().refresh();
                }
                true
            }
            None => false,
        });
        let me = QPointer::from(self);
        qmetaobject::single_shot(std::time::Duration::from_millis(0), move || {
            if let Some(p) = me.as_pinned() {
                p.borrow_mut().refresh();
            }
        });
    }

    fn refresh(&mut self) {
        let Some(core) = core() else { return };
        let me = QPointer::from(&*self);
        spawn_then(
            async move {
                let files = core.edited_files().await?;
                Ok::<_, Error>(files.iter().map(|e| CopyData::of(&core, e)).collect::<Vec<_>>())
            },
            move |res| {
                if let (Some(p), Ok(rows)) = (me.as_pinned(), res) {
                    p.borrow_mut().apply(rows);
                }
            },
        );
    }

    fn apply(&mut self, new: Vec<CopyData>) {
        let same = new.len() == self.rows.len() && new.iter().zip(&self.rows).all(|(a, b)| a.id == b.id);
        if same {
            for (i, row) in new.into_iter().enumerate() {
                if self.rows[i] != row {
                    self.rows[i] = row;
                    let ix = self.row_index(i as i32);
                    self.data_changed(ix, ix);
                }
            }
            return;
        }
        self.begin_reset_model();
        self.rows = new;
        self.end_reset_model();
        self.count = self.rows.len() as i32;
        self.countChanged();
    }

    /// EDT-3: keep or release a working copy (pinned copies are not removed
    /// 24 h after the last upload).
    fn pin(&self, copy_id: i64, pinned: bool) {
        let Some(core) = core() else { return };
        let me = QPointer::from(self);
        spawn_then(
            async move { core.working_copies.pin(copy_id, pinned).await },
            move |res| match res {
                Ok(()) => events::reload(),
                Err(e) => report_error(&me, copy_id, &e),
            },
        );
    }

    /// EDT-2 by hand: uploads when the remote is unchanged, otherwise asks
    /// through `conflict`.
    fn uploadNow(&self, copy_id: i64) {
        let Some(core) = core() else { return };
        let me = QPointer::from(self);
        spawn_then(
            async move {
                let out = core.write_back(copy_id).await?;
                let conflict = match &out {
                    WriteBack::Conflict(c) => Some(conflict_text(&core, c)),
                    _ => None,
                };
                let name = match &out {
                    WriteBack::Uploaded(c) => core.working_copy_name(c),
                    _ => String::new(),
                };
                Ok::<_, Error>((conflict, name, out))
            },
            move |res| {
                events::reload();
                match res {
                    Ok((Some(json), _, _)) => {
                        if let Some(p) = me.as_pinned() {
                            p.borrow().conflict(copy_id, QString::from(json.as_str()));
                        }
                    }
                    Ok((None, name, WriteBack::Uploaded(_))) => {
                        if let Some(p) = me.as_pinned() {
                            p.borrow().uploaded(copy_id, QString::from(name.as_str()));
                        }
                    }
                    Ok(_) => {}
                    Err(e) => report_error(&me, copy_id, &e),
                }
            },
        );
    }

    fn loadConflict(&self, copy_id: i64) {
        let Some(core) = core() else { return };
        let me = QPointer::from(self);
        spawn_then(
            async move {
                let c = core.edit_conflict(copy_id).await?;
                Ok::<_, Error>(c.map(|c| conflict_text(&core, &c)).unwrap_or_default())
            },
            move |res| match (res, me.as_pinned()) {
                (Ok(json), Some(p)) => p.borrow().conflictLoaded(copy_id, QString::from(json.as_str())),
                (Err(e), _) => report_error(&me, copy_id, &e),
                _ => {}
            },
        );
    }

    fn resolve(&self, copy_id: i64, choice: QString) {
        let Some(core) = core() else { return };
        let Some(choice) = parse_edit_choice(&choice.to_string()) else {
            let e = Error::new(lautta_core::ErrorKind::InvalidArgument, "unknown choice");
            report_error(&QPointer::from(self), copy_id, &e);
            return;
        };
        let me = QPointer::from(self);
        spawn_then(
            async move { core.resolve_edit_conflict(copy_id, choice).await },
            move |res| {
                events::reload();
                match (res, me.as_pinned()) {
                    (Ok(r), Some(p)) => {
                        let (what, detail) = match r {
                            Resolution::Uploaded(_) => ("uploaded", String::new()),
                            Resolution::SavedCopy(u) => ("savedCopy", u.to_string()),
                            Resolution::Discarded => ("discarded", String::new()),
                        };
                        p.borrow()
                            .resolved(copy_id, QString::from(what), QString::from(detail.as_str()));
                    }
                    (Err(e), _) => report_error(&me, copy_id, &e),
                    _ => {}
                }
            },
        );
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_role_has_a_value() {
        let d = CopyData::default();
        for r in ROLES {
            assert!(d.value(r).is_valid(), "{r}");
        }
        assert_eq!(d.value("state").to_qstring().to_string(), "watching");
        let dirty = CopyData {
            dirty: true,
            ..CopyData::default()
        };
        assert_eq!(dirty.value("state").to_qstring().to_string(), "changed");
    }
}
