// SPDX-License-Identifier: LGPL-2.1-or-later
//! `BrowseLayoutModel`: the rows of Settings › Browse page (BRW-12), hidden
//! ones included, in Browse order. Roles: `section`, `kind`, `item`, `name`,
//! `hidden`, `index` (the row's place in its section), `movable`.

use super::{cell_data, error_parts, role_names, store_changed, Cell};
use crate::runtime::{blocking_then, core, spawn_then};
use lautta_core::app::Core;
use lautta_core::app_browse::LayoutRow;
use qmetaobject::prelude::*;
use qmetaobject::QPointer;
use std::collections::HashMap;
use std::sync::Arc;

const COLUMNS: [&str; 7] = ["section", "kind", "item", "name", "hidden", "index", "movable"];

/// Sections whose rows the user can reorder; volumes come and go and nearby
/// servers are one switch.
const MOVABLE: [&str; 4] = ["favourites", "device", "android", "servers"];

fn cells(rows: &[LayoutRow]) -> Vec<Vec<Cell>> {
    rows.iter()
        .enumerate()
        .map(|(i, r)| {
            let index = rows[..i].iter().filter(|o| o.section == r.section).count();
            let size = rows.iter().filter(|o| o.section == r.section).count();
            let movable = MOVABLE.contains(&r.section) && size > 1;
            vec![
                Cell::s(r.section),
                Cell::s(r.kind),
                Cell::s(&r.item),
                Cell::s(&r.name),
                Cell::Bool(r.hidden),
                Cell::Int(index as i64),
                Cell::Bool(movable),
            ]
        })
        .collect()
}

fn cell_at(rows: &[Vec<Cell>], row: i32, col: usize) -> Option<&Cell> {
    rows.get(usize::try_from(row).ok()?)?.get(col)
}

#[derive(QObject, Default)]
pub struct BrowseLayoutModel {
    base: qt_base_class!(trait QAbstractListModel),
    count: qt_property!(i32; NOTIFY countChanged),
    countChanged: qt_signal!(),

    reload: qt_method!(fn(&mut self)),
    setHidden: qt_method!(fn(&mut self, section: QString, item: QString, hidden: bool)),
    /// Moves a row to `index` within its section.
    moveTo: qt_method!(fn(&mut self, section: QString, item: QString, index: i32)),
    /// Every row shown again, in its natural order (favourites keep theirs).
    restoreDefaults: qt_method!(fn(&mut self)),
    /// The section of list row `row`, `""` outside the list.
    sectionAt: qt_method!(fn(&self, row: i32) -> QString),
    /// The place of list row `row` in its section, `-1` outside the list.
    placeAt: qt_method!(fn(&self, row: i32) -> i32),
    failed: qt_signal!(kind: QString, message: QString),

    rows: Vec<Vec<Cell>>,
}

impl BrowseLayoutModel {
    fn reload(&mut self) {
        let Some(core) = core() else { return };
        let me = QPointer::from(&*self);
        spawn_then(async move { core.browse_layout_rows().await }, move |rows| {
            if let Some(model) = me.as_pinned() {
                model.borrow_mut().show(cells(&rows));
            }
        });
    }

    fn show(&mut self, rows: Vec<Vec<Cell>>) {
        self.begin_reset_model();
        self.rows = rows;
        self.end_reset_model();
        self.count = self.rows.len() as i32;
        self.countChanged();
    }

    /// Tells the Browse page and shows the new rows, or why it failed.
    fn changed(me: &QPointer<Self>, res: lautta_core::Result<()>) {
        store_changed();
        let Some(model) = me.as_pinned() else { return };
        let mut model = model.borrow_mut();
        if let Err(e) = res {
            let (kind, message) = error_parts(&e);
            model.failed(kind, message);
        }
        model.reload();
    }

    fn change(&mut self, work: impl FnOnce(Arc<Core>) -> lautta_core::Result<()> + Send + 'static) {
        let Some(core) = core() else { return };
        let me = QPointer::from(&*self);
        blocking_then(move || work(core), move |res| Self::changed(&me, res));
    }

    fn setHidden(&mut self, section: QString, item: QString, hidden: bool) {
        let (section, item) = (section.to_string(), item.to_string());
        self.change(move |core| core.browse_layout.set_hidden(&section, &item, hidden));
    }

    fn moveTo(&mut self, section: QString, item: QString, index: i32) {
        let Some(core) = core() else { return };
        let (section, item) = (section.to_string(), item.to_string());
        let index = usize::try_from(index).unwrap_or(0);
        let me = QPointer::from(&*self);
        spawn_then(
            async move { core.move_browse_row(&section, &item, index).await },
            move |res| Self::changed(&me, res),
        );
    }

    fn restoreDefaults(&mut self) {
        self.change(|core| core.browse_layout.reset());
    }

    fn sectionAt(&self, row: i32) -> QString {
        match cell_at(&self.rows, row, 0) {
            Some(Cell::Str(s)) => QString::from(s.as_str()),
            _ => QString::default(),
        }
    }

    fn placeAt(&self, row: i32) -> i32 {
        match cell_at(&self.rows, row, 5) {
            Some(Cell::Int(i)) => *i as i32,
            _ => -1,
        }
    }
}

impl QAbstractListModel for BrowseLayoutModel {
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

    fn row(section: &'static str, kind: &'static str, item: &str) -> LayoutRow {
        LayoutRow {
            section,
            kind,
            item: item.to_owned(),
            name: item.to_uppercase(),
            hidden: item == "b",
        }
    }

    #[test]
    fn rows_know_their_place_and_whether_they_move() {
        let rows = [
            row("favourites", "favourite", "1"),
            row("device", "folder", "a"),
            row("device", "folder", "b"),
            row("device", "deleted", "deleted"),
            row("volumes", "volume", "sd"),
            row("volumes", "volume", "usb"),
            row("nearby", "nearby", "nearby"),
        ];
        let cells = cells(&rows);
        assert_eq!(cells[0].len(), COLUMNS.len());
        let pick = |i: usize| (cells[i][4].clone(), cells[i][5].clone(), cells[i][6].clone());
        assert_eq!(
            pick(0),
            (Cell::Bool(false), Cell::Int(0), Cell::Bool(false)),
            "a lone favourite"
        );
        assert_eq!(pick(1), (Cell::Bool(false), Cell::Int(0), Cell::Bool(true)));
        assert_eq!(pick(2), (Cell::Bool(true), Cell::Int(1), Cell::Bool(true)));
        assert_eq!(pick(3), (Cell::Bool(false), Cell::Int(2), Cell::Bool(true)));
        assert_eq!(
            pick(5),
            (Cell::Bool(false), Cell::Int(1), Cell::Bool(false)),
            "volumes stay put"
        );
        assert_eq!(pick(6), (Cell::Bool(false), Cell::Int(0), Cell::Bool(false)));
        assert_eq!(
            cells[2][..4],
            [Cell::s("device"), Cell::s("folder"), Cell::s("b"), Cell::s("B")]
        );
    }

    #[test]
    fn rows_are_looked_up_by_list_row() {
        let model = BrowseLayoutModel {
            rows: cells(&[row("device", "folder", "a"), row("device", "folder", "b")]),
            ..BrowseLayoutModel::default()
        };
        assert_eq!(model.sectionAt(1).to_string(), "device");
        assert_eq!((model.placeAt(0), model.placeAt(1)), (0, 1));
        assert_eq!(model.sectionAt(2).to_string(), "");
        assert_eq!((model.placeAt(2), model.placeAt(-1)), (-1, -1));
    }
}
