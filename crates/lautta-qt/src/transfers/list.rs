// SPDX-License-Identifier: LGPL-2.1-or-later
//! `TransfersModel`: the grouped list of the Transfers page (§15.4): active,
//! waiting, paused, edited files and history.

use super::events;
use crate::runtime::{core, spawn_then};
use lautta_core::app_transfers::ProgressBook;
use lautta_core::app_transfers::{state_name, wait_reason_name, EditedFile, TransferGroup, TransferRow};
use lautta_core::transfer::TransferEvent;
use lautta_core::workcopy::Purpose;
use qmetaobject::prelude::*;
use qmetaobject::QPointer;
use std::cell::Cell;
use std::collections::{HashMap, HashSet};

/// Role names in role-id order (`Qt::UserRole + 1 + index`).
const ROLES: [&str; 28] = [
    "transferId",
    "group",
    "kind",
    "title",
    "state",
    "waitReason",
    "bytesDone",
    "bytesTotal",
    "rate",
    "eta",
    "itemsDone",
    "itemsTotal",
    "itemsFailed",
    "direction",
    "destination",
    "destName",
    "createdMs",
    "finishedMs",
    "questions",
    "progress",
    "copyId",
    "remote",
    "pinned",
    "dirty",
    "localSize",
    "lastUploadMs",
    "failed",
    "name",
];
const FIRST_ROLE: i32 = 257;

/// Everything a row shows, flat, so rows can be compared for changes.
#[derive(Debug, Clone, PartialEq, Default)]
pub(super) struct RowData {
    pub is_edit: bool,
    pub id: i64,
    pub group: &'static str,
    pub kind: &'static str,
    pub title: String,
    pub state: String,
    pub wait_reason: &'static str,
    pub bytes_done: i64,
    pub bytes_total: i64,
    pub rate: i64,
    pub eta: i64,
    pub items_done: i64,
    pub items_total: i64,
    pub items_failed: i64,
    pub direction: &'static str,
    pub destination: String,
    pub dest_name: String,
    pub created_ms: i64,
    pub finished_ms: i64,
    pub questions: i64,
    pub copy_id: i64,
    pub remote: String,
    pub pinned: bool,
    pub dirty: bool,
    pub local_size: i64,
    pub last_upload_ms: i64,
}

fn to_i64(v: u64) -> i64 {
    i64::try_from(v).unwrap_or(i64::MAX)
}

impl RowData {
    fn key(&self) -> (bool, i64) {
        (self.is_edit, if self.is_edit { self.copy_id } else { self.id })
    }

    fn progress(&self) -> f64 {
        if self.bytes_total > 0 {
            (self.bytes_done as f64 / self.bytes_total as f64).clamp(0.0, 1.0)
        } else {
            0.0
        }
    }

    fn value(&self, role: &str) -> QVariant {
        let s = |v: &str| QVariant::from(QString::from(v));
        match role {
            "transferId" => QVariant::from(if self.is_edit { -1 } else { self.id }),
            "group" => s(self.group),
            "kind" => s(self.kind),
            "title" | "name" => s(&self.title),
            "state" => s(&self.state),
            "waitReason" => s(self.wait_reason),
            "bytesDone" => QVariant::from(self.bytes_done),
            "bytesTotal" => QVariant::from(self.bytes_total),
            "rate" => QVariant::from(self.rate),
            "eta" => QVariant::from(self.eta),
            "itemsDone" => QVariant::from(self.items_done),
            "itemsTotal" => QVariant::from(self.items_total),
            "itemsFailed" => QVariant::from(self.items_failed),
            "failed" => QVariant::from(self.items_failed > 0),
            "direction" => s(self.direction),
            "destination" => s(&self.destination),
            "destName" => s(&self.dest_name),
            "createdMs" => QVariant::from(self.created_ms),
            "finishedMs" => QVariant::from(self.finished_ms),
            "questions" => QVariant::from(self.questions),
            "progress" => QVariant::from(self.progress()),
            "copyId" => QVariant::from(self.copy_id),
            "remote" => s(&self.remote),
            "pinned" => QVariant::from(self.pinned),
            "dirty" => QVariant::from(self.dirty),
            "localSize" => QVariant::from(self.local_size),
            "lastUploadMs" => QVariant::from(self.last_upload_ms),
            _ => QVariant::default(),
        }
    }
}

pub(super) fn transfer_row(r: &TransferRow) -> RowData {
    let s = &r.summary;
    RowData {
        is_edit: false,
        id: s.id,
        group: r.group.name(),
        kind: r.kind_name(),
        title: s.title.clone(),
        state: state_name(s.state).to_owned(),
        wait_reason: wait_reason_name(s.state),
        bytes_done: to_i64(r.bytes_done),
        bytes_total: to_i64(s.bytes_total),
        rate: to_i64(r.rate),
        eta: r.eta_secs.map_or(-1, to_i64),
        items_done: to_i64(s.items_done),
        items_total: to_i64(s.items_total),
        items_failed: to_i64(s.items_failed),
        direction: r.direction.name(),
        destination: r.dest_address.clone(),
        dest_name: r.dest_name.clone(),
        created_ms: s.created_ms,
        finished_ms: s.finished_ms.unwrap_or(-1),
        questions: r.questions as i64,
        ..RowData::default()
    }
}

/// A working copy as a list row: `state` is `changed` while an upload is
/// pending and `watching` otherwise (EDT-3).
pub(super) fn edited_row(e: &EditedFile) -> RowData {
    let core = core();
    let c = &e.copy;
    RowData {
        is_edit: true,
        group: TransferGroup::Edited.name(),
        kind: if c.purpose == Purpose::Edit {
            "edit"
        } else {
            "open"
        },
        title: c
            .remote
            .name()
            .map(lautta_core::vpath::display_name)
            .unwrap_or_default(),
        state: if e.dirty { "changed" } else { "watching" }.to_owned(),
        direction: "edit",
        destination: core
            .as_ref()
            .map(|k| k.locations.display_address(&c.remote))
            .unwrap_or_default(),
        dest_name: core
            .as_ref()
            .and_then(|k| k.location(&c.remote.location))
            .map(|l| l.name)
            .unwrap_or_default(),
        eta: -1,
        finished_ms: -1,
        created_ms: -1,
        copy_id: c.id,
        remote: c.remote.to_string(),
        pinned: c.pinned,
        dirty: e.dirty,
        local_size: to_i64(e.local_size),
        last_upload_ms: c.last_upload_ms.unwrap_or(-1),
        ..RowData::default()
    }
}

/// Puts the edited files between paused and history.
pub(super) fn assemble(mut transfers: Vec<RowData>, edited: &[RowData]) -> Vec<RowData> {
    let at = transfers.partition_point(|r| r.group != TransferGroup::History.name());
    let tail = transfers.split_off(at);
    transfers.extend(edited.iter().cloned());
    transfers.extend(tail);
    transfers
}

#[derive(QObject, Default)]
pub struct TransfersModel {
    base: qt_base_class!(trait QAbstractListModel),
    count: qt_property!(i32; NOTIFY countChanged),
    countChanged: qt_signal!(),
    refresh: qt_method!(fn(&mut self)),

    rows: Vec<RowData>,
    edited: Vec<RowData>,
    book: ProgressBook,
    registered: Cell<bool>,
}

impl QAbstractListModel for TransfersModel {
    fn row_count(&self) -> i32 {
        // Views ask for the row count first, before any role names.
        self.ensure_registered();
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

impl TransfersModel {
    /// Connects to the engine's events the first time a view looks at the
    /// model (QML gives list models no start-up hook).
    fn ensure_registered(&self) {
        if self.registered.replace(true) {
            return;
        }
        events::ensure_forwarder();
        let me = QPointer::from(self);
        events::listen(move |ev| match me.as_pinned() {
            Some(p) => {
                p.borrow_mut().on_event(ev);
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

    fn on_event(&mut self, ev: &TransferEvent) {
        if let TransferEvent::Progress {
            id,
            bytes_done,
            rate,
            eta_secs,
        } = ev
        {
            self.book.update(*id, *bytes_done, *rate, *eta_secs);
        }
        if events::is_reload(ev) {
            self.reload_edited();
        }
        self.rebuild();
    }

    fn refresh(&mut self) {
        self.reload_edited();
        self.rebuild();
    }

    fn reload_edited(&self) {
        let Some(core) = core() else { return };
        let me = QPointer::from(self);
        spawn_then(async move { core.edited_files().await }, move |res| {
            let (Some(p), Ok(files)) = (me.as_pinned(), res) else {
                return;
            };
            let mut m = p.borrow_mut();
            m.edited = files.iter().map(edited_row).collect();
            m.rebuild();
        });
    }

    fn rebuild(&mut self) {
        let Some(core) = core() else { return };
        let rows = core.transfer_rows(&self.book);
        let ids: HashSet<i64> = rows.iter().map(|r| r.summary.id).collect();
        self.book.retain(&ids);
        let data = rows.iter().map(transfer_row).collect();
        let new = assemble(data, &self.edited);
        self.apply(new);
    }

    /// Updates changed rows in place when the row order is the same (keeps
    /// the scroll position and open context menus), resets otherwise.
    fn apply(&mut self, new: Vec<RowData>) {
        let same =
            new.len() == self.rows.len() && new.iter().zip(&self.rows).all(|(a, b)| a.key() == b.key());
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
}

#[cfg(test)]
mod tests {
    use super::*;

    fn row(group: &'static str, id: i64) -> RowData {
        RowData {
            group,
            id,
            ..RowData::default()
        }
    }

    #[test]
    fn edited_files_sit_between_paused_and_history() {
        let t = vec![
            row("active", 1),
            row("paused", 2),
            row("history", 3),
            row("history", 4),
        ];
        let e = vec![RowData {
            is_edit: true,
            group: "edited",
            copy_id: 9,
            ..RowData::default()
        }];
        let ids: Vec<(bool, i64)> = assemble(t, &e).iter().map(RowData::key).collect();
        assert_eq!(ids, [(false, 1), (false, 2), (true, 9), (false, 3), (false, 4)]);
        assert_eq!(assemble(vec![], &e).len(), 1);
        assert_eq!(assemble(vec![row("history", 1)], &[]).len(), 1);
    }

    #[test]
    fn progress_is_a_clamped_fraction() {
        let mut r = RowData::default();
        assert_eq!(r.progress(), 0.0);
        r.bytes_total = 200;
        r.bytes_done = 50;
        assert_eq!(r.progress(), 0.25);
        r.bytes_done = 500;
        assert_eq!(r.progress(), 1.0);
    }

    #[test]
    fn roles_cover_every_field_name() {
        let r = RowData::default();
        for name in ROLES {
            assert!(r.value(name).is_valid(), "role {name} has no value");
        }
        assert!(!r.value("nonsense").is_valid());
        assert_eq!(r.value("transferId").to_int(), 0);
        let e = RowData {
            is_edit: true,
            ..RowData::default()
        };
        assert_eq!(e.value("transferId").to_int() as i32, -1);
    }
}
