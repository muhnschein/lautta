// SPDX-License-Identifier: LGPL-2.1-or-later
//! `FavouritesModel` (ORG-1): favourites in their order, with label and
//! colour. Roles: `itemId`, `uri`, `label`, `colour`, `place`.

use super::{cell_data, error_parts, role_names, store_changed, Cell};
use crate::json::to_json;
use crate::runtime::{blocking_then, core};
use lautta_core::app::Core;
use lautta_core::Uri;
use qmetaobject::prelude::*;
use qmetaobject::QPointer;
use std::collections::HashMap;
use std::sync::Arc;

const COLUMNS: [&str; 5] = ["itemId", "uri", "label", "colour", "place"];

fn load(core: &Core) -> lautta_core::Result<Vec<Vec<Cell>>> {
    Ok(core
        .favourites
        .list()?
        .into_iter()
        .map(|f| {
            vec![
                Cell::Int(f.id),
                Cell::Str(f.uri.to_string()),
                Cell::Str(f.label),
                Cell::Str(f.colour.unwrap_or_default()),
                Cell::Str(core.place_of(&f.uri)),
            ]
        })
        .collect())
}

fn int_at(row: &[Cell], col: usize) -> i64 {
    match row.get(col) {
        Some(Cell::Int(i)) => *i,
        _ => -1,
    }
}

fn str_at(row: &[Cell], col: usize) -> &str {
    match row.get(col) {
        Some(Cell::Str(s)) => s,
        _ => "",
    }
}

/// The index to give a favourite moved by `delta` places; `None` when it
/// would not move.
fn target_index(rows: &[Vec<Cell>], id: i64, delta: i32) -> Option<usize> {
    let at = rows.iter().position(|r| int_at(r, 0) == id)?;
    let to = (at as i64 + i64::from(delta)).clamp(0, rows.len() as i64 - 1) as usize;
    (to != at).then_some(to)
}

#[derive(QObject, Default)]
pub struct FavouritesModel {
    base: qt_base_class!(trait QAbstractListModel),
    count: qt_property!(i32; NOTIFY countChanged),
    countChanged: qt_signal!(),
    /// True once the first load finished.
    loaded: qt_property!(bool; NOTIFY loadedChanged),
    loadedChanged: qt_signal!(),

    reload: qt_method!(fn(&mut self)),
    add: qt_method!(fn(&mut self, uri: QString, label: QString, colour: QString)),
    edit: qt_method!(fn(&mut self, id: i32, label: QString, colour: QString)),
    remove: qt_method!(fn(&mut self, id: i32)),
    moveTo: qt_method!(fn(&mut self, id: i32, index: i32)),
    moveBy: qt_method!(fn(&mut self, id: i32, delta: i32)),
    /// `{id, uri, label, colour, place}` of a loaded favourite, or `{}`.
    get: qt_method!(fn(&self, id: i32) -> QString),
    /// The id of the favourite for a folder, `-1` when it is none.
    idOf: qt_method!(fn(&self, uri: QString) -> i32),
    /// `Location › folder` for a folder URI (no I/O).
    placeOf: qt_method!(fn(&self, uri: QString) -> QString),
    failed: qt_signal!(kind: QString, message: QString),

    rows: Vec<Vec<Cell>>,
}

impl FavouritesModel {
    fn reload(&mut self) {
        let Some(core) = core() else { return };
        let me = QPointer::from(&*self);
        blocking_then(
            move || load(&core),
            move |res| {
                let Some(model) = me.as_pinned() else { return };
                let mut model = model.borrow_mut();
                match res {
                    Ok(rows) => model.show(rows),
                    Err(e) => model.report(&e),
                }
            },
        );
    }

    fn show(&mut self, rows: Vec<Vec<Cell>>) {
        self.begin_reset_model();
        self.rows = rows;
        self.end_reset_model();
        self.count = self.rows.len() as i32;
        self.countChanged();
        if !self.loaded {
            self.loaded = true;
            self.loadedChanged();
        }
    }

    fn report(&self, e: &lautta_core::Error) {
        let (kind, message) = error_parts(e);
        self.failed(kind, message);
    }

    /// Runs a store change off the GUI thread, then tells the other models
    /// and shows the new list.
    fn change(&mut self, work: impl FnOnce(Arc<Core>) -> lautta_core::Result<()> + Send + 'static) {
        let Some(core) = core() else { return };
        let me = QPointer::from(&*self);
        blocking_then(
            move || work(core),
            move |res| {
                let Some(model) = me.as_pinned() else { return };
                let mut model = model.borrow_mut();
                if let Err(e) = res {
                    model.report(&e);
                }
                store_changed();
                model.reload();
            },
        );
    }

    fn add(&mut self, uri: QString, label: QString, colour: QString) {
        let (uri, label, colour) = (uri.to_string(), label.to_string(), colour.to_string());
        self.change(move |core| {
            let uri = Uri::parse(&uri)?;
            core.favourites
                .add(&uri, &label, Some(colour.as_str()).filter(|c| !c.is_empty()))
                .map(|_| ())
        });
    }

    fn edit(&mut self, id: i32, label: QString, colour: QString) {
        let (label, colour) = (label.to_string(), colour.to_string());
        self.change(move |core| {
            core.favourites.update(
                i64::from(id),
                &label,
                Some(colour.as_str()).filter(|c| !c.is_empty()),
            )
        });
    }

    fn remove(&mut self, id: i32) {
        self.change(move |core| core.favourites.remove(i64::from(id)).map(|_| ()));
    }

    fn moveTo(&mut self, id: i32, index: i32) {
        let index = usize::try_from(index).unwrap_or(0);
        self.change(move |core| core.favourites.move_to(i64::from(id), index));
    }

    fn moveBy(&mut self, id: i32, delta: i32) {
        if let Some(to) = target_index(&self.rows, i64::from(id), delta) {
            self.moveTo(id, to as i32);
        }
    }

    fn get(&self, id: i32) -> QString {
        let value = self
            .rows
            .iter()
            .find(|r| int_at(r, 0) == i64::from(id))
            .map(|r| {
                serde_json::json!({
                    "id": int_at(r, 0), "uri": str_at(r, 1), "label": str_at(r, 2),
                    "colour": str_at(r, 3), "place": str_at(r, 4),
                })
            })
            .unwrap_or_else(|| serde_json::json!({}));
        QString::from(to_json(&value).as_str())
    }

    fn placeOf(&self, uri: QString) -> QString {
        match (core(), Uri::parse(&uri.to_string())) {
            (Some(core), Ok(uri)) => QString::from(core.place_of(&uri).as_str()),
            _ => QString::default(),
        }
    }

    fn idOf(&self, uri: QString) -> i32 {
        let uri = uri.to_string();
        self.rows
            .iter()
            .find(|r| str_at(r, 1) == uri)
            .map_or(-1, |r| int_at(r, 0) as i32)
    }
}

impl QAbstractListModel for FavouritesModel {
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

#[cfg(test)]
mod tests {
    use super::*;

    fn row(id: i64) -> Vec<Cell> {
        vec![
            Cell::Int(id),
            Cell::s("u"),
            Cell::s("l"),
            Cell::s(""),
            Cell::s("p"),
        ]
    }

    #[test]
    fn moves_are_clamped_and_noops_are_none() {
        let rows = vec![row(1), row(2), row(3)];
        assert_eq!(target_index(&rows, 2, -1), Some(0));
        assert_eq!(target_index(&rows, 2, 1), Some(2));
        assert_eq!(target_index(&rows, 1, -1), None);
        assert_eq!(target_index(&rows, 3, 5), None);
        assert_eq!(target_index(&rows, 1, 5), Some(2));
        assert_eq!(target_index(&rows, 9, 1), None);
    }

    #[test]
    fn cell_readers_are_forgiving() {
        let r = row(4);
        assert_eq!((int_at(&r, 0), int_at(&r, 1), int_at(&r, 9)), (4, -1, -1));
        assert_eq!((str_at(&r, 4), str_at(&r, 0), str_at(&r, 9)), ("p", "", ""));
    }
}
