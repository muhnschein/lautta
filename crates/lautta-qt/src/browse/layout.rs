// SPDX-License-Identifier: LGPL-2.1-or-later
//! Settings › Browse rows, hidden ones included, in Browse order. Roles: `section`, `kind`, `item`,
//! `name`, `hidden`, `place` (-1 for the switch of *Volumes* or *Servers*, which leads its
//! section), `movable`, `nested`.
//!
//! Changes apply in place ([`steps`]) so scroll position and delegates survive. A dragged row moves
//! at once ([`BrowseLayoutModel::moveRow`]) and is stored behind it.

use super::{cell_data, error_parts, role_names, store_changed, watch_sources, Cell, Guard};
use crate::runtime::{core, spawn_then};
use lautta_core::app::Core;
use lautta_core::app_browse::{LayoutRow, SECTION_KIND};
use lautta_core::browse_layout::SWITCHED_SECTIONS;
use lautta_core::error::{Error, ErrorKind};
use qmetaobject::prelude::*;
use qmetaobject::QPointer;
use std::collections::{HashMap, HashSet};
use std::future::Future;
use std::sync::Arc;
use tokio::sync::oneshot;

/// `place`, not `index`: a role named `index` hides the delegate's own `index` in QML.
const COLUMNS: [&str; 8] = [
    "section", "kind", "item", "name", "hidden", "place", "movable", "nested",
];

/// Volumes come and go, nearby servers are one switch.
const MOVABLE: [&str; 4] = ["favourites", "device", "android", "servers"];

fn cells(rows: &[LayoutRow]) -> Vec<Vec<Cell>> {
    let own = |r: &LayoutRow, o: &&LayoutRow| o.section == r.section && o.kind != SECTION_KIND;
    rows.iter()
        .enumerate()
        .map(|(i, r)| {
            let switch = r.kind == SECTION_KIND;
            let index = if switch {
                -1
            } else {
                rows[..i].iter().filter(|o| own(r, o)).count() as i64
            };
            let size = rows.iter().filter(|o| own(r, o)).count();
            let movable = !switch && MOVABLE.contains(&r.section) && size > 1;
            let nested = !switch && SWITCHED_SECTIONS.contains(&r.section);
            vec![
                Cell::s(r.section),
                Cell::s(r.kind),
                Cell::s(&r.item),
                Cell::s(&r.name),
                Cell::Bool(r.hidden),
                Cell::Int(index),
                Cell::Bool(movable),
                Cell::Bool(nested),
            ]
        })
        .collect()
}

fn cell_at(rows: &[Vec<Cell>], row: i32, col: usize) -> Option<&Cell> {
    rows.get(usize::try_from(row).ok()?)?.get(col)
}

/// Places are those at the time of the step, after earlier steps.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Step {
    Remove(usize),
    Move { from: usize, to: usize },
    Insert(usize),
    Change(usize),
}

type Key<'a> = (&'a str, &'a str);

fn key(row: &[Cell]) -> Key<'_> {
    let text = |col: usize| match row.get(col) {
        Some(Cell::Str(s)) => s.as_str(),
        _ => "",
    };
    (text(0), text(2))
}

fn keys(rows: &[Vec<Cell>]) -> Option<Vec<Key<'_>>> {
    let keys: Vec<Key> = rows.iter().map(|r| key(r)).collect();
    let distinct: HashSet<&Key> = keys.iter().collect();
    (distinct.len() == keys.len()).then_some(keys)
}

/// `None` when a row's identity repeats: rows are then replaced as a whole.
fn steps(old: &[Vec<Cell>], new: &[Vec<Cell>]) -> Option<Vec<Step>> {
    let (old_keys, new_keys) = (keys(old)?, keys(new)?);
    // From the end: places of earlier rows stay valid.
    let mut out: Vec<Step> = (0..old_keys.len())
        .rev()
        .filter(|i| !new_keys.contains(&old_keys[*i]))
        .map(Step::Remove)
        .collect();
    let mut kept: Vec<Key> = old_keys
        .iter()
        .copied()
        .filter(|k| new_keys.contains(k))
        .collect();
    out.extend(moves(&mut kept, &new_keys));
    for (i, k) in new_keys.iter().enumerate() {
        match old_keys.iter().position(|o| o == k) {
            None => out.push(Step::Insert(i)),
            Some(j) if old[j] != new[i] => out.push(Step::Change(i)),
            Some(_) => {}
        }
    }
    // After every insertion: places are final.
    out.sort_by_key(|s| matches!(s, Step::Change(_)));
    Some(out)
}

/// Longest run already in order stays; every other row moves right behind the row it follows in
/// `new`.
fn moves(kept: &mut Vec<Key>, new: &[Key]) -> Vec<Step> {
    let order: Vec<Key> = new.iter().copied().filter(|k| kept.contains(k)).collect();
    let places: Vec<usize> = kept
        .iter()
        .map(|k| order.iter().position(|o| o == k).unwrap_or(0))
        .collect();
    let staying: Vec<Key> = kept
        .iter()
        .zip(in_order(&places))
        .filter_map(|(k, stays)| stays.then_some(*k))
        .collect();
    let mut out = Vec::new();
    for (t, k) in order.iter().enumerate() {
        let Some(from) = kept.iter().position(|o| o == k).filter(|_| !staying.contains(k)) else {
            continue;
        };
        let moving = kept.remove(from);
        let to = match t.checked_sub(1) {
            Some(before) => kept.iter().position(|o| *o == order[before]).map_or(0, |p| p + 1),
            None => 0,
        };
        kept.insert(to, moving);
        if from != to {
            out.push(Step::Move { from, to });
        }
    }
    out
}

fn in_order(places: &[usize]) -> Vec<bool> {
    let mut runs: Vec<(usize, Option<usize>)> = Vec::with_capacity(places.len());
    for (i, place) in places.iter().enumerate() {
        let best = (0..i)
            .filter(|j| places[*j] < *place)
            .max_by_key(|j| runs[*j].0)
            .map_or((1, None), |j| (runs[j].0 + 1, Some(j)));
        runs.push(best);
    }
    let mut stays = vec![false; places.len()];
    let mut at = (0..runs.len()).max_by_key(|i| runs[*i].0);
    while let Some(i) = at {
        stays[i] = true;
        at = runs[i].1;
    }
    stays
}

fn carried(rows: &[Vec<Cell>], from: i32, to: i32) -> Option<(String, String, usize)> {
    let text = |row: i32, col: usize| match cell_at(rows, row, col) {
        Some(Cell::Str(s)) => Some(s.clone()),
        _ => None,
    };
    let section = text(from, 0).filter(|s| text(to, 0).as_ref() == Some(s))?;
    let item = text(from, 2).filter(|_| from != to)?;
    let place = match (cell_at(rows, from, 6), cell_at(rows, to, 5)) {
        (Some(Cell::Bool(true)), Some(Cell::Int(p))) => usize::try_from(*p).ok()?,
        _ => return None,
    };
    Some((section, item, place))
}

/// After a move within the section the places stay where they were, rows changed.
fn renumber(rows: &mut [Vec<Cell>], first: usize, last: usize, base: i64) {
    for (place, row) in (base..).zip(rows[first..=last].iter_mut()) {
        if let Some(cell) = row.get_mut(5) {
            *cell = Cell::Int(place);
        }
    }
}

fn apply_step(rows: &mut Vec<Vec<Cell>>, new: &[Vec<Cell>], step: Step) {
    match step {
        Step::Remove(i) => {
            rows.remove(i);
        }
        Step::Move { from, to } => {
            let row = rows.remove(from);
            rows.insert(to, row);
        }
        Step::Insert(i) => rows.insert(i, new[i].clone()),
        Step::Change(i) => rows[i] = new[i].clone(),
    }
}

#[derive(QObject, Default)]
pub struct BrowseLayoutModel {
    base: qt_base_class!(trait QAbstractListModel),
    count: qt_property!(i32; NOTIFY countChanged),
    countChanged: qt_signal!(),

    reload: qt_method!(fn(&mut self)),
    setHidden: qt_method!(fn(&mut self, section: QString, item: QString, hidden: bool)),
    moveTo: qt_method!(fn(&mut self, section: QString, item: QString, index: i32)),
    moveRow: qt_method!(fn(&mut self, from: i32, to: i32) -> bool),
    restoreDefaults: qt_method!(fn(&mut self)),
    sectionAt: qt_method!(fn(&self, row: i32) -> QString),
    placeAt: qt_method!(fn(&self, row: i32) -> i32),
    itemAt: qt_method!(fn(&self, row: i32) -> QString),
    failed: qt_signal!(kind: QString, message: QString),

    rows: Vec<Vec<Cell>>,
    /// Each change waits for the one before: switch and Restore defaults apply in order made.
    last: Option<oneshot::Receiver<()>>,
    /// Older reload ending later is dropped (a switch would flip back). A change makes running
    /// reloads old too.
    generation: u64,
    /// Rows are read again only once all are stored: reading in between moves a dragged row back.
    pending: usize,
    watching: bool,
    guard: Guard,
}

async fn blocking(
    work: impl FnOnce() -> lautta_core::Result<()> + Send + 'static,
) -> lautta_core::Result<()> {
    tokio::task::spawn_blocking(work)
        .await
        .map_err(|e| Error::new(ErrorKind::Internal, e.to_string()))?
}

impl BrowseLayoutModel {
    /// First call also starts following what Browse follows.
    fn reload(&mut self) {
        self.generation += 1;
        let generation = self.generation;
        let Some(core) = core() else { return };
        if !self.watching {
            self.watching = true;
            watch_sources(
                QPointer::from(&*self),
                &core,
                &self.guard,
                Self::reload_when_stored,
            );
        }
        let me = QPointer::from(&*self);
        spawn_then(async move { core.browse_layout_rows().await }, move |rows| {
            if let Some(model) = me.as_pinned() {
                let mut model = model.borrow_mut();
                if model.generation == generation {
                    model.show(cells(&rows));
                }
            }
        });
    }

    fn reload_when_stored(&mut self) {
        if self.pending == 0 {
            self.reload();
        }
    }

    fn show(&mut self, rows: Vec<Vec<Cell>>) {
        match steps(&self.rows, &rows).filter(|_| !self.rows.is_empty()) {
            Some(steps) => steps.into_iter().for_each(|s| self.make(s, &rows)),
            None => {
                self.begin_reset_model();
                self.rows = rows;
                self.end_reset_model();
            }
        }
        self.count = self.rows.len() as i32;
        self.countChanged();
    }

    fn make(&mut self, step: Step, new: &[Vec<Cell>]) {
        match step {
            Step::Remove(i) => self.begin_remove_rows(i as i32, i as i32),
            Step::Insert(i) => self.begin_insert_rows(i as i32, i as i32),
            // Qt names the place before the move: past `to` when moving down.
            Step::Move { from, to } => {
                let top = QModelIndex::default();
                let before = if to > from { to + 1 } else { to };
                self.begin_move_rows(top, from as i32, from as i32, top, before as i32);
            }
            Step::Change(_) => {}
        }
        apply_step(&mut self.rows, new, step);
        match step {
            Step::Remove(_) => self.end_remove_rows(),
            Step::Insert(_) => self.end_insert_rows(),
            Step::Move { .. } => self.end_move_rows(),
            Step::Change(i) => {
                let index = self.row_index(i as i32);
                self.data_changed(index, index);
            }
        }
    }

    fn changed(me: &QPointer<Self>, res: lautta_core::Result<()>) {
        store_changed();
        let Some(model) = me.as_pinned() else { return };
        let mut model = model.borrow_mut();
        if let Err(e) = res {
            let (kind, message) = error_parts(&e);
            model.failed(kind, message);
        }
        model.pending = model.pending.saturating_sub(1);
        model.reload_when_stored();
    }

    fn change<F>(&mut self, work: impl FnOnce(Arc<Core>) -> F + Send + 'static)
    where
        F: Future<Output = lautta_core::Result<()>> + Send + 'static,
    {
        let Some(core) = core() else { return };
        self.pending += 1;
        self.generation += 1;
        let before = self.last.take();
        let (stored, after) = oneshot::channel();
        self.last = Some(after);
        let me = QPointer::from(&*self);
        spawn_then(
            async move {
                if let Some(before) = before {
                    let _ = before.await;
                }
                let res = work(core).await;
                let _ = stored.send(());
                res
            },
            move |res| Self::changed(&me, res),
        );
    }

    fn setHidden(&mut self, section: QString, item: QString, hidden: bool) {
        let (section, item) = (section.to_string(), item.to_string());
        self.change(move |core| blocking(move || core.browse_layout.set_hidden(&section, &item, hidden)));
    }

    fn moveTo(&mut self, section: QString, item: QString, index: i32) {
        let (section, item) = (section.to_string(), item.to_string());
        let index = usize::try_from(index).unwrap_or(0);
        self.change(move |core| async move { core.move_browse_row(&section, &item, index).await });
    }

    fn moveRow(&mut self, from: i32, to: i32) -> bool {
        let Some((section, item, place)) = carried(&self.rows, from, to) else {
            return false;
        };
        let (from, to) = (from as usize, to as usize);
        let (first, last) = (from.min(to), from.max(to));
        let base = match self.rows[first].get(5) {
            Some(Cell::Int(p)) => *p,
            _ => 0,
        };
        self.make(Step::Move { from, to }, &[]);
        renumber(&mut self.rows, first, last, base);
        let (top, bottom) = (self.row_index(first as i32), self.row_index(last as i32));
        self.data_changed(top, bottom);
        self.change(move |core| async move { core.move_browse_row(&section, &item, place).await });
        true
    }

    fn restoreDefaults(&mut self) {
        self.change(|core| blocking(move || core.browse_layout.reset()));
    }

    fn text_at(&self, row: i32, col: usize) -> QString {
        match cell_at(&self.rows, row, col) {
            Some(Cell::Str(s)) => QString::from(s.as_str()),
            _ => QString::default(),
        }
    }

    fn sectionAt(&self, row: i32) -> QString {
        self.text_at(row, 0)
    }

    fn itemAt(&self, row: i32) -> QString {
        self.text_at(row, 2)
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

    fn switch(section: &'static str) -> LayoutRow {
        LayoutRow {
            section,
            kind: SECTION_KIND,
            item: "section".to_owned(),
            name: String::new(),
            hidden: false,
        }
    }

    fn shape(rows: &[LayoutRow]) -> Vec<(Cell, Cell, Cell)> {
        cells(rows)
            .into_iter()
            .map(|c| (c[5].clone(), c[6].clone(), c[7].clone()))
            .collect()
    }

    #[test]
    fn a_section_switch_leads_its_section_and_takes_no_place() {
        let (no, yes) = (Cell::Bool(false), Cell::Bool(true));
        let rows = [
            row("device", "folder", "a"),
            switch("volumes"),
            row("volumes", "volume", "sd"),
            row("volumes", "volume", "usb"),
            switch("servers"),
            row("servers", "account", "nv-account:1"),
            row("servers", "account", "nv-account:2"),
        ];
        assert_eq!(
            shape(&rows),
            [
                (Cell::Int(0), no.clone(), no.clone()),
                (Cell::Int(-1), no.clone(), no.clone()),
                (Cell::Int(0), no.clone(), yes.clone()),
                (Cell::Int(1), no.clone(), yes.clone()),
                (Cell::Int(-1), no.clone(), no.clone()),
                (Cell::Int(0), yes.clone(), yes.clone()),
                (Cell::Int(1), yes.clone(), yes.clone()),
            ]
        );
        assert_eq!(
            shape(&rows[4..6]),
            [(Cell::Int(-1), no.clone(), no.clone()), (Cell::Int(0), no, yes)]
        );
    }

    #[test]
    fn a_carried_row_stops_below_the_switch() {
        let rows = cells(&[
            switch("servers"),
            row("servers", "account", "nv-account:1"),
            row("servers", "account", "nv-account:2"),
        ]);
        assert_eq!(
            carried(&rows, 2, 1),
            Some(("servers".into(), "nv-account:2".into(), 0))
        );
        assert_eq!(carried(&rows, 1, 0), None, "not onto the switch");
        assert_eq!(carried(&rows, 0, 1), None, "the switch does not move");
    }

    #[test]
    fn no_role_hides_the_delegate_index() {
        assert!(!COLUMNS.contains(&"index") && !COLUMNS.contains(&"model"));
        assert_eq!(COLUMNS[5], "place", "placeAt reads this column");
    }

    #[test]
    fn rows_are_looked_up_by_list_row() {
        let model = BrowseLayoutModel {
            rows: cells(&[row("device", "folder", "a"), row("device", "folder", "b")]),
            ..BrowseLayoutModel::default()
        };
        assert_eq!(model.sectionAt(1).to_string(), "device");
        assert_eq!(
            (model.itemAt(0).to_string(), model.itemAt(1).to_string()),
            ("a".into(), "b".into())
        );
        assert_eq!(model.itemAt(2).to_string(), "");
        assert_eq!((model.placeAt(0), model.placeAt(1)), (0, 1));
        assert_eq!(model.sectionAt(2).to_string(), "");
        assert_eq!((model.placeAt(2), model.placeAt(-1)), (-1, -1));
    }

    #[test]
    fn a_source_change_waits_for_a_change_of_the_users_own() {
        let mut model = BrowseLayoutModel {
            pending: 1,
            ..BrowseLayoutModel::default()
        };
        model.reload_when_stored();
        assert_eq!(model.generation, 0, "no read while a change is not stored");
        model.pending = 0;
        model.reload_when_stored();
        assert_eq!(model.generation, 1, "read once all are stored");
    }

    fn named(names: &str) -> Vec<Vec<Cell>> {
        names
            .chars()
            .map(|c| {
                vec![
                    Cell::s("device"),
                    Cell::s("folder"),
                    Cell::Str(c.to_ascii_lowercase().to_string()),
                    Cell::s(""),
                    Cell::Bool(c.is_ascii_uppercase()),
                ]
            })
            .collect()
    }

    fn replayed(old: &[Vec<Cell>], new: &[Vec<Cell>]) -> Vec<Vec<Cell>> {
        let mut rows = old.to_vec();
        for step in steps(old, new).unwrap() {
            apply_step(&mut rows, new, step);
        }
        rows
    }

    fn steps_of(old: &str, new: &str) -> Vec<Step> {
        steps(&named(old), &named(new)).unwrap()
    }

    #[test]
    fn a_switch_changes_its_row_only() {
        assert_eq!(steps_of("abc", "aBc"), [Step::Change(1)]);
        assert_eq!(steps_of("aBc", "abc"), [Step::Change(1)]);
        assert!(steps_of("abc", "abc").is_empty());
    }

    #[test]
    fn a_dragged_row_is_one_move() {
        assert_eq!(steps_of("abcd", "bcda"), [Step::Move { from: 0, to: 3 }]);
        assert_eq!(steps_of("abcd", "dabc"), [Step::Move { from: 3, to: 0 }]);
        assert_eq!(steps_of("abcd", "acbd"), [Step::Move { from: 1, to: 2 }]);
        assert_eq!(
            steps_of("abcd", "bAcd"),
            [Step::Move { from: 0, to: 1 }, Step::Change(1)]
        );
    }

    #[test]
    fn rows_come_and_go_where_they_are() {
        assert_eq!(steps_of("abc", "ac"), [Step::Remove(1)]);
        assert_eq!(steps_of("abc", "abxc"), [Step::Insert(2)]);
        assert_eq!(
            steps_of("abcd", "xbdy"),
            [Step::Remove(2), Step::Remove(0), Step::Insert(0), Step::Insert(3)]
        );
        assert_eq!(steps_of("ab", "Bxa").last(), Some(&Step::Change(0)));
    }

    #[test]
    fn every_change_ends_in_the_new_rows() {
        let words = [
            "", "a", "ab", "ba", "abc", "cab", "bca", "abcd", "dcba", "badc", "cdab", "aBcD",
        ];
        let more = ["xa", "axyb", "dxcy", "e", "Cxbea", "dCbAe", "edcba", "bdfac"];
        for old in words.iter().chain(&more) {
            for new in words.iter().chain(&more) {
                let (o, n) = (named(old), named(new));
                assert_eq!(replayed(&o, &n), n, "{old} → {new}");
            }
        }
    }

    #[test]
    fn rows_that_keep_their_order_do_not_move() {
        let moves = |old: &str, new: &str| {
            steps_of(old, new)
                .iter()
                .filter(|s| matches!(s, Step::Move { .. }))
                .count()
        };
        assert_eq!(moves("abcdef", "abcdef"), 0);
        assert_eq!(moves("abcdef", "bcdefa"), 1);
        assert_eq!(moves("abcdef", "badcfe"), 3);
        assert_eq!(moves("abcdef", "fedcba"), 5);
        assert_eq!(
            moves("abxcdef", "aybcdfe"),
            1,
            "removals and insertions are not moves"
        );
    }

    #[test]
    fn the_longest_run_in_order_stays() {
        assert_eq!(in_order(&[3, 0, 1, 2]), [false, true, true, true]);
        assert_eq!(in_order(&[1, 2, 3, 0]), [true, true, true, false]);
        assert_eq!(in_order(&[0, 2, 1, 3]).iter().filter(|s| **s).count(), 3);
        assert!(in_order(&[]).is_empty());
    }

    fn device(names: &str) -> Vec<Vec<Cell>> {
        cells(
            &names
                .chars()
                .map(|c| row("device", "folder", &c.to_string()))
                .collect::<Vec<_>>(),
        )
    }

    #[test]
    fn a_carried_row_moves_within_its_section_only() {
        let rows = [
            cells(&[row("favourites", "favourite", "1")]),
            device("acd"),
            cells(&[row("volumes", "volume", "sd"), row("volumes", "volume", "usb")]),
        ]
        .concat();
        let carry = |from, to| carried(&rows, from, to);
        assert_eq!(carry(1, 3), Some(("device".into(), "a".into(), 2)));
        assert_eq!(carry(3, 2), Some(("device".into(), "d".into(), 1)));
        assert_eq!(
            (carry(1, 0), carry(0, 1)),
            (None, None),
            "not into another section"
        );
        assert_eq!(carry(4, 5), None, "volumes stay put");
        assert_eq!(carry(2, 2), None, "nowhere to go");
        assert_eq!((carry(3, 6), carry(-1, 1)), (None, None), "rows outside the list");
    }

    #[test]
    fn places_follow_a_move() {
        let mut rows = [cells(&[row("favourites", "favourite", "1")]), device("abcd")].concat();
        apply_step(&mut rows, &[], Step::Move { from: 1, to: 3 });
        renumber(&mut rows, 1, 3, 0);
        let order: Vec<_> = rows.iter().map(|r| (r[2].clone(), r[5].clone())).collect();
        assert_eq!(
            order,
            [
                (Cell::s("1"), Cell::Int(0)),
                (Cell::s("b"), Cell::Int(0)),
                (Cell::s("c"), Cell::Int(1)),
                (Cell::s("a"), Cell::Int(2)),
                (Cell::s("d"), Cell::Int(3)),
            ]
        );
    }

    #[test]
    fn a_repeated_row_is_replaced_as_a_whole() {
        assert_eq!(steps(&named("aa"), &named("a")), None);
        assert_eq!(steps(&named("a"), &named("bb")), None);
        let other_section = vec![vec![Cell::s("android"), Cell::s("folder"), Cell::s("a")]];
        let rows = [named("a"), other_section].concat();
        assert_eq!(
            steps(&rows, &rows),
            Some(vec![]),
            "the section is part of the identity"
        );
    }
}
