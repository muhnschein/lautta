// SPDX-License-Identifier: LGPL-2.1-or-later
//! Roles: `itemId`, `uri`, `label`, `colour`, `place`, `isDir`, `icon` (chosen name, empty for
//! default), `defaultIcon` (`Favourite::icon_source`: theme icon URL or glyph token).

use super::{cell_data, error_parts, role_names, store_changed, Cell};
use crate::json::to_json;
use crate::runtime::{blocking_then, core};
use lautta_core::app::Core;
use lautta_core::org::favourites::ICONS;
use lautta_core::Uri;
use qmetaobject::prelude::*;
use qmetaobject::{QPointer, QVariantList};
use std::collections::HashMap;
use std::sync::Arc;

const COLUMNS: [&str; 8] = [
    "itemId",
    "uri",
    "label",
    "colour",
    "place",
    "isDir",
    "icon",
    "defaultIcon",
];

fn load(core: &Core) -> lautta_core::Result<Vec<Vec<Cell>>> {
    Ok(core
        .favourites
        .list()?
        .into_iter()
        .map(|f| {
            let place = core.favourite_place(&f);
            let default_icon = f.default_icon();
            vec![
                Cell::Int(f.id),
                Cell::Str(f.uri.to_string()),
                Cell::Str(f.label),
                Cell::Str(f.colour.unwrap_or_default()),
                Cell::Str(place),
                Cell::Bool(f.is_dir),
                Cell::Str(f.icon.unwrap_or_default()),
                Cell::s(default_icon),
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

fn bool_at(row: &[Cell], col: usize) -> bool {
    matches!(row.get(col), Some(Cell::Bool(true)))
}

fn chosen(value: &str) -> Option<&str> {
    Some(value).filter(|c| !c.is_empty())
}

#[derive(QObject, Default)]
pub struct FavouritesModel {
    base: qt_base_class!(trait QAbstractListModel),
    count: qt_property!(i32; NOTIFY countChanged),
    countChanged: qt_signal!(),
    loaded: qt_property!(bool; NOTIFY loadedChanged),
    loadedChanged: qt_signal!(),

    reload: qt_method!(fn(&mut self)),
    add: qt_method!(fn(&mut self, uri: QString, label: QString, colour: QString)),
    /// `icon`: a name of `iconChoices`, empty for default.
    save:
        qt_method!(fn(&mut self, uri: QString, label: QString, colour: QString, icon: QString, isDir: bool)),
    edit: qt_method!(fn(&mut self, id: i32, label: QString, colour: QString)),
    remove: qt_method!(fn(&mut self, id: i32)),
    removeUri: qt_method!(fn(&mut self, uri: QString)),
    moveTo: qt_method!(fn(&mut self, id: i32, index: i32)),
    get: qt_method!(fn(&self, id: i32) -> QString),
    idOf: qt_method!(fn(&self, uri: QString) -> i32),
    placeOf: qt_method!(fn(&self, uri: QString) -> QString),
    /// Theme names (`icon-m-<name>`), default star first.
    iconChoices: qt_method!(fn(&self) -> QVariantList),
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

    fn change(&mut self, work: impl FnOnce(Arc<Core>) -> lautta_core::Result<()> + Send + 'static) {
        let Some(core) = core() else { return };
        let me = QPointer::from(&*self);
        blocking_then(
            move || work(core),
            move |res| {
                // Before the model check: a dialog's model may be gone, Browse still must see the
                // change.
                store_changed();
                let Some(model) = me.as_pinned() else { return };
                let mut model = model.borrow_mut();
                if let Err(e) = res {
                    model.report(&e);
                }
                model.reload();
            },
        );
    }

    fn add(&mut self, uri: QString, label: QString, colour: QString) {
        let (uri, label, colour) = (uri.to_string(), label.to_string(), colour.to_string());
        self.change(move |core| {
            core.favourites
                .add(&Uri::parse(&uri)?, &label, chosen(&colour), true)
                .map(|_| ())
        });
    }

    fn save(&mut self, uri: QString, label: QString, colour: QString, icon: QString, isDir: bool) {
        let (uri, label) = (uri.to_string(), label.to_string());
        let (colour, icon) = (colour.to_string(), icon.to_string());
        self.change(move |core| {
            core.favourites
                .save(&Uri::parse(&uri)?, &label, chosen(&colour), chosen(&icon), isDir)
                .map(|_| ())
        });
    }

    fn edit(&mut self, id: i32, label: QString, colour: QString) {
        let (label, colour) = (label.to_string(), colour.to_string());
        self.change(move |core| core.favourites.update(i64::from(id), &label, chosen(&colour)));
    }

    fn remove(&mut self, id: i32) {
        self.change(move |core| core.favourites.remove(i64::from(id)).map(|_| ()));
    }

    fn removeUri(&mut self, uri: QString) {
        let uri = uri.to_string();
        self.change(move |core| core.favourites.remove_uri(&Uri::parse(&uri)?).map(|_| ()));
    }

    fn moveTo(&mut self, id: i32, index: i32) {
        let index = usize::try_from(index).unwrap_or(0);
        self.change(move |core| core.favourites.move_to(i64::from(id), index));
    }

    fn get(&self, id: i32) -> QString {
        let value = self
            .rows
            .iter()
            .find(|r| int_at(r, 0) == i64::from(id))
            .map(|r| {
                serde_json::json!({
                    "id": int_at(r, 0), "uri": str_at(r, 1), "label": str_at(r, 2),
                    "colour": str_at(r, 3), "place": str_at(r, 4), "isDir": bool_at(r, 5),
                    "icon": str_at(r, 6), "defaultIcon": str_at(r, 7),
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

    fn iconChoices(&self) -> QVariantList {
        ICONS
            .iter()
            .map(|name| QVariant::from(QString::from(*name)))
            .collect()
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
            Cell::Bool(true),
            Cell::s("home"),
            Cell::s("image://theme/icon-m-favorite"),
        ]
    }

    #[test]
    fn rows_have_every_column() {
        assert_eq!(row(1).len(), COLUMNS.len());
        assert_eq!((COLUMNS[6], COLUMNS[7]), ("icon", "defaultIcon"));
    }

    #[test]
    fn empty_colour_or_icon_is_none() {
        assert_eq!(chosen(""), None);
        assert_eq!(chosen("#f00"), Some("#f00"));
        assert_eq!(chosen("home"), Some("home"));
    }

    #[test]
    fn the_icon_choices_start_with_the_star() {
        let list = FavouritesModel::default().iconChoices();
        let choices: Vec<String> = list.into_iter().map(|v| v.to_qstring().to_string()).collect();
        assert_eq!(choices.len(), ICONS.len());
        assert_eq!((choices[0].as_str(), choices[1].as_str()), ("favorite", "home"));
    }

    #[test]
    fn cell_readers_are_forgiving() {
        let r = row(4);
        assert_eq!((int_at(&r, 0), int_at(&r, 1), int_at(&r, 9)), (4, -1, -1));
        assert_eq!((str_at(&r, 4), str_at(&r, 0), str_at(&r, 9)), ("p", "", ""));
        assert_eq!(
            (bool_at(&r, 5), bool_at(&r, 4), bool_at(&r, 9)),
            (true, false, false)
        );
    }
}
