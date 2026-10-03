// SPDX-License-Identifier: LGPL-2.1-or-later
//! Recents (ORG-2): files opened, previewed, edited or transferred;
//! filterable, clearable, can be switched off.

use super::{location_prefix, now_ms, parse_uri};
use crate::db::Db;
use crate::error::{Error, ErrorKind, Result};
use crate::uri::Uri;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;

/// Rows kept; older ones are dropped on `record`.
pub const MAX_RECENTS: usize = 500;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum RecentKind {
    Opened,
    Previewed,
    Edited,
    Transferred,
}

impl RecentKind {
    pub fn as_str(self) -> &'static str {
        match self {
            RecentKind::Opened => "opened",
            RecentKind::Previewed => "previewed",
            RecentKind::Edited => "edited",
            RecentKind::Transferred => "transferred",
        }
    }

    pub fn parse(s: &str) -> Option<RecentKind> {
        Some(match s {
            "opened" => RecentKind::Opened,
            "previewed" => RecentKind::Previewed,
            "edited" => RecentKind::Edited,
            "transferred" => RecentKind::Transferred,
            _ => return None,
        })
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Recent {
    pub id: i64,
    pub uri: Uri,
    pub name: String,
    pub kind: RecentKind,
    pub location: String,
    pub at_ms: i64,
}

/// Listing filter: empty `kinds` means all kinds; `text` is a
/// case-insensitive substring of the name.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct RecentsFilter {
    pub kinds: Vec<RecentKind>,
    pub text: Option<String>,
    pub limit: Option<usize>,
}

#[derive(Clone)]
pub struct Recents {
    db: Db,
    enabled: Arc<AtomicBool>,
    cap: usize,
}

impl Recents {
    pub fn new(db: Db) -> Recents {
        Recents::with_cap(db, MAX_RECENTS)
    }

    pub fn with_cap(db: Db, cap: usize) -> Recents {
        Recents {
            db,
            enabled: Arc::new(AtomicBool::new(true)),
            cap: cap.max(1),
        }
    }

    /// The setting "Recents" (§18). Turning it off keeps existing rows; use
    /// [`Recents::clear`] to forget them.
    pub fn set_enabled(&self, on: bool) {
        self.enabled.store(on, Ordering::Relaxed);
    }

    pub fn is_enabled(&self) -> bool {
        self.enabled.load(Ordering::Relaxed)
    }

    /// Records an event, now. The same (uri, kind) only moves to the top. A
    /// no-op while recents are off.
    pub fn record(&self, uri: &Uri, name: &str, kind: RecentKind) -> Result<()> {
        self.record_at(uri, name, kind, now_ms())
    }

    pub fn record_at(&self, uri: &Uri, name: &str, kind: RecentKind, at_ms: i64) -> Result<()> {
        if !self.is_enabled() {
            return Ok(());
        }
        let conn = self.db.lock();
        conn.execute(
            "INSERT INTO recents(uri, name, kind, location_id, at_ms) VALUES (?1, ?2, ?3, ?4, ?5)
             ON CONFLICT(uri, kind) DO UPDATE SET at_ms = excluded.at_ms, name = excluded.name",
            (uri.to_string(), name, kind.as_str(), &uri.location, at_ms),
        )?;
        conn.execute(
            "DELETE FROM recents WHERE id NOT IN
             (SELECT id FROM recents ORDER BY at_ms DESC, id DESC LIMIT ?1)",
            [i64::try_from(self.cap).unwrap_or(i64::MAX)],
        )?;
        Ok(())
    }

    /// Newest first.
    pub fn list(&self, filter: &RecentsFilter) -> Result<Vec<Recent>> {
        let all = self.load_all()?;
        let needle = filter.text.as_ref().map(|t| t.to_lowercase());
        let mut out: Vec<Recent> = all
            .into_iter()
            .filter(|r| filter.kinds.is_empty() || filter.kinds.contains(&r.kind))
            .filter(|r| {
                needle
                    .as_ref()
                    .map_or(true, |n| r.name.to_lowercase().contains(n))
            })
            .collect();
        if let Some(limit) = filter.limit {
            out.truncate(limit);
        }
        Ok(out)
    }

    fn load_all(&self) -> Result<Vec<Recent>> {
        let conn = self.db.lock();
        let mut stmt = conn.prepare(
            "SELECT id, uri, name, kind, location_id, at_ms FROM recents
             ORDER BY at_ms DESC, id DESC",
        )?;
        let raw = stmt
            .query_map([], |r| {
                Ok((
                    r.get::<_, i64>(0)?,
                    r.get::<_, String>(1)?,
                    r.get::<_, String>(2)?,
                    r.get::<_, String>(3)?,
                    r.get::<_, String>(4)?,
                    r.get::<_, i64>(5)?,
                ))
            })?
            .collect::<std::result::Result<Vec<_>, _>>()?;
        raw.into_iter()
            .map(|(id, uri, name, kind, location, at_ms)| {
                Ok(Recent {
                    id,
                    uri: parse_uri(&uri)?,
                    name,
                    kind: RecentKind::parse(&kind)
                        .ok_or_else(|| Error::new(ErrorKind::Internal, "unknown recent kind"))?,
                    location,
                    at_ms,
                })
            })
            .collect()
    }

    pub fn remove(&self, id: i64) -> Result<bool> {
        Ok(self
            .db
            .lock()
            .execute("DELETE FROM recents WHERE id = ?1", [id])?
            > 0)
    }

    pub fn clear(&self) -> Result<usize> {
        Ok(self.db.lock().execute("DELETE FROM recents", [])?)
    }

    /// SEC-5: forget everything of a removed location.
    pub fn purge_location(&self, location: &str) -> Result<usize> {
        let n = self.db.lock().execute(
            "DELETE FROM recents WHERE location_id = ?1 OR substr(uri, 1, length(?2)) = ?2",
            (location, location_prefix(location)),
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

    fn store() -> Recents {
        Recents::new(Db::open_in_memory().unwrap())
    }

    #[test]
    fn newest_first_and_dedupe_by_uri_and_kind() {
        let r = store();
        let a = u("lautta://x/a.txt");
        r.record_at(&a, "a.txt", RecentKind::Opened, 100).unwrap();
        r.record_at(&u("lautta://x/b.txt"), "b.txt", RecentKind::Opened, 200)
            .unwrap();
        r.record_at(&a, "a.txt", RecentKind::Opened, 300).unwrap();
        r.record_at(&a, "a.txt", RecentKind::Edited, 150).unwrap();
        let list = r.list(&RecentsFilter::default()).unwrap();
        let seen: Vec<(String, RecentKind, i64)> =
            list.iter().map(|x| (x.name.clone(), x.kind, x.at_ms)).collect();
        assert_eq!(
            seen,
            [
                ("a.txt".to_owned(), RecentKind::Opened, 300),
                ("b.txt".to_owned(), RecentKind::Opened, 200),
                ("a.txt".to_owned(), RecentKind::Edited, 150),
            ]
        );
        assert_eq!(list[0].location, "x");
    }

    #[test]
    fn filter_by_kind_text_and_limit() {
        let r = store();
        r.record_at(
            &u("lautta://x/Report.pdf"),
            "Report.pdf",
            RecentKind::Previewed,
            1,
        )
        .unwrap();
        r.record_at(&u("lautta://x/notes.txt"), "notes.txt", RecentKind::Edited, 2)
            .unwrap();
        r.record_at(
            &u("lautta://x/photo.jpg"),
            "photo.jpg",
            RecentKind::Transferred,
            3,
        )
        .unwrap();
        let by_kind = RecentsFilter {
            kinds: vec![RecentKind::Edited, RecentKind::Transferred],
            ..RecentsFilter::default()
        };
        assert_eq!(r.list(&by_kind).unwrap().len(), 2);
        let by_text = RecentsFilter {
            text: Some("REPORT".into()),
            ..RecentsFilter::default()
        };
        let got = r.list(&by_text).unwrap();
        assert_eq!(got.len(), 1);
        assert_eq!(got[0].name, "Report.pdf");
        let limited = RecentsFilter {
            limit: Some(1),
            ..RecentsFilter::default()
        };
        assert_eq!(r.list(&limited).unwrap()[0].name, "photo.jpg");
    }

    #[test]
    fn disabled_record_is_a_no_op() {
        let r = store();
        r.set_enabled(false);
        assert!(!r.is_enabled());
        r.record(&u("lautta://x/a"), "a", RecentKind::Opened).unwrap();
        assert!(r.list(&RecentsFilter::default()).unwrap().is_empty());
        r.set_enabled(true);
        r.record(&u("lautta://x/a"), "a", RecentKind::Opened).unwrap();
        assert_eq!(r.list(&RecentsFilter::default()).unwrap().len(), 1);
    }

    #[test]
    fn cap_drops_the_oldest() {
        let r = Recents::with_cap(Db::open_in_memory().unwrap(), 3);
        for i in 0..5 {
            let name = format!("f{i}");
            r.record_at(&u(&format!("lautta://x/{name}")), &name, RecentKind::Opened, i)
                .unwrap();
        }
        let names: Vec<String> = r
            .list(&RecentsFilter::default())
            .unwrap()
            .into_iter()
            .map(|x| x.name)
            .collect();
        assert_eq!(names, ["f4", "f3", "f2"]);
    }

    #[test]
    fn remove_clear_and_purge() {
        let r = store();
        r.record_at(&u("lautta://x/a"), "a", RecentKind::Opened, 1)
            .unwrap();
        r.record_at(&u("lautta://xy/a"), "b", RecentKind::Opened, 2)
            .unwrap();
        r.record_at(&u("lautta://x/c"), "c", RecentKind::Opened, 3)
            .unwrap();
        assert_eq!(r.purge_location("x").unwrap(), 2);
        let left = r.list(&RecentsFilter::default()).unwrap();
        assert_eq!(left.len(), 1);
        assert!(r.remove(left[0].id).unwrap());
        assert!(!r.remove(left[0].id).unwrap());
        r.record_at(&u("lautta://x/a"), "a", RecentKind::Opened, 1)
            .unwrap();
        assert_eq!(r.clear().unwrap(), 1);
    }

    #[test]
    fn kind_names_round_trip() {
        for k in [
            RecentKind::Opened,
            RecentKind::Previewed,
            RecentKind::Edited,
            RecentKind::Transferred,
        ] {
            assert_eq!(RecentKind::parse(k.as_str()), Some(k));
        }
        assert_eq!(RecentKind::parse("nope"), None);
    }
}
