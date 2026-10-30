// SPDX-License-Identifier: LGPL-2.1-or-later
//! Browse page layout: rows the user hid and the order they gave a section's rows. Sections keep a
//! fixed order; favourites keep their own.
//!
//! A row is named by section and item: location id, `recents`, `deleted`, `nearby` (all nearby
//! servers). *Volumes* and *Servers* have a section switch, stored as hidden flag of item
//! `section`; its rows' flags stay while it is off. Blocking: call from `spawn_blocking`.

use crate::db::Db;
use crate::error::Result;
use rusqlite::params;
use std::collections::{BTreeMap, BTreeSet};

pub const DELETED_ITEM: &str = "deleted";
pub const RECENTS_ITEM: &str = "recents";
pub const NEARBY_ITEM: &str = "nearby";
/// No location id is called this.
pub const SECTION_ITEM: &str = "section";
pub const SWITCHED_SECTIONS: [&str; 2] = ["volumes", "servers"];

pub fn layout_item(kind: &str, item_id: &str) -> Option<String> {
    match kind {
        "deleted" => Some(DELETED_ITEM.to_owned()),
        "recents" => Some(RECENTS_ITEM.to_owned()),
        "nearby" => Some(NEARBY_ITEM.to_owned()),
        "folder" | "volume" | "account" | "adhoc" | "adhocRecent" => Some(item_id.to_owned()),
        _ => None,
    }
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Layout {
    hidden: BTreeSet<(String, String)>,
    positions: BTreeMap<(String, String), i64>,
}

impl Layout {
    pub fn is_hidden(&self, section: &str, item: &str) -> bool {
        self.hidden.contains(&(section.to_owned(), item.to_owned()))
    }

    pub fn section_shown(&self, section: &str) -> bool {
        !SWITCHED_SECTIONS.contains(&section) || !self.is_hidden(section, SECTION_ITEM)
    }

    pub fn position(&self, section: &str, item: &str) -> Option<i64> {
        self.positions
            .get(&(section.to_owned(), item.to_owned()))
            .copied()
    }

    /// Rows with a stored place first, by place; others after, in arrival order. `None` from `key`:
    /// keep it where it is among the unplaced rows.
    pub fn arrange<T>(&self, rows: Vec<T>, key: impl Fn(&T) -> (String, Option<String>)) -> Vec<T> {
        let mut out: Vec<T> = Vec::with_capacity(rows.len());
        let mut group: Vec<(Option<i64>, usize, T)> = Vec::new();
        let mut current: Option<String> = None;
        for row in rows {
            let (section, item) = key(&row);
            if current.as_deref() != Some(section.as_str()) {
                flush(&mut group, &mut out);
                current = Some(section.clone());
            }
            let place = item.and_then(|i| self.position(&section, &i));
            let index = group.len();
            group.push((place, index, row));
        }
        flush(&mut group, &mut out);
        out
    }
}

fn flush<T>(group: &mut Vec<(Option<i64>, usize, T)>, out: &mut Vec<T>) {
    group.sort_by_key(|(place, index, _)| (place.is_none(), *place, *index));
    out.extend(group.drain(..).map(|(_, _, row)| row));
}

#[derive(Clone)]
pub struct BrowseLayout {
    db: Db,
}

impl BrowseLayout {
    pub fn new(db: Db) -> BrowseLayout {
        BrowseLayout { db }
    }

    pub fn load(&self) -> Result<Layout> {
        let conn = self.db.lock();
        let mut stmt = conn.prepare("SELECT section, item, hidden, position FROM browse_layout")?;
        let rows = stmt
            .query_map([], |r| {
                Ok((
                    r.get::<_, String>(0)?,
                    r.get::<_, String>(1)?,
                    r.get::<_, i64>(2)?,
                    r.get::<_, Option<i64>>(3)?,
                ))
            })?
            .collect::<std::result::Result<Vec<_>, _>>()?;
        let mut layout = Layout::default();
        for (section, item, hidden, position) in rows {
            let key = (section, item);
            if hidden != 0 {
                layout.hidden.insert(key.clone());
            }
            if let Some(p) = position {
                layout.positions.insert(key, p);
            }
        }
        Ok(layout)
    }

    pub fn set_hidden(&self, section: &str, item: &str, hidden: bool) -> Result<()> {
        let conn = self.db.lock();
        conn.execute(
            "INSERT INTO browse_layout(section, item, hidden) VALUES (?1, ?2, ?3) \
             ON CONFLICT(section, item) DO UPDATE SET hidden = excluded.hidden",
            params![section, item, i64::from(hidden)],
        )?;
        tidy(&conn)
    }

    /// Rows not in `items` lose their place.
    pub fn set_order(&self, section: &str, items: &[String]) -> Result<()> {
        let mut conn = self.db.lock();
        let tx = conn.transaction()?;
        tx.execute(
            "UPDATE browse_layout SET position = NULL WHERE section = ?1",
            [section],
        )?;
        for (position, item) in items.iter().enumerate() {
            tx.execute(
                "INSERT INTO browse_layout(section, item, position) VALUES (?1, ?2, ?3) \
                 ON CONFLICT(section, item) DO UPDATE SET position = excluded.position",
                params![section, item, position as i64],
            )?;
        }
        tidy(&tx)?;
        tx.commit()?;
        Ok(())
    }

    pub fn reset(&self) -> Result<()> {
        self.db.lock().execute("DELETE FROM browse_layout", [])?;
        Ok(())
    }

    pub fn forget(&self, item: &str) -> Result<()> {
        self.db
            .lock()
            .execute("DELETE FROM browse_layout WHERE item = ?1", [item])?;
        Ok(())
    }
}

/// Rows that say nothing (shown, no place) are not kept.
fn tidy(conn: &rusqlite::Connection) -> Result<()> {
    conn.execute(
        "DELETE FROM browse_layout WHERE hidden = 0 AND position IS NULL",
        [],
    )?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn store() -> BrowseLayout {
        BrowseLayout::new(Db::open_in_memory().unwrap())
    }

    fn rows(list: &[(&str, &str)]) -> Vec<(String, String)> {
        list.iter().map(|(s, i)| (s.to_string(), i.to_string())).collect()
    }

    fn arranged(layout: &Layout, list: &[(&str, &str)]) -> Vec<String> {
        layout
            .arrange(rows(list), |(s, i)| {
                (s.clone(), (!i.is_empty()).then(|| i.clone()))
            })
            .into_iter()
            .map(|(_, i)| i)
            .collect()
    }

    fn count(s: &BrowseLayout) -> i64 {
        s.db.lock()
            .query_row("SELECT count(*) FROM browse_layout", [], |r| r.get(0))
            .unwrap()
    }

    #[test]
    fn items_of_rows() {
        assert_eq!(layout_item("deleted", "").as_deref(), Some("deleted"));
        assert_eq!(layout_item("recents", "").as_deref(), Some("recents"));
        assert_eq!(layout_item("nearby", "3").as_deref(), Some("nearby"));
        for kind in ["folder", "volume", "account", "adhoc", "adhocRecent"] {
            assert_eq!(layout_item(kind, "x").as_deref(), Some("x"), "{kind}");
        }
        for kind in ["favourite", "consent", "unavailable", ""] {
            assert_eq!(layout_item(kind, "x"), None, "{kind}");
        }
    }

    #[test]
    fn hiding_and_showing_again() {
        let s = store();
        assert!(!s.load().unwrap().is_hidden("device", "user-music"));
        s.set_hidden("device", "user-music", true).unwrap();
        s.set_hidden("android", "android-dcim", true).unwrap();
        let l = s.load().unwrap();
        assert!(l.is_hidden("device", "user-music"));
        assert!(!l.is_hidden("android", "user-music"), "per section");
        assert!(l.is_hidden("android", "android-dcim"));
        s.set_hidden("device", "user-music", false).unwrap();
        assert!(!s.load().unwrap().is_hidden("device", "user-music"));
        assert_eq!(count(&s), 1, "a shown row without a place is not kept");
    }

    #[test]
    fn order_is_stored_per_section() {
        let s = store();
        let order: Vec<String> = ["c", "a", "b"].iter().map(|x| x.to_string()).collect();
        s.set_order("device", &order).unwrap();
        s.set_hidden("device", "a", true).unwrap();
        let l = s.load().unwrap();
        assert_eq!(
            (
                l.position("device", "c"),
                l.position("device", "a"),
                l.position("device", "b")
            ),
            (Some(0), Some(1), Some(2))
        );
        assert_eq!(l.position("servers", "c"), None);
        s.set_order("device", &["b".to_owned()]).unwrap();
        let l = s.load().unwrap();
        assert_eq!(
            (l.position("device", "b"), l.position("device", "c")),
            (Some(0), None)
        );
        assert!(l.is_hidden("device", "a") && l.position("device", "a").is_none());
        assert_eq!(count(&s), 2);
    }

    #[test]
    fn arrange_sorts_within_sections_only() {
        let s = store();
        s.set_order("device", &["b".to_owned(), "a".to_owned()]).unwrap();
        s.set_order("servers", &["y".to_owned()]).unwrap();
        let l = s.load().unwrap();
        let list = [
            ("favourites", ""),
            ("favourites", ""),
            ("device", "a"),
            ("device", "new"),
            ("device", "b"),
            ("device", "other"),
            ("servers", "x"),
            ("servers", "y"),
            ("nearby", "nearby"),
        ];
        assert_eq!(
            arranged(&l, &list),
            ["", "", "b", "a", "new", "other", "y", "x", "nearby"]
        );
        assert_eq!(
            arranged(&Layout::default(), &list[2..6]),
            ["a", "new", "b", "other"]
        );
    }

    #[test]
    fn a_section_switch_keeps_the_choices_of_its_rows() {
        let s = store();
        s.set_hidden("servers", "nv-account:2", true).unwrap();
        s.set_order("servers", &["nv-account:2".to_owned(), "nv-account:1".to_owned()])
            .unwrap();
        s.set_hidden("servers", SECTION_ITEM, true).unwrap();
        let off = s.load().unwrap();
        assert!(!off.section_shown("servers"));
        assert!(off.section_shown("volumes"), "per section");
        assert!(off.is_hidden("servers", "nv-account:2"));
        assert_eq!(off.position("servers", "nv-account:1"), Some(1));
        assert_eq!(count(&s), 3);
        s.set_hidden("servers", SECTION_ITEM, false).unwrap();
        let on = s.load().unwrap();
        assert!(on.section_shown("servers"));
        assert!(on.is_hidden("servers", "nv-account:2"), "the row stays hidden");
        assert_eq!(
            on.position("servers", "nv-account:2"),
            Some(0),
            "and in its place"
        );
        assert_eq!(count(&s), 2, "a section that is on is not kept");
    }

    #[test]
    fn only_volumes_and_servers_have_a_switch() {
        let s = store();
        let others = ["device", "android", "favourites", "nearby"];
        for section in others {
            s.set_hidden(section, SECTION_ITEM, true).unwrap();
        }
        let l = s.load().unwrap();
        for section in others {
            assert!(l.section_shown(section), "{section}");
        }
        s.reset().unwrap();
        for section in SWITCHED_SECTIONS {
            s.set_hidden(section, SECTION_ITEM, true).unwrap();
        }
        let l = s.load().unwrap();
        assert!(!l.section_shown("volumes") && !l.section_shown("servers"));
        s.reset().unwrap();
        let l = s.load().unwrap();
        assert!(
            l.section_shown("volumes") && l.section_shown("servers"),
            "defaults: on"
        );
    }

    #[test]
    fn reset_and_forget() {
        let s = store();
        s.set_hidden("servers", "nv-account:1", true).unwrap();
        s.set_order("device", &["a".to_owned(), "nv-account:1".to_owned()])
            .unwrap();
        assert_eq!(count(&s), 3);
        s.forget("nv-account:1").unwrap();
        let l = s.load().unwrap();
        assert_eq!(count(&s), 1);
        assert_eq!(l.position("device", "a"), Some(0));
        s.set_hidden("device", "b", true).unwrap();
        s.reset().unwrap();
        assert_eq!(s.load().unwrap(), Layout::default());
        assert_eq!(count(&s), 0);
    }
}
