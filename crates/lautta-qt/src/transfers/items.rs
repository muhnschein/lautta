// SPDX-License-Identifier: LGPL-2.1-or-later
//! `TransferItemsModel { transferId }`: the items of one transfer and its
//! summary for the details page.

use super::events;
use crate::json::to_json;
use crate::runtime::core;
use lautta_core::app_transfers::{direction_of, split_error, state_name, wait_reason_name, ProgressBook};
use lautta_core::ops::OperationKind;
use lautta_core::transfer::model::{kind_name, operation_name};
use lautta_core::transfer::{TransferEvent, TransferItem};
use lautta_core::vpath::display_name;
use qmetaobject::prelude::*;
use qmetaobject::QPointer;
use std::collections::HashMap;

const ROLES: [&str; 11] = [
    "seq",
    "name",
    "src",
    "dst",
    "kind",
    "size",
    "committed",
    "state",
    "errorKind",
    "errorMessage",
    "progress",
];
const FIRST_ROLE: i32 = 257;

#[derive(Debug, Clone, PartialEq, Default)]
struct ItemData {
    seq: i64,
    name: String,
    src: String,
    dst: String,
    kind: &'static str,
    size: i64,
    committed: i64,
    state: &'static str,
    error_kind: String,
    error_message: String,
}

impl ItemData {
    fn of(i: &TransferItem, delete: bool) -> ItemData {
        let (error_kind, error_message) = i.error.as_deref().map(split_error).unwrap_or_default();
        let named = if delete { &i.plan.src } else { &i.plan.dst };
        ItemData {
            seq: i64::from(i.seq),
            name: named.name().map(display_name).unwrap_or_default(),
            src: i.plan.src.to_string(),
            dst: i.plan.dst.to_string(),
            kind: kind_name(i.plan.kind),
            size: i64::try_from(i.size()).unwrap_or(i64::MAX),
            committed: i64::try_from(i.committed).unwrap_or(i64::MAX),
            state: i.state.name(),
            error_kind,
            error_message,
        }
    }

    fn progress(&self) -> f64 {
        match self.state {
            "done" | "skipped" => 1.0,
            _ if self.size > 0 => (self.committed as f64 / self.size as f64).clamp(0.0, 1.0),
            _ => 0.0,
        }
    }

    fn value(&self, role: &str) -> QVariant {
        let s = |v: &str| QVariant::from(QString::from(v));
        match role {
            "seq" => QVariant::from(self.seq),
            "name" => s(&self.name),
            "src" => s(&self.src),
            "dst" => s(&self.dst),
            "kind" => s(self.kind),
            "size" => QVariant::from(self.size),
            "committed" => QVariant::from(self.committed),
            "state" => s(self.state),
            "errorKind" => s(&self.error_kind),
            "errorMessage" => s(&self.error_message),
            "progress" => QVariant::from(self.progress()),
            _ => QVariant::default(),
        }
    }
}

#[derive(QObject, Default)]
pub struct TransferItemsModel {
    base: qt_base_class!(trait QAbstractListModel),
    transferId: qt_property!(i64; NOTIFY transferIdChanged WRITE set_transfer_id),
    transferIdChanged: qt_signal!(),
    count: qt_property!(i32; NOTIFY countChanged),
    countChanged: qt_signal!(),
    /// Summary of the transfer as JSON (see `summary_json`); empty when the
    /// transfer is gone.
    summaryJson: qt_property!(QString; NOTIFY summaryChanged),
    summaryChanged: qt_signal!(),
    refresh: qt_method!(fn(&mut self)),

    rows: Vec<ItemData>,
    book: ProgressBook,
}

impl QAbstractListModel for TransferItemsModel {
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
        ROLES
            .iter()
            .enumerate()
            .map(|(i, n)| (FIRST_ROLE + i as i32, QByteArray::from(*n)))
            .collect()
    }
}

impl TransferItemsModel {
    fn set_transfer_id(&mut self, id: i64) {
        if self.transferId == id {
            return;
        }
        let first = self.transferId == 0;
        self.transferId = id;
        self.transferIdChanged();
        if first {
            events::ensure_forwarder();
            let me = QPointer::from(&*self);
            events::listen(move |ev| match me.as_pinned() {
                Some(p) => {
                    p.borrow_mut().on_event(ev);
                    true
                }
                None => false,
            });
        }
        self.book = ProgressBook::default();
        self.refresh();
    }

    fn on_event(&mut self, ev: &TransferEvent) {
        match ev {
            TransferEvent::Progress {
                id,
                bytes_done,
                rate,
                eta_secs,
            } if *id == self.transferId => {
                self.book.update(*id, *bytes_done, *rate, *eta_secs);
                self.publish_summary();
            }
            TransferEvent::Changed(id) | TransferEvent::Added(id) | TransferEvent::Finished(id)
                if *id == self.transferId || events::is_reload(ev) =>
            {
                self.refresh();
            }
            TransferEvent::NeedsAnswer { id, .. } if *id == self.transferId => self.refresh(),
            _ => {}
        }
    }

    fn refresh(&mut self) {
        let Some(core) = core() else { return };
        let id = self.transferId;
        let delete = core
            .engine
            .get(id)
            .is_some_and(|s| s.kind == OperationKind::Delete);
        let new: Vec<ItemData> = core
            .engine
            .items(id)
            .map(|items| items.iter().map(|i| ItemData::of(i, delete)).collect())
            .unwrap_or_default();
        self.apply(new);
        self.publish_summary();
    }

    fn apply(&mut self, new: Vec<ItemData>) {
        if new.len() == self.rows.len() {
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

    fn publish_summary(&mut self) {
        let text = core()
            .and_then(|c| summary_json(&c, self.transferId, &self.book))
            .unwrap_or_default();
        if self.summaryJson.to_string() != text {
            self.summaryJson = QString::from(text.as_str());
            self.summaryChanged();
        }
    }
}

/// The header of the details page: names, counters, live rate, options.
pub(super) fn summary_json(core: &lautta_core::app::Core, id: i64, book: &ProgressBook) -> Option<String> {
    let s = core.engine.get(id)?;
    let first = core.engine.first_item(id);
    let (done, rate, eta) = book.of(&s);
    let source_dir = first.as_ref().and_then(|i| i.src.parent());
    let direction = first.as_ref().map_or("local", |i| {
        direction_of(&core.locations, s.kind, &i.src, &i.dst).name()
    });
    let loc_name = |u: &lautta_core::Uri| core.location(&u.location).map(|l| l.name).unwrap_or_default();
    Some(to_json(&serde_json::json!({
        "id": s.id,
        "kind": operation_name(s.kind),
        "title": s.title,
        "state": state_name(s.state),
        "waitReason": wait_reason_name(s.state),
        "bytesDone": done,
        "bytesTotal": s.bytes_total,
        "rate": rate,
        "eta": eta.map_or(-1, |e| i64::try_from(e).unwrap_or(i64::MAX)),
        "itemsDone": s.items_done,
        "itemsTotal": s.items_total,
        "itemsFailed": s.items_failed,
        "createdMs": s.created_ms,
        "finishedMs": s.finished_ms,
        "direction": direction,
        "destination": core.locations.display_address(&s.dest),
        "destName": loc_name(&s.dest),
        "sourceAddress": source_dir.as_ref().map(|u| core.locations.display_address(u)),
        "sourceName": source_dir.as_ref().map(loc_name),
        "verifyChecksums": s.options.verify_checksums,
        "preserveMtime": s.options.preserve_mtime,
    })))
}

#[cfg(test)]
mod tests {
    use super::*;
    use lautta_core::entry::Kind;
    use lautta_core::ops::PlanItem;
    use lautta_core::transfer::ItemState;
    use lautta_core::{Uri, VPath};

    fn item(state: ItemState, error: Option<&str>, size: u64, committed: u64) -> TransferItem {
        let uri = |p: &str| Uri::new("a", VPath::parse(p.as_bytes()).unwrap());
        TransferItem {
            seq: 3,
            plan: PlanItem {
                src: uri("x/in.txt"),
                dst: uri("y/out.txt"),
                kind: Kind::File,
                size: Some(size),
                mtime_ms: None,
                mode: None,
                link_target: None,
                conflict: None,
                proposed_name: None,
                resolution: None,
            },
            state,
            committed,
            temp_name: None,
            attempts: 0,
            error: error.map(str::to_owned),
        }
    }

    #[test]
    fn item_rows_carry_names_errors_and_progress() {
        let d = ItemData::of(&item(ItemState::Failed, Some("NoSpace: full"), 100, 25), false);
        assert_eq!(
            (d.seq, d.name.as_str(), d.kind, d.state),
            (3, "out.txt", "file", "failed")
        );
        assert_eq!(
            (d.error_kind.as_str(), d.error_message.as_str()),
            ("NoSpace", "full")
        );
        assert_eq!(d.progress(), 0.25);
        let del = ItemData::of(&item(ItemState::Done, None, 100, 100), true);
        assert_eq!(del.name, "in.txt", "deletes name the source");
        assert_eq!(del.progress(), 1.0);
        assert_eq!(del.error_kind, "");
        assert_eq!(
            ItemData::of(&item(ItemState::Pending, None, 0, 0), false).progress(),
            0.0
        );
    }

    #[test]
    fn every_role_has_a_value() {
        let d = ItemData::default();
        for r in ROLES {
            assert!(d.value(r).is_valid(), "{r}");
        }
        assert_eq!(ROLES.len(), 11);
    }
}
