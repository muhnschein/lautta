// SPDX-License-Identifier: LGPL-2.1-or-later
//! `BulkRenameModel { urisJson }`: rules as JSON and a live preview (OPS-11).

use super::{error_parts, parse_uris};
use crate::runtime::{core, spawn_then};
use lautta_core::app_operations::{parse_rules, rename_preview, status_name, BulkOutcome, RenameContext};
use lautta_core::entry::Kind;
use lautta_core::ops::bulkrename::{RenameStatus, RuleSet};
use lautta_core::ops::names::NameRules;
use lautta_core::{Error, Result, Uri};
use qmetaobject::prelude::*;
use qmetaobject::QPointer;
use std::collections::HashMap;

const ROLE_OLD: i32 = 0x100;
const ROLE_NEW: i32 = 0x101;
const ROLE_STATUS: i32 = 0x102;
const ROLE_CHANGED: i32 = 0x103;

#[derive(Clone, Debug, PartialEq, Eq)]
struct Row {
    old: String,
    new: String,
    status: &'static str,
    changed: bool,
}

/// The preview before the folder was read: only the selected names are
/// known, so collisions with other files in the folder show up later.
fn provisional_context(uris: &[Uri]) -> Option<RenameContext> {
    let parent = uris.first()?.parent()?;
    let entries: Vec<_> = uris
        .iter()
        .map(|u| (u.name().unwrap_or_default().to_vec(), None))
        .collect();
    Some(RenameContext {
        parent,
        uris: uris.to_vec(),
        existing: entries.iter().map(|(n, _)| n.clone()).collect(),
        kinds: vec![Kind::File; uris.len()],
        entries,
        name_rules: NameRules::permissive(),
    })
}

fn rows_for(ctx: &RenameContext, rules: &RuleSet) -> Result<Vec<Row>> {
    Ok(rename_preview(ctx, rules)?
        .into_iter()
        .map(|p| Row {
            old: String::from_utf8_lossy(&p.old).into_owned(),
            new: String::from_utf8_lossy(&p.new).into_owned(),
            status: status_name(p.status),
            changed: p.status != RenameStatus::Unchanged,
        })
        .collect())
}

#[derive(QObject, Default)]
pub struct BulkRenameModel {
    base: qt_base_class!(trait QAbstractListModel),
    urisJson: qt_property!(QString; WRITE set_uris NOTIFY uris_changed),
    uris_changed: qt_signal!(),
    rulesJson: qt_property!(QString; WRITE set_rules NOTIFY rules_changed),
    rules_changed: qt_signal!(),
    count: qt_property!(i32; NOTIFY preview_changed),
    changedCount: qt_property!(i32; NOTIFY preview_changed),
    problemCount: qt_property!(i32; NOTIFY preview_changed),
    errorKind: qt_property!(QString; NOTIFY preview_changed),
    errorMessage: qt_property!(QString; NOTIFY preview_changed),
    preview_changed: qt_signal!(),
    busy: qt_property!(bool; NOTIFY busy_changed),
    busy_changed: qt_signal!(),

    apply: qt_method!(fn(&mut self)),
    applied: qt_signal!(renamed: i32, skipped: i32, failedCount: i32, firstError: QString),
    failed: qt_signal!(kind: QString, message: QString),

    uris: Vec<Uri>,
    ctx: Option<RenameContext>,
    rules: Option<RuleSet>,
    rows: Vec<Row>,
    generation: u64,
}

impl QAbstractListModel for BulkRenameModel {
    fn row_count(&self) -> i32 {
        i32::try_from(self.rows.len()).unwrap_or(i32::MAX)
    }

    fn data(&self, index: QModelIndex, role: i32) -> QVariant {
        let Some(row) = usize::try_from(index.row()).ok().and_then(|i| self.rows.get(i)) else {
            return QVariant::default();
        };
        match role {
            ROLE_OLD => QString::from(row.old.as_str()).into(),
            ROLE_NEW => QString::from(row.new.as_str()).into(),
            ROLE_STATUS => QString::from(row.status).into(),
            ROLE_CHANGED => row.changed.into(),
            _ => QVariant::default(),
        }
    }

    fn role_names(&self) -> HashMap<i32, QByteArray> {
        let mut names = HashMap::new();
        names.insert(ROLE_OLD, "old".into());
        names.insert(ROLE_NEW, "new".into());
        names.insert(ROLE_STATUS, "status".into());
        names.insert(ROLE_CHANGED, "changed".into());
        names
    }
}

impl BulkRenameModel {
    fn set_uris(&mut self, value: QString) {
        self.urisJson = value.clone();
        self.uris = parse_uris(&value).unwrap_or_default();
        self.ctx = provisional_context(&self.uris);
        self.generation += 1;
        self.uris_changed();
        self.refresh();
        self.load_folder();
    }

    fn set_rules(&mut self, value: QString) {
        self.rulesJson = value.clone();
        match parse_rules(&value.to_string()) {
            Ok(rules) => self.rules = Some(rules),
            Err(e) => {
                self.set_error(Some(&e));
                self.rules_changed();
                return;
            }
        }
        self.rules_changed();
        self.refresh();
    }

    /// Reads the folder to find collisions with files that are not selected.
    fn load_folder(&mut self) {
        let Some(core) = core() else { return };
        if self.uris.is_empty() {
            return;
        }
        let uris = self.uris.clone();
        let generation = self.generation;
        let me = QPointer::from(&*self);
        spawn_then(async move { core.rename_context(&uris).await }, move |res| {
            let Some(p) = me.as_pinned() else { return };
            let mut this = p.borrow_mut();
            if this.generation != generation {
                return;
            }
            match res {
                Ok(ctx) => {
                    this.ctx = Some(ctx);
                    this.refresh();
                }
                Err(e) => this.set_error(Some(&e)),
            }
            this.preview_changed();
        });
    }

    fn set_error(&mut self, e: Option<&Error>) {
        let (kind, message) = e.map(error_parts).unwrap_or_default();
        self.errorKind = kind;
        self.errorMessage = message;
        self.preview_changed();
    }

    fn refresh(&mut self) {
        let (Some(ctx), Some(rules)) = (&self.ctx, &self.rules) else {
            return;
        };
        match rows_for(ctx, rules) {
            Ok(rows) => {
                self.set_error(None);
                self.replace_rows(rows);
            }
            Err(e) => self.set_error(Some(&e)),
        }
    }

    fn replace_rows(&mut self, rows: Vec<Row>) {
        self.begin_reset_model();
        self.rows = rows;
        self.end_reset_model();
        self.count = i32::try_from(self.rows.len()).unwrap_or(i32::MAX);
        let changed = self.rows.iter().filter(|r| r.changed).count();
        let problems = self
            .rows
            .iter()
            .filter(|r| matches!(r.status, "collision" | "invalid"))
            .count();
        self.changedCount = i32::try_from(changed).unwrap_or(i32::MAX);
        self.problemCount = i32::try_from(problems).unwrap_or(i32::MAX);
        self.preview_changed();
    }

    fn apply(&mut self) {
        let (Some(core), Some(rules)) = (core(), self.rules.clone()) else {
            return;
        };
        if self.busy || self.uris.is_empty() {
            return;
        }
        self.busy = true;
        self.busy_changed();
        let uris = self.uris.clone();
        let me = QPointer::from(&*self);
        spawn_then(async move { core.bulk_rename(&uris, &rules).await }, move |res| {
            let Some(p) = me.as_pinned() else { return };
            p.borrow_mut().busy = false;
            let this = p.borrow();
            this.busy_changed();
            this.finished(res);
        });
    }

    fn finished(&self, res: Result<BulkOutcome>) {
        match res {
            Ok(o) => self.applied(
                i32::try_from(o.renamed).unwrap_or(i32::MAX),
                i32::try_from(o.skipped).unwrap_or(i32::MAX),
                i32::try_from(o.failed).unwrap_or(i32::MAX),
                QString::from(o.first_error.unwrap_or_default().as_str()),
            ),
            Err(e) => {
                let (kind, message) = error_parts(&e);
                self.failed(kind, message);
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use lautta_core::ops::bulkrename::Rule;

    fn uris(names: &[&str]) -> Vec<Uri> {
        names
            .iter()
            .map(|n| Uri::parse(&format!("lautta://user-documents/{n}")).unwrap())
            .collect()
    }

    #[test]
    fn provisional_context_knows_only_the_selection() {
        let ctx = provisional_context(&uris(&["a.txt", "b.txt"])).unwrap();
        assert_eq!(ctx.existing, vec![b"a.txt".to_vec(), b"b.txt".to_vec()]);
        assert_eq!(ctx.parent.to_string(), "lautta://user-documents/");
        assert!(provisional_context(&[]).is_none());
    }

    #[test]
    fn rows_follow_the_rules() {
        let ctx = provisional_context(&uris(&["IMG_1.jpg", "keep.jpg"])).unwrap();
        let rules = RuleSet {
            rules: vec![Rule::FindReplace {
                find: "IMG_".into(),
                replace: "Trip-".into(),
                regex: false,
                case_sensitive: true,
            }],
            include_extension: false,
        };
        let rows = rows_for(&ctx, &rules).unwrap();
        assert_eq!(rows[0].new, "Trip-1.jpg");
        assert_eq!((rows[0].status, rows[0].changed), ("ok", true));
        assert_eq!((rows[1].status, rows[1].changed), ("unchanged", false));
    }

    #[test]
    fn colliding_names_are_flagged() {
        let ctx = provisional_context(&uris(&["a1.txt", "a2.txt"])).unwrap();
        let rules = RuleSet {
            rules: vec![Rule::FindReplace {
                find: "[12]".into(),
                replace: "".into(),
                regex: true,
                case_sensitive: true,
            }],
            include_extension: false,
        };
        let rows = rows_for(&ctx, &rules).unwrap();
        assert!(rows.iter().all(|r| r.status == "collision"));
    }
}
