// SPDX-License-Identifier: LGPL-2.1-or-later
//! Go to folder hits. Roles: `uri`, `name`, `source` (`go`, `in`, or a name hit's
//! `HitSource`), `place`, `address` (text that drills into the folder, for `in` rows).

use crate::browse::{cell_data, role_names, Cell};
use crate::runtime::{blocking_then, core};
use lautta_core::app::Core;
use lautta_core::app_directory::looks_like_address;
use lautta_core::Uri;
use qmetaobject::prelude::*;
use qmetaobject::{QByteArray, QPointer};
use std::collections::HashMap;

const COLUMNS: [&str; 5] = ["uri", "name", "source", "place", "address"];

#[derive(QObject, Default)]
pub struct GotoModel {
    base: qt_base_class!(trait QAbstractListModel),
    /// Folder shown: its own folders rank first and `/…` on a server is a path in it.
    folder: qt_property!(QString; NOTIFY folderChanged),
    folderChanged: qt_signal!(),
    count: qt_property!(i32; NOTIFY changed),
    /// Text asked for in the last `query` whose answer has arrived.
    text: qt_property!(QString; NOTIFY changed),
    isPath: qt_property!(bool; NOTIFY changed),
    /// Top row: what Go and Enter open.
    firstUri: qt_property!(QString; NOTIFY changed),
    firstName: qt_property!(QString; NOTIFY changed),
    firstSource: qt_property!(QString; NOTIFY changed),
    changed: qt_signal!(),

    query: qt_method!(fn(&mut self, text: QString)),

    rows: Vec<Vec<Cell>>,
    generation: u64,
}

fn hit_rows(core: &Core, text: &str, base: &Uri) -> (bool, Vec<Vec<Cell>>) {
    if !looks_like_address(text) {
        let rows = core
            .goto_hits(text, base)
            .into_iter()
            .map(|h| {
                row(
                    h.uri.to_string(),
                    h.name,
                    h.source.as_str(),
                    h.place,
                    String::new(),
                )
            })
            .collect();
        return (false, rows);
    }
    let found = core.goto_path(text, base);
    let mut rows = Vec::new();
    if let Some(target) = &found.target {
        let place = target.parent().map(|p| core.place_of(&p)).unwrap_or_default();
        rows.push(row(
            target.to_string(),
            folder_name(core, target),
            "go",
            place,
            String::new(),
        ));
    }
    for u in found.folders {
        let address = format!("{}/", core.edit_address(&u).trim_end_matches('/'));
        rows.push(row(
            u.to_string(),
            folder_name(core, &u),
            "in",
            String::new(),
            address,
        ));
    }
    (true, rows)
}

fn folder_name(core: &Core, uri: &Uri) -> String {
    core.path_steps(uri).pop().map(|s| s.name).unwrap_or_default()
}

fn cell_text(cell: &Cell) -> QString {
    match cell {
        Cell::Str(s) => QString::from(s.as_str()),
        _ => QString::default(),
    }
}

fn row(uri: String, name: String, source: &str, place: String, address: String) -> Vec<Cell> {
    vec![
        Cell::Str(uri),
        Cell::Str(name),
        Cell::s(source),
        Cell::Str(place),
        Cell::Str(address),
    ]
}

impl GotoModel {
    fn query(&mut self, text: QString) {
        let Some(core) = core() else { return };
        self.generation += 1;
        let generation = self.generation;
        let text = text.to_string();
        let Ok(base) = Uri::parse(&self.folder.to_string()) else {
            return;
        };
        let me = QPointer::from(&*self);
        blocking_then(
            move || {
                let (is_path, rows) = hit_rows(&core, &text, &base);
                (text, is_path, rows)
            },
            move |(text, is_path, rows)| {
                let Some(model) = me.as_pinned() else { return };
                let mut model = model.borrow_mut();
                if model.generation != generation {
                    return;
                }
                model.firstUri = rows.first().map(|r| cell_text(&r[0])).unwrap_or_default();
                model.firstName = rows.first().map(|r| cell_text(&r[1])).unwrap_or_default();
                model.firstSource = rows.first().map(|r| cell_text(&r[2])).unwrap_or_default();
                model.begin_reset_model();
                model.rows = rows;
                model.end_reset_model();
                model.count = model.rows.len() as i32;
                model.isPath = is_path;
                model.text = QString::from(text.as_str());
                model.changed();
            },
        );
    }
}

impl QAbstractListModel for GotoModel {
    fn row_count(&self) -> i32 {
        self.rows.len() as i32
    }

    fn data(&self, index: QModelIndex, role: i32) -> QVariant {
        cell_data(&self.rows, index.row(), role)
    }

    fn role_names(&self) -> HashMap<i32, QByteArray> {
        role_names(&COLUMNS)
    }
}
