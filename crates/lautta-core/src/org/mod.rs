// SPDX-License-Identifier: LGPL-2.1-or-later
//! Favourites, recents and saved sync pairs (ORG-1, ORG-2, SYN-3). All
//! stores wrap [`Db`] and are blocking: call them from `spawn_blocking`
//! (ARC-6).

pub mod favourites;
pub mod recents;
pub mod syncpairs;

use crate::db::Db;
use crate::error::{Error, ErrorKind, Result};
use crate::uri::Uri;
use rusqlite::Connection;
use std::time::{SystemTime, UNIX_EPOCH};

pub use favourites::{Favourite, Favourites};
pub use recents::{Recent, RecentKind, Recents, RecentsFilter};
pub use syncpairs::{SyncPair, SyncPairSpec, SyncPairs};

pub(crate) fn now_ms() -> i64 {
    match SystemTime::now().duration_since(UNIX_EPOCH) {
        Ok(d) => i64::try_from(d.as_millis()).unwrap_or(i64::MAX),
        Err(_) => 0,
    }
}

pub(crate) fn parse_uri(s: &str) -> Result<Uri> {
    Uri::parse(s).map_err(|_| Error::new(ErrorKind::Internal, "stored URI is malformed"))
}

/// `lautta://<location>/`: every URI of a location starts with this, so a
/// purge can compare prefixes without LIKE wildcards (ids contain `_`).
pub(crate) fn location_prefix(location: &str) -> String {
    format!("lautta://{location}/")
}

/// Prefix of the strings of all descendants of `uri`.
pub(crate) fn descendant_prefix(uri: &Uri) -> String {
    let s = uri.to_string();
    if s.ends_with('/') {
        s
    } else {
        format!("{s}/")
    }
}

/// Next free `position` of an ordered table. `table` is always a literal.
pub(crate) fn next_position(conn: &Connection, table: &'static str) -> Result<i64> {
    let max: Option<i64> = conn.query_row(&format!("SELECT max(position) FROM {table}"), [], |r| r.get(0))?;
    Ok(max.map_or(0, |m| m + 1))
}

/// Rewrites `position` as 0..n in the current order.
pub(crate) fn renumber(conn: &Connection, table: &'static str) -> Result<()> {
    let ids = ordered_ids(conn, table)?;
    write_order(conn, table, &ids)
}

fn ordered_ids(conn: &Connection, table: &'static str) -> Result<Vec<i64>> {
    let mut stmt = conn.prepare(&format!("SELECT id FROM {table} ORDER BY position, id"))?;
    let ids = stmt
        .query_map([], |r| r.get::<_, i64>(0))?
        .collect::<std::result::Result<Vec<_>, _>>()?;
    Ok(ids)
}

fn write_order(conn: &Connection, table: &'static str, ids: &[i64]) -> Result<()> {
    let mut stmt = conn.prepare(&format!("UPDATE {table} SET position = ?1 WHERE id = ?2"))?;
    for (pos, id) in ids.iter().enumerate() {
        stmt.execute((pos as i64, id))?;
    }
    Ok(())
}

/// Moves row `id` to `index` (clamped) and renumbers.
pub(crate) fn move_to(conn: &Connection, table: &'static str, id: i64, index: usize) -> Result<()> {
    let mut ids = ordered_ids(conn, table)?;
    let from = ids
        .iter()
        .position(|x| *x == id)
        .ok_or_else(|| Error::kind(ErrorKind::NotFound))?;
    ids.remove(from);
    ids.insert(index.min(ids.len()), id);
    write_order(conn, table, &ids)
}

/// Rewrites the `uri` column of rows equal to `old` or below it so they point
/// below `new`. A row that would collide with an existing one replaces it.
/// `table` is always a literal.
pub(crate) fn rewrite_uris(conn: &Connection, table: &'static str, old: &Uri, new: &Uri) -> Result<usize> {
    let old_s = old.to_string();
    let old_prefix = descendant_prefix(old);
    let new_prefix = descendant_prefix(new);
    let new_s = new.to_string();
    let tx = conn.unchecked_transaction()?;
    let rows = select_below(&tx, table, &old_s, &old_prefix)?;
    for (rowid, uri) in &rows {
        let moved = match uri.strip_prefix(&old_prefix) {
            Some(rest) => format!("{new_prefix}{rest}"),
            None => new_s.clone(),
        };
        tx.execute(
            &format!("UPDATE OR REPLACE {table} SET uri = ?1 WHERE rowid = ?2"),
            (moved, rowid),
        )?;
    }
    tx.commit()?;
    Ok(rows.len())
}

/// Rows (rowid, uri) whose uri is `exact` or starts with `prefix`.
fn select_below(
    conn: &Connection,
    table: &'static str,
    exact: &str,
    prefix: &str,
) -> Result<Vec<(i64, String)>> {
    let mut stmt = conn.prepare(&format!(
        "SELECT rowid, uri FROM {table} WHERE uri = ?1 OR substr(uri, 1, length(?2)) = ?2"
    ))?;
    let rows = stmt
        .query_map((exact, prefix), |r| Ok((r.get(0)?, r.get(1)?)))?
        .collect::<std::result::Result<Vec<_>, _>>()?;
    Ok(rows)
}

/// What a location purge removed (SEC-5: account removal).
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct PurgeCounts {
    pub favourites: usize,
    pub recents: usize,
    pub sync_pairs: usize,
}

/// Removes everything the app remembers about `location` (SEC-5).
pub fn purge_location(db: &Db, location: &str) -> Result<PurgeCounts> {
    Ok(PurgeCounts {
        favourites: Favourites::new(db.clone()).remove_location(location)?,
        recents: Recents::new(db.clone()).purge_location(location)?,
        sync_pairs: SyncPairs::new(db.clone()).remove_location(location)?,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn prefixes() {
        assert_eq!(location_prefix("nv-1"), "lautta://nv-1/");
        assert_eq!(descendant_prefix(&Uri::root("a")), "lautta://a/");
        let u = Uri::parse("lautta://a/b").unwrap();
        assert_eq!(descendant_prefix(&u), "lautta://a/b/");
    }

    #[test]
    fn purge_location_clears_every_store() {
        let db = Db::open_in_memory().unwrap();
        let gone = Uri::parse("lautta://nv-1/x").unwrap();
        let kept = Uri::parse("lautta://user-documents/x").unwrap();
        let fav = Favourites::new(db.clone());
        fav.add(&gone, "g", None).unwrap();
        fav.add(&kept, "k", None).unwrap();
        let rec = Recents::new(db.clone());
        rec.record(&gone, "g", RecentKind::Opened).unwrap();
        rec.record(&kept, "k", RecentKind::Opened).unwrap();
        let pairs = SyncPairs::new(db.clone());
        let spec = |l: &Uri, r: &Uri| SyncPairSpec::new("p", l.clone(), r.clone());
        pairs.add(&spec(&gone, &kept)).unwrap();
        pairs.add(&spec(&kept, &kept)).unwrap();

        let counts = purge_location(&db, "nv-1").unwrap();
        assert_eq!(
            counts,
            PurgeCounts {
                favourites: 1,
                recents: 1,
                sync_pairs: 1
            }
        );
        assert_eq!(fav.list().unwrap().len(), 1);
        assert_eq!(rec.list(&RecentsFilter::default()).unwrap().len(), 1);
        assert_eq!(pairs.list().unwrap().len(), 1);
    }

    #[test]
    fn move_to_reorders_and_clamps() {
        let db = Db::open_in_memory().unwrap();
        let conn = db.lock();
        for i in 0..4 {
            conn.execute(
                "INSERT INTO favourites(id,uri,label,position) VALUES (?1, ?2, 'l', ?1)",
                (i, format!("lautta://a/{i}")),
            )
            .unwrap();
        }
        move_to(&conn, "favourites", 3, 0).unwrap();
        assert_eq!(ordered_ids(&conn, "favourites").unwrap(), vec![3, 0, 1, 2]);
        move_to(&conn, "favourites", 3, 99).unwrap();
        assert_eq!(ordered_ids(&conn, "favourites").unwrap(), vec![0, 1, 2, 3]);
        assert_eq!(
            move_to(&conn, "favourites", 42, 0).unwrap_err().kind,
            ErrorKind::NotFound
        );
        assert_eq!(next_position(&conn, "favourites").unwrap(), 4);
    }

    #[test]
    fn rewrite_uris_handles_descendants_and_collisions() {
        let db = Db::open_in_memory().unwrap();
        let conn = db.lock();
        for (i, uri) in [
            "lautta://x/a",
            "lautta://x/a/b",
            "lautta://x/ab",
            "lautta://x/z/b",
        ]
        .iter()
        .enumerate()
        {
            conn.execute(
                "INSERT INTO favourites(uri,label,position) VALUES (?1, 'l', ?2)",
                (uri, i as i64),
            )
            .unwrap();
        }
        let old = Uri::parse("lautta://x/a").unwrap();
        let new = Uri::parse("lautta://x/z").unwrap();
        assert_eq!(rewrite_uris(&conn, "favourites", &old, &new).unwrap(), 2);
        let mut stmt = conn.prepare("SELECT uri FROM favourites ORDER BY uri").unwrap();
        let uris: Vec<String> = stmt
            .query_map([], |r| r.get(0))
            .unwrap()
            .map(Result::unwrap)
            .collect();
        assert_eq!(uris, ["lautta://x/ab", "lautta://x/z", "lautta://x/z/b"]);
    }
}
