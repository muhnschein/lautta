// SPDX-License-Identifier: LGPL-2.1-or-later
//! Favourites (ORG-1): any folder, local or remote, with label and colour,
//! reorderable.

use super::{location_prefix, move_to, next_position, parse_uri, renumber, rewrite_uris};
use crate::db::Db;
use crate::error::{Error, ErrorKind, Result};
use crate::uri::Uri;
use rusqlite::{params, OptionalExtension, Row};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Favourite {
    pub id: i64,
    pub uri: Uri,
    pub label: String,
    pub colour: Option<String>,
    pub position: i64,
}

#[derive(Clone)]
pub struct Favourites {
    db: Db,
}

type RawRow = (i64, String, String, Option<String>, i64);

fn from_row(r: &Row<'_>) -> rusqlite::Result<RawRow> {
    Ok((r.get(0)?, r.get(1)?, r.get(2)?, r.get(3)?, r.get(4)?))
}

fn build(raw: RawRow) -> Result<Favourite> {
    Ok(Favourite {
        id: raw.0,
        uri: parse_uri(&raw.1)?,
        label: raw.2,
        colour: raw.3,
        position: raw.4,
    })
}

const COLUMNS: &str = "id, uri, label, colour, position";

fn clean_label(label: &str) -> Result<&str> {
    let label = label.trim();
    if label.is_empty() {
        return Err(Error::new(ErrorKind::InvalidArgument, "empty label"));
    }
    Ok(label)
}

impl Favourites {
    pub fn new(db: Db) -> Favourites {
        Favourites { db }
    }

    /// Appends a favourite. A folder can be a favourite once
    /// (`AlreadyExists`); an empty label is `InvalidArgument`.
    pub fn add(&self, uri: &Uri, label: &str, colour: Option<&str>) -> Result<Favourite> {
        let label = clean_label(label)?;
        let conn = self.db.lock();
        let key = uri.to_string();
        let exists: Option<i64> = conn
            .query_row("SELECT id FROM favourites WHERE uri = ?1", [&key], |r| r.get(0))
            .optional()?;
        if exists.is_some() {
            return Err(Error::kind(ErrorKind::AlreadyExists));
        }
        let position = next_position(&conn, "favourites")?;
        conn.execute(
            "INSERT INTO favourites(uri, label, colour, position) VALUES (?1, ?2, ?3, ?4)",
            params![key, label, colour, position],
        )?;
        Ok(Favourite {
            id: conn.last_insert_rowid(),
            uri: uri.clone(),
            label: label.to_owned(),
            colour: colour.map(str::to_owned),
            position,
        })
    }

    pub fn list(&self) -> Result<Vec<Favourite>> {
        let conn = self.db.lock();
        let mut stmt = conn.prepare(&format!("SELECT {COLUMNS} FROM favourites ORDER BY position, id"))?;
        let raw = stmt
            .query_map([], from_row)?
            .collect::<std::result::Result<Vec<_>, _>>()?;
        raw.into_iter().map(build).collect()
    }

    pub fn get_by_uri(&self, uri: &Uri) -> Result<Option<Favourite>> {
        let conn = self.db.lock();
        let raw = conn
            .query_row(
                &format!("SELECT {COLUMNS} FROM favourites WHERE uri = ?1"),
                [uri.to_string()],
                from_row,
            )
            .optional()?;
        raw.map(build).transpose()
    }

    pub fn is_favourite(&self, uri: &Uri) -> Result<bool> {
        Ok(self.get_by_uri(uri)?.is_some())
    }

    pub fn update(&self, id: i64, label: &str, colour: Option<&str>) -> Result<()> {
        let label = clean_label(label)?;
        let n = self.db.lock().execute(
            "UPDATE favourites SET label = ?1, colour = ?2 WHERE id = ?3",
            params![label, colour, id],
        )?;
        if n == 0 {
            return Err(Error::kind(ErrorKind::NotFound));
        }
        Ok(())
    }

    /// Removes by id; returns whether a row existed.
    pub fn remove(&self, id: i64) -> Result<bool> {
        let conn = self.db.lock();
        let n = conn.execute("DELETE FROM favourites WHERE id = ?1", [id])?;
        renumber(&conn, "favourites")?;
        Ok(n > 0)
    }

    pub fn remove_uri(&self, uri: &Uri) -> Result<bool> {
        let conn = self.db.lock();
        let n = conn.execute("DELETE FROM favourites WHERE uri = ?1", [uri.to_string()])?;
        renumber(&conn, "favourites")?;
        Ok(n > 0)
    }

    /// Moves a favourite to `index` (clamped to the end).
    pub fn move_to(&self, id: i64, index: usize) -> Result<()> {
        move_to(&self.db.lock(), "favourites", id, index)
    }

    /// Follows a folder moved or renamed by the app, including favourites
    /// inside it.
    pub fn on_moved(&self, old: &Uri, new: &Uri) -> Result<usize> {
        rewrite_uris(&self.db.lock(), "favourites", old, new)
    }

    /// SEC-5: forget every favourite of a removed location.
    pub fn remove_location(&self, location: &str) -> Result<usize> {
        let prefix = location_prefix(location);
        let conn = self.db.lock();
        let n = conn.execute(
            "DELETE FROM favourites WHERE substr(uri, 1, length(?1)) = ?1",
            [prefix],
        )?;
        renumber(&conn, "favourites")?;
        Ok(n)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn u(s: &str) -> Uri {
        Uri::parse(s).unwrap()
    }

    fn store() -> Favourites {
        Favourites::new(Db::open_in_memory().unwrap())
    }

    #[test]
    fn add_list_in_order_with_positions() {
        let f = store();
        let a = f.add(&u("lautta://nv-1/a"), "  A ", Some("#f00")).unwrap();
        let b = f.add(&u("lautta://nv-1/b"), "B", None).unwrap();
        assert_eq!((a.position, b.position), (0, 1));
        assert_eq!(a.label, "A");
        let list = f.list().unwrap();
        assert_eq!(list.len(), 2);
        assert_eq!(list[0].uri, u("lautta://nv-1/a"));
        assert_eq!(list[0].colour.as_deref(), Some("#f00"));
        assert_eq!(list[1].colour, None);
    }

    #[test]
    fn duplicates_and_empty_labels_are_refused() {
        let f = store();
        f.add(&u("lautta://nv-1/a"), "A", None).unwrap();
        assert_eq!(
            f.add(&u("lautta://nv-1/a"), "Again", None).unwrap_err().kind,
            ErrorKind::AlreadyExists
        );
        assert_eq!(
            f.add(&u("lautta://nv-1/b"), "   ", None).unwrap_err().kind,
            ErrorKind::InvalidArgument
        );
        assert_eq!(f.list().unwrap().len(), 1);
    }

    #[test]
    fn update_and_lookup() {
        let f = store();
        let a = f.add(&u("lautta://nv-1/a"), "A", None).unwrap();
        f.update(a.id, "Docs", Some("#0f0")).unwrap();
        let got = f.get_by_uri(&u("lautta://nv-1/a")).unwrap().unwrap();
        assert_eq!(
            (got.label.as_str(), got.colour.as_deref()),
            ("Docs", Some("#0f0"))
        );
        assert!(f.is_favourite(&u("lautta://nv-1/a")).unwrap());
        assert!(!f.is_favourite(&u("lautta://nv-1/zzz")).unwrap());
        assert_eq!(f.update(999, "x", None).unwrap_err().kind, ErrorKind::NotFound);
        assert_eq!(
            f.update(a.id, " ", None).unwrap_err().kind,
            ErrorKind::InvalidArgument
        );
    }

    #[test]
    fn reorder_and_remove_keep_positions_dense() {
        let f = store();
        let ids: Vec<i64> = ["a", "b", "c"]
            .iter()
            .map(|n| f.add(&u(&format!("lautta://x/{n}")), n, None).unwrap().id)
            .collect();
        f.move_to(ids[2], 0).unwrap();
        let order: Vec<String> = f.list().unwrap().into_iter().map(|x| x.label).collect();
        assert_eq!(order, ["c", "a", "b"]);
        assert!(f.remove(ids[0]).unwrap());
        assert!(!f.remove(ids[0]).unwrap());
        let list = f.list().unwrap();
        assert_eq!(list.iter().map(|x| x.position).collect::<Vec<_>>(), [0, 1]);
        assert!(f.remove_uri(&u("lautta://x/b")).unwrap());
        assert_eq!(f.list().unwrap().len(), 1);
    }

    #[test]
    fn remove_location_only_touches_that_location() {
        let f = store();
        f.add(&u("lautta://nv-1/a"), "a", None).unwrap();
        f.add(&u("lautta://nv-10/a"), "b", None).unwrap();
        f.add(&u("lautta://nv-1/b"), "c", None).unwrap();
        assert_eq!(f.remove_location("nv-1").unwrap(), 2);
        let left = f.list().unwrap();
        assert_eq!(left.len(), 1);
        assert_eq!(left[0].uri.location, "nv-10");
        assert_eq!(left[0].position, 0);
    }

    #[test]
    fn follows_moves() {
        let f = store();
        f.add(&u("lautta://x/a/b"), "b", None).unwrap();
        f.on_moved(&u("lautta://x/a"), &u("lautta://x/z")).unwrap();
        assert!(f.is_favourite(&u("lautta://x/z/b")).unwrap());
    }
}
