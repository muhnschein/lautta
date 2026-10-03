// SPDX-License-Identifier: LGPL-2.1-or-later
//! `TagsModel` and `TaggedItemsModel` (ORG-3): tags with colours, and the
//! items of one tag with the missing ones last.

use super::{cell_data, error_parts, role_names, store_changed, store_subscribe, Cell, Guard};
use crate::json::{from_json, to_json};
use crate::runtime::{blocking_then, core, handle, spawn_then};
use lautta_core::app::Core;
use lautta_core::app_browse::TagChanges;
use lautta_core::org::Tag;
use lautta_core::Uri;
use qmetaobject::prelude::*;
use qmetaobject::{queued_callback, QPointer};
use std::collections::HashMap;
use std::sync::Arc;

const TAG_COLUMNS: [&str; 4] = ["itemId", "name", "colour", "count"];
const ITEM_COLUMNS: [&str; 6] = ["uri", "name", "place", "missing", "section", "isDir"];

fn parse_uris(json: &str) -> Vec<Uri> {
    from_json::<Vec<String>>(json)
        .unwrap_or_default()
        .iter()
        .filter_map(|s| Uri::parse(s).ok())
        .collect()
}

fn load_tags(core: &Core) -> lautta_core::Result<Vec<Vec<Cell>>> {
    let counts: HashMap<i64, usize> = core.tags.counts()?.into_iter().collect();
    Ok(core
        .tags
        .list()?
        .into_iter()
        .map(|t| {
            vec![
                Cell::Int(t.id),
                Cell::Str(t.name),
                Cell::Str(t.colour),
                Cell::Int(counts.get(&t.id).copied().unwrap_or(0) as i64),
            ]
        })
        .collect())
}

#[derive(QObject, Default)]
pub struct TagsModel {
    base: qt_base_class!(trait QAbstractListModel),
    count: qt_property!(i32; NOTIFY countChanged),
    countChanged: qt_signal!(),

    reload: qt_method!(fn(&mut self)),
    create: qt_method!(fn(&mut self, name: QString, colour: QString)),
    update: qt_method!(fn(&mut self, id: i32, name: QString, colour: QString)),
    remove: qt_method!(fn(&mut self, id: i32)),
    /// Asks how many of the URIs (JSON list) carry each tag; the answer comes
    /// as `usageReady`.
    usageFor: qt_method!(fn(&mut self, uris_json: QString)),
    usageReady: qt_signal!(json: QString),
    /// Assigns and unassigns for the URIs: `add`/`remove` are JSON lists of
    /// tag ids, a non-empty `newName` creates that tag and assigns it too.
    apply: qt_method!(
        fn(
            &mut self,
            uris_json: QString,
            add_json: QString,
            remove_json: QString,
            new_name: QString,
            new_colour: QString,
        )
    ),
    applied: qt_signal!(),
    failed: qt_signal!(kind: QString, message: QString),

    rows: Vec<Vec<Cell>>,
    guard: Guard,
    watching: bool,
}

impl TagsModel {
    fn reload(&mut self) {
        let Some(core) = core() else { return };
        if !self.watching {
            self.watching = true;
            self.watch();
        }
        let me = QPointer::from(&*self);
        blocking_then(
            move || load_tags(&core),
            move |res| {
                let Some(model) = me.as_pinned() else { return };
                let mut model = model.borrow_mut();
                match res {
                    Ok(rows) => {
                        model.begin_reset_model();
                        model.rows = rows;
                        model.end_reset_model();
                        model.count = model.rows.len() as i32;
                        model.countChanged();
                    }
                    Err(e) => model.report(&e),
                }
            },
        );
    }

    fn watch(&mut self) {
        let me = QPointer::from(&*self);
        let notify = queued_callback(move |()| {
            if let Some(model) = me.as_pinned() {
                model.borrow_mut().reload();
            }
        });
        let mut stop = self.guard.subscribe();
        let mut store = store_subscribe();
        handle().spawn(async move {
            loop {
                tokio::select! {
                    r = store.changed() => if r.is_err() { return },
                    _ = stop.changed() => return,
                }
                notify(());
            }
        });
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
                let Some(model) = me.as_pinned() else { return };
                let model = model.borrow_mut();
                match res {
                    Ok(()) => model.applied(),
                    Err(e) => model.report(&e),
                }
                store_changed();
            },
        );
    }

    fn create(&mut self, name: QString, colour: QString) {
        let (name, colour) = (name.to_string(), colour.to_string());
        self.change(move |core| core.tags.create(&name, &colour).map(|_| ()));
    }

    fn update(&mut self, id: i32, name: QString, colour: QString) {
        let (name, colour) = (name.to_string(), colour.to_string());
        self.change(move |core| core.tags.update(i64::from(id), &name, &colour));
    }

    fn remove(&mut self, id: i32) {
        self.change(move |core| core.tags.delete(i64::from(id)).map(|_| ()));
    }

    fn usageFor(&mut self, uris_json: QString) {
        let Some(core) = core() else { return };
        let uris = parse_uris(&uris_json.to_string());
        let me = QPointer::from(&*self);
        blocking_then(
            move || core.tag_usage(&uris),
            move |res| {
                let Some(model) = me.as_pinned() else { return };
                let model = model.borrow();
                match res {
                    Ok(list) => {
                        let list: Vec<_> = list
                            .into_iter()
                            .map(|(id, count)| serde_json::json!({"id": id, "count": count}))
                            .collect();
                        model.usageReady(QString::from(to_json(&list).as_str()))
                    }
                    Err(e) => model.report(&e),
                }
            },
        );
    }

    fn apply(
        &mut self,
        uris_json: QString,
        add_json: QString,
        remove_json: QString,
        new_name: QString,
        new_colour: QString,
    ) {
        let uris = parse_uris(&uris_json.to_string());
        let new_name = new_name.to_string();
        let changes = TagChanges {
            add: from_json(&add_json.to_string()).unwrap_or_default(),
            remove: from_json(&remove_json.to_string()).unwrap_or_default(),
            new_tag: (!new_name.trim().is_empty()).then(|| (new_name, new_colour.to_string())),
        };
        self.change(move |core| core.apply_tag_changes(&uris, &changes));
    }
}

impl QAbstractListModel for TagsModel {
    fn row_count(&self) -> i32 {
        self.rows.len() as i32
    }

    fn data(&self, index: QModelIndex, role: i32) -> QVariant {
        cell_data(&self.rows, index.row(), role)
    }

    fn role_names(&self) -> HashMap<i32, QByteArray> {
        role_names(&TAG_COLUMNS)
    }
}

/// The items of one tag; the ones that are gone come last, section
/// `missing` (ORG-3).
#[derive(QObject, Default)]
pub struct TaggedItemsModel {
    base: qt_base_class!(trait QAbstractListModel),
    tagId: qt_property!(i32; NOTIFY tagIdChanged WRITE set_tag_id),
    tagIdChanged: qt_signal!(),
    tagName: qt_property!(QString; NOTIFY infoChanged),
    tagColour: qt_property!(QString; NOTIFY infoChanged),
    /// Items that are present / missing.
    count: qt_property!(i32; NOTIFY infoChanged),
    missingCount: qt_property!(i32; NOTIFY infoChanged),
    infoChanged: qt_signal!(),
    loading: qt_property!(bool; NOTIFY loadingChanged),
    loadingChanged: qt_signal!(),

    reload: qt_method!(fn(&mut self)),
    rename: qt_method!(fn(&mut self, name: QString)),
    setColour: qt_method!(fn(&mut self, colour: QString)),
    deleteTag: qt_method!(fn(&mut self)),
    removed: qt_signal!(),
    failed: qt_signal!(kind: QString, message: QString),

    rows: Vec<Vec<Cell>>,
    generation: u64,
    checked: bool,
}

type Loaded = (Option<Tag>, Vec<Vec<Cell>>);

fn load_items(core: &Core, tag_id: i64) -> lautta_core::Result<Loaded> {
    let tag = core.tags.list()?.into_iter().find(|t| t.id == tag_id);
    let rows = core
        .tagged_rows(tag_id)?
        .into_iter()
        .map(|r| {
            let section = if r.missing { "missing" } else { "items" };
            vec![
                Cell::Str(r.uri),
                Cell::Str(r.name),
                Cell::Str(r.place),
                Cell::Bool(r.missing),
                Cell::s(section),
                Cell::Bool(r.is_dir),
            ]
        })
        .collect();
    Ok((tag, rows))
}

impl TaggedItemsModel {
    fn set_tag_id(&mut self, id: i32) {
        if self.tagId == id {
            return;
        }
        self.tagId = id;
        self.checked = false;
        self.tagIdChanged();
        self.reload();
    }

    fn reload(&mut self) {
        let Some(core) = core() else { return };
        self.generation += 1;
        let generation = self.generation;
        self.loading = true;
        self.loadingChanged();
        let id = i64::from(self.tagId);
        let me = QPointer::from(&*self);
        blocking_then(
            move || load_items(&core, id),
            move |res| {
                let Some(model) = me.as_pinned() else { return };
                let mut model = model.borrow_mut();
                if model.generation != generation {
                    return;
                }
                match res {
                    Ok((tag, rows)) => model.show(tag, rows),
                    Err(e) => model.report(&e),
                }
                model.loading = false;
                model.loadingChanged();
                model.check_once();
            },
        );
    }

    fn show(&mut self, tag: Option<Tag>, rows: Vec<Vec<Cell>>) {
        self.begin_reset_model();
        self.rows = rows;
        self.end_reset_model();
        let missing = self
            .rows
            .iter()
            .filter(|r| matches!(r.get(3), Some(Cell::Bool(true))))
            .count();
        self.missingCount = missing as i32;
        self.count = (self.rows.len() - missing) as i32;
        if let Some(tag) = tag {
            self.tagName = QString::from(tag.name.as_str());
            self.tagColour = QString::from(tag.colour.as_str());
        }
        self.infoChanged();
    }

    /// Once per tag page: finds out which items are gone (ORG-3) and shows
    /// them again when something changed.
    fn check_once(&mut self) {
        if self.checked {
            return;
        }
        self.checked = true;
        let Some(core) = core() else { return };
        let me = QPointer::from(&*self);
        spawn_then(async move { core.check_tag_missing().await }, move |res| {
            let Some(model) = me.as_pinned() else { return };
            if res.is_ok() {
                model.borrow_mut().reload();
            }
        });
    }

    fn report(&self, e: &lautta_core::Error) {
        let (kind, message) = error_parts(e);
        self.failed(kind, message);
    }

    fn edit(&mut self, name: String, colour: String) {
        let Some(core) = core() else { return };
        let id = i64::from(self.tagId);
        let me = QPointer::from(&*self);
        blocking_then(
            move || core.tags.update(id, &name, &colour),
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

    fn rename(&mut self, name: QString) {
        let colour = self.tagColour.to_string();
        self.edit(name.to_string(), colour);
    }

    fn setColour(&mut self, colour: QString) {
        let name = self.tagName.to_string();
        self.edit(name, colour.to_string());
    }

    fn deleteTag(&mut self) {
        let Some(core) = core() else { return };
        let id = i64::from(self.tagId);
        let me = QPointer::from(&*self);
        blocking_then(
            move || core.tags.delete(id),
            move |res| {
                let Some(model) = me.as_pinned() else { return };
                let model = model.borrow();
                match res {
                    Ok(_) => {
                        store_changed();
                        model.removed();
                    }
                    Err(e) => model.report(&e),
                }
            },
        );
    }
}

impl QAbstractListModel for TaggedItemsModel {
    fn row_count(&self) -> i32 {
        self.rows.len() as i32
    }

    fn data(&self, index: QModelIndex, role: i32) -> QVariant {
        cell_data(&self.rows, index.row(), role)
    }

    fn role_names(&self) -> HashMap<i32, QByteArray> {
        role_names(&ITEM_COLUMNS)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn uris_are_read_leniently() {
        let uris = parse_uris(r#"["lautta://a/b","nonsense"]"#);
        assert_eq!(uris.len(), 1);
        assert!(parse_uris("not json").is_empty());
    }
}
