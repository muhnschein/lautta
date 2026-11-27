// SPDX-License-Identifier: LGPL-2.1-or-later

use crate::json::to_json;
use crate::operations::{error_parts, parse_uri};
use crate::runtime::{core, handle};
use lautta_core::app_checksum::{verdict, ItemState, Verdict, VerifyEvent};
use lautta_core::checksum::{list, to_hex, Algorithm, Cancel};
use lautta_core::provider::ProgressSink;
use lautta_core::{Error, Result};
use qmetaobject::prelude::*;
use qmetaobject::{qml_register_type, queued_callback, QPointer, USER_ROLE};
use std::collections::HashMap;
use std::sync::Arc;

pub fn register() {
    qml_register_type::<ChecksumModel>(&crate::qml_uri(), 1, 0, &crate::cstr("ChecksumModel"));
    qml_register_type::<VerifyModel>(&crate::qml_uri(), 1, 0, &crate::cstr("VerifyModel"));
}

pub fn algorithms_json(group: &str) -> String {
    let list: &[Algorithm] = if group == "more" {
        &Algorithm::MORE
    } else {
        &Algorithm::COMMON
    };
    let rows: Vec<_> = list
        .iter()
        .map(|a| serde_json::json!({ "id": a.id(), "label": a.label() }))
        .collect();
    to_json(&rows)
}

pub fn is_checksum_list(name_or_uri: &str) -> bool {
    let name = match lautta_core::Uri::parse(name_or_uri) {
        Ok(uri) => uri.name().map(<[u8]>::to_vec).unwrap_or_default(),
        Err(_) => name_or_uri.as_bytes().to_vec(),
    };
    list::list_kind(&name).is_some()
}

fn fraction(done: u64, total: Option<u64>) -> f64 {
    match total {
        Some(0) => 1.0,
        Some(t) => (done as f64 / t as f64).min(1.0),
        None => -1.0,
    }
}

fn lossy(name: &[u8]) -> QString {
    QString::from(String::from_utf8_lossy(name).as_ref())
}

struct Running(Cancel);

impl Drop for Running {
    fn drop(&mut self) {
        self.0.cancel();
    }
}

enum Step {
    Progress(u64, Option<u64>),
    Done(Result<(Vec<u8>, Option<Verdict>)>),
}

#[derive(QObject, Default)]
pub struct ChecksumModel {
    base: qt_base_class!(trait QObject),
    uri: qt_property!(QString; NOTIFY changed),
    algorithm: qt_property!(QString; NOTIFY changed),
    label: qt_property!(QString; NOTIFY changed),
    running: qt_property!(bool; NOTIFY changed),
    progress: qt_property!(f64; NOTIFY progressChanged),
    hex: qt_property!(QString; NOTIFY changed),
    verdict: qt_property!(QString; NOTIFY changed),
    verdictList: qt_property!(QString; NOTIFY changed),
    errorKind: qt_property!(QString; NOTIFY changed),
    errorMessage: qt_property!(QString; NOTIFY changed),
    changed: qt_signal!(),
    progressChanged: qt_signal!(),

    start: qt_method!(fn(&mut self) -> bool),

    run: Option<Running>,
    generation: u64,
}

impl ChecksumModel {
    fn start(&mut self) -> bool {
        let (Some(core), Some(uri), Some(algorithm)) = (
            core(),
            parse_uri(&self.uri),
            Algorithm::parse(&self.algorithm.to_string()),
        ) else {
            return false;
        };
        let cancel = Cancel::default();
        self.run = Some(Running(cancel.clone()));
        self.generation += 1;
        let generation = self.generation;
        self.reset(algorithm);
        let me = QPointer::from(&*self);
        let post = queued_callback(move |step: Step| {
            if let Some(model) = me.as_pinned() {
                model.borrow_mut().apply(generation, step);
            }
        });
        let tick = post.clone();
        let progress: ProgressSink = Arc::new(move |done, total| tick(Step::Progress(done, total)));
        handle().spawn(async move {
            let res = async {
                let digest = core.checksum(&uri, algorithm, progress, &cancel).await?;
                let listed = core.lists_naming(&uri, &cancel).await.unwrap_or_default();
                cancel.check()?;
                let v = verdict(&listed, algorithm, &digest);
                Ok::<_, Error>((digest, v))
            }
            .await;
            post(Step::Done(res));
        });
        true
    }

    fn reset(&mut self, algorithm: Algorithm) {
        self.label = QString::from(algorithm.label());
        self.running = true;
        self.progress = 0.0;
        self.hex = QString::default();
        self.verdict = QString::default();
        self.verdictList = QString::default();
        self.errorKind = QString::default();
        self.errorMessage = QString::default();
        self.changed();
        self.progressChanged();
    }

    fn apply(&mut self, generation: u64, step: Step) {
        if generation != self.generation || !self.running {
            return;
        }
        match step {
            Step::Progress(done, total) => {
                self.progress = fraction(done, total);
                self.progressChanged();
            }
            Step::Done(res) => {
                self.running = false;
                self.run = None;
                match res {
                    Ok((digest, v)) => {
                        self.hex = QString::from(to_hex(&digest).as_str());
                        self.progress = 1.0;
                        let (word, list) = match &v {
                            Some(Verdict::Matches(l)) => ("matches", lossy(l)),
                            Some(Verdict::Differs(l)) => ("differs", lossy(l)),
                            None => ("", QString::default()),
                        };
                        self.verdict = QString::from(word);
                        self.verdictList = list;
                    }
                    Err(e) => (self.errorKind, self.errorMessage) = error_parts(&e),
                }
                self.changed();
                self.progressChanged();
            }
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RowState {
    Waiting,
    Checking,
    Done(ItemState),
}

impl RowState {
    fn name(self) -> &'static str {
        match self {
            RowState::Waiting => "waiting",
            RowState::Checking => "checking",
            RowState::Done(ItemState::Matches) => "matches",
            RowState::Done(ItemState::Differs) => "differs",
            RowState::Done(ItemState::Missing) => "missing",
            RowState::Done(ItemState::NotChecked) => "notChecked",
            RowState::Done(ItemState::Failed(_)) => "failed",
        }
    }

    fn is_problem(self) -> bool {
        matches!(self, RowState::Done(s) if s != ItemState::Matches)
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct Row {
    pub index: usize,
    pub name: String,
    pub state: RowState,
    pub progress: f64,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Change {
    Reset,
    Updated(usize),
    Moved(usize, usize),
    None,
}

#[derive(Default)]
pub struct VerifyRows {
    pub rows: Vec<Row>,
    problems: usize,
}

impl VerifyRows {
    fn position(&self, index: usize) -> Option<usize> {
        self.rows.iter().position(|r| r.index == index)
    }

    pub fn apply(&mut self, event: VerifyEvent) -> Change {
        match event {
            VerifyEvent::Listed(names) => {
                self.rows = names
                    .iter()
                    .enumerate()
                    .map(|(index, n)| Row {
                        index,
                        name: String::from_utf8_lossy(n).into_owned(),
                        state: RowState::Waiting,
                        progress: 0.0,
                    })
                    .collect();
                self.problems = 0;
                Change::Reset
            }
            VerifyEvent::Progress { index, done, total } => {
                let Some(at) = self.position(index) else {
                    return Change::None;
                };
                let row = &mut self.rows[at];
                row.state = RowState::Checking;
                row.progress = fraction(done, total);
                Change::Updated(at)
            }
            VerifyEvent::Checked { index, state } => {
                let Some(at) = self.position(index) else {
                    return Change::None;
                };
                self.rows[at].state = RowState::Done(state);
                if !self.rows[at].state.is_problem() {
                    return Change::Updated(at);
                }
                let to = self.problems;
                self.problems += 1;
                if at == to {
                    return Change::Updated(at);
                }
                Change::Moved(at, to)
            }
        }
    }

    pub fn move_row(&mut self, from: usize, to: usize) {
        let row = self.rows.remove(from);
        self.rows.insert(to, row);
    }
}

const VERIFY_ROLES: [&str; 4] = ["name", "state", "progress", "errorKind"];

#[derive(QObject, Default)]
pub struct VerifyModel {
    base: qt_base_class!(trait QAbstractListModel),
    uri: qt_property!(QString; NOTIFY changed),
    running: qt_property!(bool; NOTIFY changed),
    checked: qt_property!(i32; NOTIFY changed),
    count: qt_property!(i32; NOTIFY changed),
    errorKind: qt_property!(QString; NOTIFY changed),
    errorMessage: qt_property!(QString; NOTIFY changed),
    changed: qt_signal!(),

    start: qt_method!(fn(&mut self) -> bool),

    rows: VerifyRows,
    run: Option<Running>,
    generation: u64,
}

enum VerifyStep {
    Event(VerifyEvent),
    Done(Result<()>),
}

impl QAbstractListModel for VerifyModel {
    fn row_count(&self) -> i32 {
        self.rows.rows.len() as i32
    }

    fn data(&self, index: QModelIndex, role: i32) -> QVariant {
        let Some(r) = usize::try_from(index.row())
            .ok()
            .and_then(|i| self.rows.rows.get(i))
        else {
            return QVariant::default();
        };
        match role - USER_ROLE {
            0 => QVariant::from(QString::from(r.name.as_str())),
            1 => QVariant::from(QString::from(r.state.name())),
            2 => QVariant::from(r.progress),
            3 => QVariant::from(QString::from(match r.state {
                RowState::Done(ItemState::Failed(k)) => k.name(),
                _ => "",
            })),
            _ => QVariant::default(),
        }
    }

    fn role_names(&self) -> HashMap<i32, QByteArray> {
        VERIFY_ROLES
            .iter()
            .enumerate()
            .map(|(i, n)| (USER_ROLE + i as i32, QByteArray::from(*n)))
            .collect()
    }
}

impl VerifyModel {
    fn start(&mut self) -> bool {
        let (Some(core), Some(uri)) = (core(), parse_uri(&self.uri)) else {
            return false;
        };
        let cancel = Cancel::default();
        self.run = Some(Running(cancel.clone()));
        self.generation += 1;
        let generation = self.generation;
        self.running = true;
        self.begin_reset_model();
        self.rows = VerifyRows::default();
        self.end_reset_model();
        self.count = 0;
        self.checked = 0;
        self.errorKind = QString::default();
        self.errorMessage = QString::default();
        self.changed();
        let me = QPointer::from(&*self);
        let post = queued_callback(move |step: VerifyStep| {
            if let Some(model) = me.as_pinned() {
                model.borrow_mut().apply(generation, step);
            }
        });
        let events = post.clone();
        let sink: Arc<dyn Fn(VerifyEvent) + Send + Sync> = Arc::new(move |e| events(VerifyStep::Event(e)));
        handle().spawn(async move {
            let res = core.verify_list(&uri, sink, &cancel).await;
            post(VerifyStep::Done(res));
        });
        true
    }

    fn apply(&mut self, generation: u64, step: VerifyStep) {
        if generation != self.generation || !self.running {
            return;
        }
        match step {
            VerifyStep::Event(e) => {
                let checked = matches!(e, VerifyEvent::Checked { .. });
                match self.rows.apply(e) {
                    Change::Reset => {
                        self.begin_reset_model();
                        self.end_reset_model();
                    }
                    Change::Updated(at) => {
                        let i = self.row_index(at as i32);
                        self.data_changed(i, i);
                    }
                    Change::Moved(from, to) => {
                        let (f, t) = (from as i32, to as i32);
                        self.begin_move_rows(QModelIndex::default(), f, f, QModelIndex::default(), t);
                        self.rows.move_row(from, to);
                        self.end_move_rows();
                        let i = self.row_index(t);
                        self.data_changed(i, i);
                    }
                    Change::None => {}
                }
                self.count = self.rows.rows.len() as i32;
                if checked {
                    self.checked += 1;
                }
                self.changed();
            }
            VerifyStep::Done(res) => {
                self.running = false;
                self.run = None;
                if let Err(e) = res {
                    (self.errorKind, self.errorMessage) = error_parts(&e);
                }
                self.changed();
            }
        }
    }
}

#[cfg(test)]
mod tests;
