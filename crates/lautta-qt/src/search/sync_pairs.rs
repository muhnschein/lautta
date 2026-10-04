// SPDX-License-Identifier: LGPL-2.1-or-later
//! `SyncPairsModel`: the saved sync pairs (SYN-3), reorderable and removable.
//! Running a pair is the page's job: it opens CompareResults with the
//! pair's id and `CompareModel.loadPair`.

use crate::runtime::{blocking_then, core};
use lautta_core::org::syncpairs::SyncPair;
use qmetaobject::prelude::*;
use qmetaobject::{QPointer, USER_ROLE};
use std::collections::HashMap;

const ROLES: [&str; 8] = [
    "pairId",
    "label",
    "leftUri",
    "rightUri",
    "mode",
    "excludes",
    "checksums",
    "dstTolerance",
];

#[derive(QObject, Default)]
pub struct SyncPairsModel {
    base: qt_base_class!(trait QAbstractListModel),
    count: qt_property!(i32; NOTIFY countChanged),
    countChanged: qt_signal!(),
    reload: qt_method!(fn(&mut self)),
    remove: qt_method!(fn(&mut self, id: i64)),
    /// Moves the pair at row `from` to row `to`.
    moveTo: qt_method!(fn(&mut self, from: i32, to: i32) -> bool),
    rename: qt_method!(fn(&mut self, id: i64, label: QString)),
    pairs: Vec<SyncPair>,
}

impl QAbstractListModel for SyncPairsModel {
    fn row_count(&self) -> i32 {
        self.pairs.len() as i32
    }

    fn data(&self, index: QModelIndex, role: i32) -> QVariant {
        let Some(p) = usize::try_from(index.row()).ok().and_then(|i| self.pairs.get(i)) else {
            return QVariant::default();
        };
        let text = |s: &str| QVariant::from(QString::from(s));
        match role - USER_ROLE {
            0 => QVariant::from(p.id),
            1 => text(&p.spec.label),
            2 => text(&p.spec.left.to_string()),
            3 => text(&p.spec.right.to_string()),
            4 => text(p.spec.mode.as_str()),
            5 => text(&lautta_core::app_search::excludes_text(&p.spec.excludes)),
            6 => QVariant::from(p.spec.checksums),
            7 => QVariant::from(p.spec.dst_tolerance),
            _ => QVariant::default(),
        }
    }

    fn role_names(&self) -> HashMap<i32, QByteArray> {
        ROLES
            .iter()
            .enumerate()
            .map(|(i, n)| (USER_ROLE + i as i32, QByteArray::from(*n)))
            .collect()
    }
}

impl SyncPairsModel {
    /// Reads the pairs from the database; the list replaces the current one.
    fn reload(&mut self) {
        self.change(|core| core.sync_pairs.list().ok());
    }

    fn remove(&mut self, id: i64) {
        self.change(move |core| {
            let _ = core.sync_pairs.remove(id);
            core.sync_pairs.list().ok()
        });
    }

    fn moveTo(&mut self, from: i32, to: i32) -> bool {
        let Some(id) = usize::try_from(from)
            .ok()
            .and_then(|i| self.pairs.get(i))
            .map(|p| p.id)
        else {
            return false;
        };
        let Ok(to) = usize::try_from(to) else {
            return false;
        };
        self.change(move |core| {
            let _ = core.sync_pairs.move_to(id, to);
            core.sync_pairs.list().ok()
        });
        true
    }

    fn rename(&mut self, id: i64, label: QString) {
        let label = label.to_string();
        self.change(move |core| {
            if let Ok(Some(mut pair)) = core.sync_pairs.get(id) {
                pair.spec.label = label;
                let _ = core.sync_pairs.update(id, &pair.spec);
            }
            core.sync_pairs.list().ok()
        });
    }

    /// Runs `work` on a blocking thread and shows the list it returns.
    fn change(
        &mut self,
        work: impl FnOnce(&lautta_core::app::Core) -> Option<Vec<SyncPair>> + Send + 'static,
    ) {
        let Some(core) = core() else { return };
        let me = QPointer::from(&*self);
        blocking_then(
            move || work(&core),
            move |list| {
                if let (Some(model), Some(list)) = (me.as_pinned(), list) {
                    model.borrow_mut().show(list);
                }
            },
        );
    }

    fn show(&mut self, list: Vec<SyncPair>) {
        self.begin_reset_model();
        self.pairs = list;
        self.end_reset_model();
        self.count = self.pairs.len() as i32;
        self.countChanged();
    }
}
