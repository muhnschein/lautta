// SPDX-License-Identifier: LGPL-2.1-or-later
//! Tags (ORG-3): named, coloured, stored by URI; they follow moves and
//! renames done by the app, external changes leave orphans flagged missing.

use super::{descendant_prefix, location_prefix, move_to, next_position, parse_uri, renumber, rewrite_uris};
use crate::db::Db;
use crate::error::{Error, ErrorKind, Result};
use crate::uri::Uri;
use rusqlite::{params, Connection, OptionalExtension};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Tag {
    pub id: i64,
    pub name: String,
    pub colour: String,
    pub position: i64,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TaggedItem {
    pub uri: Uri,
    pub missing: bool,
}

#[derive(Clone)]
pub struct Tags {
    db: Db,
}

fn clean_name(name: &str) -> Result<&str> {
    let name = name.trim();
    if name.is_empty() {
        return Err(Error::new(ErrorKind::InvalidArgument, "empty tag name"));
    }
    Ok(name)
}

/// Another tag (not `except`) already has `name`.
fn name_taken(conn: &Connection, name: &str, except: i64) -> Result<bool> {
    let found: Option<i64> = conn
        .query_row(
            "SELECT id FROM tags WHERE name = ?1 AND id != ?2",
            (name, except),
            |r| r.get(0),
        )
        .optional()?;
    Ok(found.is_some())
}

fn tag_from_row(r: &rusqlite::Row<'_>) -> rusqlite::Result<Tag> {
    Ok(Tag {
        id: r.get(0)?,
        name: r.get(1)?,
        colour: r.get(2)?,
        position: r.get(3)?,
    })
}

impl Tags {
    pub fn new(db: Db) -> Tags {
        Tags { db }
    }

    /// There are no default tags. Names are unique (`AlreadyExists`).
    pub fn create(&self, name: &str, colour: &str) -> Result<Tag> {
        let name = clean_name(name)?;
        let conn = self.db.lock();
        if name_taken(&conn, name, -1)? {
            return Err(Error::kind(ErrorKind::AlreadyExists));
        }
        let position = next_position(&conn, "tags")?;
        conn.execute(
            "INSERT INTO tags(name, colour, position) VALUES (?1, ?2, ?3)",
            params![name, colour, position],
        )?;
        Ok(Tag {
            id: conn.last_insert_rowid(),
            name: name.to_owned(),
            colour: colour.to_owned(),
            position,
        })
    }

    pub fn list(&self) -> Result<Vec<Tag>> {
        let conn = self.db.lock();
        let mut stmt = conn.prepare("SELECT id, name, colour, position FROM tags ORDER BY position, id")?;
        let tags = stmt
            .query_map([], tag_from_row)?
            .collect::<std::result::Result<Vec<_>, _>>()?;
        Ok(tags)
    }

    pub fn update(&self, id: i64, name: &str, colour: &str) -> Result<()> {
        let name = clean_name(name)?;
        let conn = self.db.lock();
        if name_taken(&conn, name, id)? {
            return Err(Error::kind(ErrorKind::AlreadyExists));
        }
        let n = conn.execute(
            "UPDATE tags SET name = ?1, colour = ?2 WHERE id = ?3",
            params![name, colour, id],
        )?;
        if n == 0 {
            return Err(Error::kind(ErrorKind::NotFound));
        }
        Ok(())
    }

    /// Deletes the tag and, by cascade, its assignments.
    pub fn delete(&self, id: i64) -> Result<bool> {
        let conn = self.db.lock();
        let n = conn.execute("DELETE FROM tags WHERE id = ?1", [id])?;
        renumber(&conn, "tags")?;
        Ok(n > 0)
    }

    pub fn move_to(&self, id: i64, index: usize) -> Result<()> {
        move_to(&self.db.lock(), "tags", id, index)
    }

    /// Tags `uris` (already tagged ones are kept, and are no longer missing).
    pub fn assign(&self, tag_id: i64, uris: &[Uri]) -> Result<()> {
        let conn = self.db.lock();
        let exists: Option<i64> = conn
            .query_row("SELECT id FROM tags WHERE id = ?1", [tag_id], |r| r.get(0))
            .optional()?;
        if exists.is_none() {
            return Err(Error::kind(ErrorKind::NotFound));
        }
        let tx = conn.unchecked_transaction()?;
        for uri in uris {
            tx.execute(
                "INSERT INTO item_tags(tag_id, uri, missing) VALUES (?1, ?2, 0)
                 ON CONFLICT(tag_id, uri) DO UPDATE SET missing = 0",
                (tag_id, uri.to_string()),
            )?;
        }
        tx.commit()?;
        Ok(())
    }

    pub fn unassign(&self, tag_id: i64, uris: &[Uri]) -> Result<usize> {
        let conn = self.db.lock();
        let tx = conn.unchecked_transaction()?;
        let mut removed = 0;
        for uri in uris {
            removed += tx.execute(
                "DELETE FROM item_tags WHERE tag_id = ?1 AND uri = ?2",
                (tag_id, uri.to_string()),
            )?;
        }
        tx.commit()?;
        Ok(removed)
    }

    pub fn tags_for(&self, uri: &Uri) -> Result<Vec<Tag>> {
        let conn = self.db.lock();
        let mut stmt = conn.prepare(
            "SELECT t.id, t.name, t.colour, t.position FROM tags t
             JOIN item_tags i ON i.tag_id = t.id WHERE i.uri = ?1
             ORDER BY t.position, t.id",
        )?;
        let tags = stmt
            .query_map([uri.to_string()], tag_from_row)?
            .collect::<std::result::Result<Vec<_>, _>>()?;
        Ok(tags)
    }

    pub fn items_for(&self, tag_id: i64) -> Result<Vec<TaggedItem>> {
        let conn = self.db.lock();
        let mut stmt = conn.prepare("SELECT uri, missing FROM item_tags WHERE tag_id = ?1 ORDER BY uri")?;
        let raw = stmt
            .query_map([tag_id], |r| Ok((r.get::<_, String>(0)?, r.get::<_, i64>(1)?)))?
            .collect::<std::result::Result<Vec<_>, _>>()?;
        raw.into_iter()
            .map(|(uri, missing)| {
                Ok(TaggedItem {
                    uri: parse_uri(&uri)?,
                    missing: missing != 0,
                })
            })
            .collect()
    }

    /// Follows an item moved or renamed by the app, including everything
    /// below a moved folder; moved items are present again.
    pub fn on_moved(&self, old: &Uri, new: &Uri) -> Result<usize> {
        let conn = self.db.lock();
        let n = rewrite_uris(&conn, "item_tags", old, new)?;
        conn.execute(
            "UPDATE item_tags SET missing = 0 WHERE uri = ?1 OR substr(uri, 1, length(?2)) = ?2",
            (new.to_string(), descendant_prefix(new)),
        )?;
        Ok(n)
    }

    /// Flags every assignment of `uri` (and below it) as missing.
    pub fn mark_missing(&self, uri: &Uri) -> Result<usize> {
        let n = self.db.lock().execute(
            "UPDATE item_tags SET missing = 1 WHERE uri = ?1 OR substr(uri, 1, length(?2)) = ?2",
            (uri.to_string(), descendant_prefix(uri)),
        )?;
        Ok(n)
    }

    /// Re-checks every tagged URI with `exists` (run off the database lock,
    /// it may do I/O) and updates the missing flags. Returns how many items
    /// are missing afterwards.
    pub fn check_missing(&self, mut exists: impl FnMut(&Uri) -> bool) -> Result<usize> {
        let uris = self.distinct_uris()?;
        let verdicts: Vec<(String, bool)> = uris
            .into_iter()
            .map(|(s, uri)| {
                let present = exists(&uri);
                (s, present)
            })
            .collect();
        let conn = self.db.lock();
        let tx = conn.unchecked_transaction()?;
        for (uri, present) in &verdicts {
            tx.execute(
                "UPDATE item_tags SET missing = ?1 WHERE uri = ?2",
                (i64::from(!present), uri),
            )?;
        }
        tx.commit()?;
        let missing: i64 = conn.query_row(
            "SELECT count(DISTINCT uri) FROM item_tags WHERE missing = 1",
            [],
            |r| r.get(0),
        )?;
        Ok(usize::try_from(missing).unwrap_or(0))
    }

    fn distinct_uris(&self) -> Result<Vec<(String, Uri)>> {
        let conn = self.db.lock();
        let mut stmt = conn.prepare("SELECT DISTINCT uri FROM item_tags")?;
        let raw = stmt
            .query_map([], |r| r.get::<_, String>(0))?
            .collect::<std::result::Result<Vec<_>, _>>()?;
        raw.into_iter().map(|s| parse_uri(&s).map(|u| (s, u))).collect()
    }

    /// Items flagged missing, across all tags (the *Missing* list).
    pub fn missing_items(&self) -> Result<Vec<(Tag, TaggedItem)>> {
        let mut out = Vec::new();
        for tag in self.list()? {
            for item in self.items_for(tag.id)? {
                if item.missing {
                    out.push((tag.clone(), item));
                }
            }
        }
        Ok(out)
    }

    /// Number of assigned items per tag id (missing ones included, ORG-3).
    pub fn counts(&self) -> Result<Vec<(i64, usize)>> {
        let conn = self.db.lock();
        let mut stmt = conn.prepare("SELECT tag_id, count(*) FROM item_tags GROUP BY tag_id")?;
        let rows = stmt
            .query_map([], |r| Ok((r.get::<_, i64>(0)?, r.get::<_, i64>(1)?)))?
            .collect::<std::result::Result<Vec<_>, _>>()?;
        Ok(rows
            .into_iter()
            .map(|(id, n)| (id, usize::try_from(n).unwrap_or(0)))
            .collect())
    }

    /// SEC-5: drop assignments below a removed location. Returns the number
    /// of assignments removed; tags themselves stay.
    pub fn purge_location(&self, location: &str) -> Result<usize> {
        let n = self.db.lock().execute(
            "DELETE FROM item_tags WHERE substr(uri, 1, length(?1)) = ?1",
            [location_prefix(location)],
        )?;
        Ok(n)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn u(s: &str) -> Uri {
        Uri::parse(s).unwrap()
    }

    fn store() -> Tags {
        Tags::new(Db::open_in_memory().unwrap())
    }

    #[test]
    fn no_default_tags_and_unique_names() {
        let t = store();
        assert!(t.list().unwrap().is_empty());
        let a = t.create(" Work ", "#f00").unwrap();
        assert_eq!(a.name, "Work");
        assert_eq!(
            t.create("Work", "#0f0").unwrap_err().kind,
            ErrorKind::AlreadyExists
        );
        assert_eq!(
            t.create(" ", "#0f0").unwrap_err().kind,
            ErrorKind::InvalidArgument
        );
        let b = t.create("Home", "#00f").unwrap();
        assert_eq!(b.position, 1);
        assert_eq!(
            t.update(b.id, "Work", "#00f").unwrap_err().kind,
            ErrorKind::AlreadyExists
        );
        t.update(b.id, "Home", "#111").unwrap();
        assert_eq!(t.update(99, "x", "#111").unwrap_err().kind, ErrorKind::NotFound);
        assert_eq!(t.list().unwrap()[1].colour, "#111");
    }

    #[test]
    fn reorder_and_delete_cascades() {
        let t = store();
        let a = t.create("a", "#1").unwrap();
        let b = t.create("b", "#2").unwrap();
        t.move_to(b.id, 0).unwrap();
        assert_eq!(t.list().unwrap()[0].name, "b");
        t.assign(a.id, &[u("lautta://x/f")]).unwrap();
        assert!(t.delete(a.id).unwrap());
        assert!(!t.delete(a.id).unwrap());
        assert!(t.tags_for(&u("lautta://x/f")).unwrap().is_empty());
        assert_eq!(t.list().unwrap()[0].position, 0);
    }

    #[test]
    fn assign_many_and_query_both_ways() {
        let t = store();
        let a = t.create("a", "#1").unwrap();
        let b = t.create("b", "#2").unwrap();
        let f1 = u("lautta://x/f1");
        let f2 = u("lautta://x/f2");
        t.assign(a.id, &[f1.clone(), f2.clone()]).unwrap();
        t.assign(a.id, &[f1.clone()]).unwrap();
        t.assign(b.id, &[f1.clone()]).unwrap();
        let names: Vec<String> = t.tags_for(&f1).unwrap().into_iter().map(|x| x.name).collect();
        assert_eq!(names, ["a", "b"]);
        assert_eq!(t.items_for(a.id).unwrap().len(), 2);
        assert_eq!(t.unassign(a.id, &[f1.clone(), u("lautta://x/none")]).unwrap(), 1);
        assert_eq!(t.items_for(a.id).unwrap()[0].uri, f2);
        assert_eq!(t.assign(777, &[f1]).unwrap_err().kind, ErrorKind::NotFound);
    }

    #[test]
    fn follows_move_of_folder_with_descendants() {
        let t = store();
        let tag = t.create("a", "#1").unwrap();
        t.assign(
            tag.id,
            &[
                u("lautta://x/dir"),
                u("lautta://x/dir/sub/f"),
                u("lautta://x/dirt"),
            ],
        )
        .unwrap();
        t.mark_missing(&u("lautta://x/dir")).unwrap();
        let n = t
            .on_moved(&u("lautta://x/dir"), &u("lautta://y/renamed"))
            .unwrap();
        assert_eq!(n, 2);
        let items = t.items_for(tag.id).unwrap();
        let got: Vec<(String, bool)> = items.iter().map(|i| (i.uri.to_string(), i.missing)).collect();
        assert_eq!(
            got,
            [
                ("lautta://x/dirt".to_owned(), false),
                ("lautta://y/renamed".to_owned(), false),
                ("lautta://y/renamed/sub/f".to_owned(), false),
            ]
        );
    }

    #[test]
    fn mark_and_check_missing() {
        let t = store();
        let tag = t.create("a", "#1").unwrap();
        let (keep, gone, below) = (
            u("lautta://x/keep"),
            u("lautta://x/gone"),
            u("lautta://x/gone/child"),
        );
        t.assign(tag.id, &[keep.clone(), gone.clone(), below.clone()])
            .unwrap();
        assert_eq!(t.mark_missing(&gone).unwrap(), 2);
        let missing = t.missing_items().unwrap();
        assert_eq!(missing.len(), 2);
        assert_eq!(missing[0].0.id, tag.id);

        // The checker is authoritative in both directions.
        let still = t.check_missing(|uri| *uri == gone || *uri == keep).unwrap();
        assert_eq!(still, 1);
        let flags: Vec<bool> = t.items_for(tag.id).unwrap().iter().map(|i| i.missing).collect();
        assert_eq!(flags, [false, true, false]);
        // Re-assigning a missing item brings it back.
        t.assign(tag.id, &[below]).unwrap();
        assert_eq!(t.missing_items().unwrap().len(), 0);
    }

    #[test]
    fn purge_location_keeps_tags() {
        let t = store();
        let tag = t.create("a", "#1").unwrap();
        t.assign(
            tag.id,
            &[u("lautta://nv-1/a"), u("lautta://nv-12/a"), u("lautta://nv-1/b")],
        )
        .unwrap();
        assert_eq!(t.purge_location("nv-1").unwrap(), 2);
        assert_eq!(t.items_for(tag.id).unwrap().len(), 1);
        assert_eq!(t.list().unwrap().len(), 1);
    }
}
