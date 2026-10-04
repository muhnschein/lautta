// SPDX-License-Identifier: LGPL-2.1-or-later
//! Saved sync pairs (SYN-3), listed under Favourites; runs are manual.

use super::{location_prefix, move_to, next_position, parse_uri, renumber};
use crate::compare::SyncMode;
use crate::db::Db;
use crate::error::{Error, ErrorKind, Result};
use crate::uri::Uri;
use rusqlite::{params, OptionalExtension};

/// The editable part of a pair.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SyncPairSpec {
    pub label: String,
    pub left: Uri,
    pub right: Uri,
    pub mode: SyncMode,
    /// Glob patterns on relative path or name (SYN-2).
    pub excludes: Vec<String>,
    pub checksums: bool,
    pub dst_tolerance: bool,
}

impl SyncPairSpec {
    /// A pair with the safest defaults: update both ways, no excludes.
    pub fn new(label: &str, left: Uri, right: Uri) -> SyncPairSpec {
        SyncPairSpec {
            label: label.to_owned(),
            left,
            right,
            mode: SyncMode::UpdateBoth,
            excludes: Vec::new(),
            checksums: false,
            dst_tolerance: false,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SyncPair {
    pub id: i64,
    pub position: i64,
    pub spec: SyncPairSpec,
}

#[derive(Clone)]
pub struct SyncPairs {
    db: Db,
}

type RawRow = (i64, String, String, String, String, String, i64, i64, i64);

const COLUMNS: &str = "id, label, left_uri, right_uri, mode, excludes, checksums, dst_tolerance, position";

fn build(raw: RawRow) -> Result<SyncPair> {
    let (id, label, left, right, mode, excludes, checksums, dst, position) = raw;
    Ok(SyncPair {
        id,
        position,
        spec: SyncPairSpec {
            label,
            left: parse_uri(&left)?,
            right: parse_uri(&right)?,
            mode: SyncMode::parse(&mode)
                .ok_or_else(|| Error::new(ErrorKind::Internal, "unknown sync mode"))?,
            excludes: serde_json::from_str(&excludes)
                .map_err(|_| Error::new(ErrorKind::Internal, "bad excludes"))?,
            checksums: checksums != 0,
            dst_tolerance: dst != 0,
        },
    })
}

fn validate(spec: &SyncPairSpec) -> Result<String> {
    if spec.label.trim().is_empty() {
        return Err(Error::new(ErrorKind::InvalidArgument, "empty label"));
    }
    serde_json::to_string(&spec.excludes).map_err(|_| Error::kind(ErrorKind::InvalidArgument))
}

impl SyncPairs {
    pub fn new(db: Db) -> SyncPairs {
        SyncPairs { db }
    }

    pub fn add(&self, spec: &SyncPairSpec) -> Result<SyncPair> {
        let excludes = validate(spec)?;
        let conn = self.db.lock();
        let position = next_position(&conn, "sync_pairs")?;
        conn.execute(
            "INSERT INTO sync_pairs(label, left_uri, right_uri, mode, excludes, checksums,
                                    dst_tolerance, position)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8)",
            params![
                spec.label.trim(),
                spec.left.to_string(),
                spec.right.to_string(),
                spec.mode.as_str(),
                excludes,
                i64::from(spec.checksums),
                i64::from(spec.dst_tolerance),
                position
            ],
        )?;
        let mut stored = spec.clone();
        stored.label = spec.label.trim().to_owned();
        Ok(SyncPair {
            id: conn.last_insert_rowid(),
            position,
            spec: stored,
        })
    }

    pub fn update(&self, id: i64, spec: &SyncPairSpec) -> Result<()> {
        let excludes = validate(spec)?;
        let n = self.db.lock().execute(
            "UPDATE sync_pairs SET label = ?1, left_uri = ?2, right_uri = ?3, mode = ?4,
                    excludes = ?5, checksums = ?6, dst_tolerance = ?7 WHERE id = ?8",
            params![
                spec.label.trim(),
                spec.left.to_string(),
                spec.right.to_string(),
                spec.mode.as_str(),
                excludes,
                i64::from(spec.checksums),
                i64::from(spec.dst_tolerance),
                id
            ],
        )?;
        if n == 0 {
            return Err(Error::kind(ErrorKind::NotFound));
        }
        Ok(())
    }

    pub fn get(&self, id: i64) -> Result<Option<SyncPair>> {
        let conn = self.db.lock();
        let raw = conn
            .query_row(
                &format!("SELECT {COLUMNS} FROM sync_pairs WHERE id = ?1"),
                [id],
                row,
            )
            .optional()?;
        raw.map(build).transpose()
    }

    pub fn list(&self) -> Result<Vec<SyncPair>> {
        let conn = self.db.lock();
        let mut stmt = conn.prepare(&format!("SELECT {COLUMNS} FROM sync_pairs ORDER BY position, id"))?;
        let raw = stmt
            .query_map([], row)?
            .collect::<std::result::Result<Vec<_>, _>>()?;
        raw.into_iter().map(build).collect()
    }

    pub fn remove(&self, id: i64) -> Result<bool> {
        let conn = self.db.lock();
        let n = conn.execute("DELETE FROM sync_pairs WHERE id = ?1", [id])?;
        renumber(&conn, "sync_pairs")?;
        Ok(n > 0)
    }

    pub fn move_to(&self, id: i64, index: usize) -> Result<()> {
        move_to(&self.db.lock(), "sync_pairs", id, index)
    }

    /// SEC-5: drop pairs with either side in a removed location.
    pub fn remove_location(&self, location: &str) -> Result<usize> {
        let prefix = location_prefix(location);
        let conn = self.db.lock();
        let n = conn.execute(
            "DELETE FROM sync_pairs WHERE substr(left_uri, 1, length(?1)) = ?1
                OR substr(right_uri, 1, length(?1)) = ?1",
            [prefix],
        )?;
        renumber(&conn, "sync_pairs")?;
        Ok(n)
    }
}

fn row(r: &rusqlite::Row<'_>) -> rusqlite::Result<RawRow> {
    Ok((
        r.get(0)?,
        r.get(1)?,
        r.get(2)?,
        r.get(3)?,
        r.get(4)?,
        r.get(5)?,
        r.get(6)?,
        r.get(7)?,
        r.get(8)?,
    ))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn u(s: &str) -> Uri {
        Uri::parse(s).unwrap()
    }

    fn spec(label: &str) -> SyncPairSpec {
        SyncPairSpec::new(
            label,
            u("lautta://user-documents/Photos"),
            u("lautta://nv-1/backup"),
        )
    }

    #[test]
    fn add_get_list_round_trips_every_field() {
        let s = SyncPairs::new(Db::open_in_memory().unwrap());
        let mut sp = spec(" Photos ");
        sp.mode = SyncMode::MirrorLeftToRight;
        sp.excludes = vec!["*.tmp".into(), "cache/".into()];
        sp.checksums = true;
        sp.dst_tolerance = true;
        let added = s.add(&sp).unwrap();
        assert_eq!(added.spec.label, "Photos");
        let got = s.get(added.id).unwrap().unwrap();
        assert_eq!(got, added);
        assert_eq!(got.spec.mode, SyncMode::MirrorLeftToRight);
        assert_eq!(got.spec.excludes, ["*.tmp", "cache/"]);
        assert!(got.spec.checksums && got.spec.dst_tolerance);
        assert_eq!(s.list().unwrap(), vec![got]);
        assert!(s.get(999).unwrap().is_none());
    }

    #[test]
    fn defaults_are_safe() {
        let sp = spec("x");
        assert_eq!(sp.mode, SyncMode::UpdateBoth);
        assert!(!sp.checksums && !sp.dst_tolerance && sp.excludes.is_empty());
    }

    #[test]
    fn update_validates_and_persists() {
        let s = SyncPairs::new(Db::open_in_memory().unwrap());
        let p = s.add(&spec("a")).unwrap();
        let mut changed = spec("b");
        changed.mode = SyncMode::MirrorRightToLeft;
        s.update(p.id, &changed).unwrap();
        let got = s.get(p.id).unwrap().unwrap();
        assert_eq!(
            (got.spec.label.as_str(), got.spec.mode),
            ("b", SyncMode::MirrorRightToLeft)
        );
        assert_eq!(
            s.update(p.id, &spec(" ")).unwrap_err().kind,
            ErrorKind::InvalidArgument
        );
        assert_eq!(s.update(999, &spec("c")).unwrap_err().kind, ErrorKind::NotFound);
        assert_eq!(s.add(&spec("")).unwrap_err().kind, ErrorKind::InvalidArgument);
    }

    #[test]
    fn reorder_remove_and_purge() {
        let s = SyncPairs::new(Db::open_in_memory().unwrap());
        let a = s.add(&spec("a")).unwrap();
        let b = s.add(&spec("b")).unwrap();
        let c = s
            .add(&SyncPairSpec::new("c", u("lautta://x/1"), u("lautta://y/1")))
            .unwrap();
        s.move_to(c.id, 0).unwrap();
        let labels: Vec<String> = s.list().unwrap().into_iter().map(|p| p.spec.label).collect();
        assert_eq!(labels, ["c", "a", "b"]);
        assert!(s.remove(a.id).unwrap());
        assert!(!s.remove(a.id).unwrap());
        assert_eq!(s.remove_location("y").unwrap(), 1);
        let left = s.list().unwrap();
        assert_eq!(left.len(), 1);
        assert_eq!((left[0].id, left[0].position), (b.id, 0));
    }
}
