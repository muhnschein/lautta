// SPDX-License-Identifier: LGPL-2.1-or-later
//! Directory listing cache (BRW-5, BRW-7, SEC-5): a small in-memory LRU in
//! front of the `dircache` table. Listings older than seven days are never
//! returned. A location can opt out (`location_prefs.no_listing_cache`),
//! which also purges what is stored for it.

use crate::db::Db;
use crate::entry::{ms_to_system_time, system_time_to_ms, Entry, EntryFlags, Kind};
use crate::error::{Error, ErrorKind, Result};
use crate::uri::{LocationId, Uri};
use rusqlite::{params, OptionalExtension};
use serde::{Deserialize, Serialize};
use std::collections::{HashMap, HashSet};
use std::sync::{Arc, Mutex, MutexGuard};

/// BRW-5: cached listings older than this are not shown.
pub const MAX_AGE_MS: i64 = 7 * 24 * 60 * 60 * 1000;
/// A cached listing younger than this is not marked stale.
pub const DEFAULT_FRESH_MS: i64 = 30_000;

/// Milliseconds since the Unix epoch; injectable for tests.
pub type Clock = Arc<dyn Fn() -> i64 + Send + Sync>;

#[derive(Debug, Clone, PartialEq)]
pub struct CachedListing {
    pub entries: Vec<Entry>,
    pub fetched_ms: i64,
    /// Older than the fresh window: the UI marks it and revalidates.
    pub stale: bool,
}

struct MemEntry {
    fetched_ms: i64,
    entries: Vec<Entry>,
    used: u64,
}

#[derive(Default)]
struct Inner {
    map: HashMap<String, MemEntry>,
    tick: u64,
    disabled: HashSet<LocationId>,
}

impl Inner {
    fn touch(&mut self, key: &str) -> Option<(i64, Vec<Entry>)> {
        self.tick += 1;
        let tick = self.tick;
        let m = self.map.get_mut(key)?;
        m.used = tick;
        Some((m.fetched_ms, m.entries.clone()))
    }

    fn insert(&mut self, key: String, fetched_ms: i64, entries: Vec<Entry>, capacity: usize) {
        self.tick += 1;
        let used = self.tick;
        self.map.insert(
            key,
            MemEntry {
                fetched_ms,
                entries,
                used,
            },
        );
        while self.map.len() > capacity {
            let Some(oldest) = self
                .map
                .iter()
                .min_by_key(|(_, m)| m.used)
                .map(|(k, _)| k.clone())
            else {
                break;
            };
            self.map.remove(&oldest);
        }
    }
}

pub struct DirCache {
    db: Db,
    inner: Mutex<Inner>,
    capacity: usize,
    fresh_ms: i64,
    clock: Clock,
}

impl DirCache {
    /// `capacity` is the number of listings kept in memory.
    pub fn new(db: Db, capacity: usize) -> Result<DirCache> {
        DirCache::with_clock(db, capacity, Arc::new(wall_clock_ms))
    }

    pub fn with_clock(db: Db, capacity: usize, clock: Clock) -> Result<DirCache> {
        let disabled = load_disabled(&db)?;
        Ok(DirCache {
            db,
            inner: Mutex::new(Inner {
                disabled,
                ..Inner::default()
            }),
            capacity,
            fresh_ms: DEFAULT_FRESH_MS,
            clock,
        })
    }

    pub fn set_fresh_window_ms(&mut self, ms: i64) {
        self.fresh_ms = ms;
    }

    fn lock(&self) -> MutexGuard<'_, Inner> {
        match self.inner.lock() {
            Ok(g) => g,
            Err(p) => p.into_inner(),
        }
    }

    /// The cached listing for `uri`, if there is one younger than seven days
    /// and the location has not opted out.
    pub fn get(&self, uri: &Uri) -> Result<Option<CachedListing>> {
        let key = uri.to_string();
        let now = (self.clock)();
        let mut inner = self.lock();
        if inner.disabled.contains(&uri.location) {
            return Ok(None);
        }
        let hit = match inner.touch(&key) {
            Some(hit) => Some(hit),
            None => self.load_into_memory(&mut inner, &key)?,
        };
        let Some((fetched_ms, entries)) = hit else {
            return Ok(None);
        };
        if now - fetched_ms > MAX_AGE_MS {
            self.remove_key(&mut inner, &key)?;
            return Ok(None);
        }
        Ok(Some(CachedListing {
            entries,
            fetched_ms,
            stale: now - fetched_ms >= self.fresh_ms,
        }))
    }

    fn load_into_memory(&self, inner: &mut Inner, key: &str) -> Result<Option<(i64, Vec<Entry>)>> {
        let row: Option<(i64, Vec<u8>)> = self
            .db
            .lock()
            .query_row(
                "SELECT fetched_ms, entries FROM dircache WHERE uri = ?1",
                params![key],
                |r| Ok((r.get(0)?, r.get(1)?)),
            )
            .optional()?;
        let Some((fetched_ms, blob)) = row else {
            return Ok(None);
        };
        match decode(&blob) {
            Ok(entries) => {
                inner.insert(key.to_owned(), fetched_ms, entries.clone(), self.capacity);
                Ok(Some((fetched_ms, entries)))
            }
            Err(_) => {
                // A row from a damaged write or an older format is only a cache.
                self.remove_key(inner, key)?;
                Ok(None)
            }
        }
    }

    fn remove_key(&self, inner: &mut Inner, key: &str) -> Result<()> {
        inner.map.remove(key);
        self.db
            .lock()
            .execute("DELETE FROM dircache WHERE uri = ?1", params![key])?;
        Ok(())
    }

    /// Stores a fresh listing. Does nothing for a location that opted out.
    pub fn put(&self, uri: &Uri, entries: &[Entry]) -> Result<()> {
        let mut inner = self.lock();
        if inner.disabled.contains(&uri.location) {
            return Ok(());
        }
        let now = (self.clock)();
        let key = uri.to_string();
        let blob = encode(entries)?;
        self.db.lock().execute(
            "INSERT INTO dircache(uri, location_id, fetched_ms, entries) VALUES (?1, ?2, ?3, ?4) \
             ON CONFLICT(uri) DO UPDATE SET location_id = excluded.location_id, \
             fetched_ms = excluded.fetched_ms, entries = excluded.entries",
            params![key, uri.location, now, blob],
        )?;
        inner.insert(key, now, entries.to_vec(), self.capacity);
        Ok(())
    }

    pub fn invalidate(&self, uri: &Uri) -> Result<()> {
        let mut inner = self.lock();
        self.remove_key(&mut inner, &uri.to_string())
    }

    pub fn invalidate_location(&self, location: &str) -> Result<()> {
        let mut inner = self.lock();
        self.purge_location(&mut inner, location)
    }

    fn purge_location(&self, inner: &mut Inner, location: &str) -> Result<()> {
        let prefix = format!("lautta://{location}/");
        inner.map.retain(|k, _| !k.starts_with(&prefix));
        self.db
            .lock()
            .execute("DELETE FROM dircache WHERE location_id = ?1", params![location])?;
        Ok(())
    }

    pub fn clear(&self) -> Result<()> {
        let mut inner = self.lock();
        inner.map.clear();
        self.db.lock().execute("DELETE FROM dircache", [])?;
        Ok(())
    }

    /// Drops stored listings past the seven-day limit (housekeeping).
    pub fn purge_expired(&self) -> Result<()> {
        let mut inner = self.lock();
        let cutoff = (self.clock)() - MAX_AGE_MS;
        inner.map.retain(|_, m| m.fetched_ms >= cutoff);
        self.db
            .lock()
            .execute("DELETE FROM dircache WHERE fetched_ms < ?1", params![cutoff])?;
        Ok(())
    }

    /// Per-location opt-out (SEC-5). Disabling also deletes what is cached.
    pub fn set_disabled(&self, location: &str, disabled: bool) -> Result<()> {
        let mut inner = self.lock();
        self.db.lock().execute(
            "INSERT INTO location_prefs(location_id, no_listing_cache) VALUES (?1, ?2) \
             ON CONFLICT(location_id) DO UPDATE SET no_listing_cache = excluded.no_listing_cache",
            params![location, disabled],
        )?;
        if disabled {
            inner.disabled.insert(location.to_owned());
            self.purge_location(&mut inner, location)
        } else {
            inner.disabled.remove(location);
            Ok(())
        }
    }

    pub fn is_disabled(&self, location: &str) -> bool {
        self.lock().disabled.contains(location)
    }

    /// Number of listings currently held in memory.
    pub fn memory_len(&self) -> usize {
        self.lock().map.len()
    }
}

fn wall_clock_ms() -> i64 {
    system_time_to_ms(std::time::SystemTime::now())
}

fn load_disabled(db: &Db) -> Result<HashSet<LocationId>> {
    let conn = db.lock();
    let mut stmt = conn.prepare("SELECT location_id FROM location_prefs WHERE no_listing_cache <> 0")?;
    let rows = stmt.query_map([], |r| r.get::<_, String>(0))?;
    let mut out = HashSet::new();
    for row in rows {
        out.insert(row?);
    }
    Ok(out)
}

/// Compact on-disk form of an [`Entry`]. Names and etags are bytes, so they
/// are stored as text when valid UTF-8 and as hex otherwise.
#[derive(Serialize, Deserialize)]
struct EntryDto {
    #[serde(default, skip_serializing_if = "String::is_empty")]
    n: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    nb: Option<String>,
    k: u8,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    tk: Option<u8>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    s: Option<u64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    m: Option<i64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    c: Option<i64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    mo: Option<u32>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    o: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    g: Option<String>,
    #[serde(default, skip_serializing_if = "is_zero")]
    f: u16,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    e: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    ct: Option<String>,
}

fn is_zero(v: &u16) -> bool {
    *v == 0
}

impl EntryDto {
    fn from_entry(e: &Entry) -> EntryDto {
        let (n, nb) = match std::str::from_utf8(&e.name) {
            Ok(s) => (s.to_owned(), None),
            Err(_) => (String::new(), Some(hex::encode(&e.name))),
        };
        EntryDto {
            n,
            nb,
            k: e.kind.to_wire(),
            tk: (e.target_kind != e.kind).then(|| e.target_kind.to_wire()),
            s: e.size,
            m: e.modified.map(system_time_to_ms),
            c: e.created.map(system_time_to_ms),
            mo: e.mode,
            o: e.owner.clone(),
            g: e.group.clone(),
            f: e.flags.bits(),
            e: e.etag.as_ref().map(hex::encode),
            ct: e.content_type.clone(),
        }
    }

    fn into_entry(self) -> Result<Entry> {
        let name = match self.nb {
            Some(h) => hex::decode(h).map_err(bad_cache)?,
            None => self.n.into_bytes(),
        };
        let kind = Kind::from_wire(self.k);
        Ok(Entry {
            name,
            kind,
            target_kind: self.tk.map_or(kind, Kind::from_wire),
            size: self.s,
            modified: self.m.map(ms_to_system_time),
            created: self.c.map(ms_to_system_time),
            mode: self.mo,
            owner: self.o,
            group: self.g,
            flags: EntryFlags::from_bits_truncate(self.f),
            etag: self.e.map(hex::decode).transpose().map_err(bad_cache)?,
            content_type: self.ct,
        })
    }
}

fn bad_cache(e: impl std::fmt::Display) -> Error {
    Error::new(ErrorKind::Internal, format!("bad cached listing: {e}"))
}

fn encode(entries: &[Entry]) -> Result<Vec<u8>> {
    let dtos: Vec<EntryDto> = entries.iter().map(EntryDto::from_entry).collect();
    serde_json::to_vec(&dtos).map_err(bad_cache)
}

fn decode(blob: &[u8]) -> Result<Vec<Entry>> {
    let dtos: Vec<EntryDto> = serde_json::from_slice(blob).map_err(bad_cache)?;
    dtos.into_iter().map(EntryDto::into_entry).collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::atomic::{AtomicI64, Ordering};

    const DAY: i64 = 24 * 60 * 60 * 1000;

    struct Fixture {
        cache: DirCache,
        now: Arc<AtomicI64>,
        db: Db,
    }

    fn fixture(capacity: usize) -> Fixture {
        let db = Db::open_in_memory().unwrap();
        let now = Arc::new(AtomicI64::new(1_700_000_000_000));
        let n = now.clone();
        let clock: Clock = Arc::new(move || n.load(Ordering::SeqCst));
        Fixture {
            cache: DirCache::with_clock(db.clone(), capacity, clock).unwrap(),
            now,
            db,
        }
    }

    fn uri(s: &str) -> Uri {
        Uri::parse(s).unwrap()
    }

    fn rich_entry() -> Entry {
        let mut e = Entry::new("caf\u{e9}.txt".as_bytes(), Kind::Symlink);
        e.target_kind = Kind::File;
        e.size = Some(1234);
        e.modified = Some(ms_to_system_time(1_650_000_000_123));
        e.created = Some(ms_to_system_time(1_640_000_000_000));
        e.mode = Some(0o640);
        e.owner = Some("nemo".into());
        e.group = Some("users".into());
        e.flags = EntryFlags::READONLY | EntryFlags::TARGET_UNKNOWN;
        e.etag = Some(vec![0, 255, 10]);
        e.content_type = Some("text/plain".into());
        e
    }

    fn sample() -> Vec<Entry> {
        let mut bad = Entry::new(b"bad\xff\xfename", Kind::File);
        bad.etag = Some(Vec::new());
        vec![
            rich_entry(),
            Entry::new(b"dir", Kind::Dir),
            bad,
            Entry::new(b".hidden", Kind::File),
        ]
    }

    #[test]
    fn dto_round_trip_keeps_every_field() {
        let entries = sample();
        let back = decode(&encode(&entries).unwrap()).unwrap();
        assert_eq!(back, entries);
        assert!(back[2].name_is_lossy());
        assert_eq!(back[0].target_kind, Kind::File);
        assert_eq!(back[1].target_kind, Kind::Dir);
    }

    #[test]
    fn encoding_is_compact_for_plain_entries() {
        let blob = encode(&[Entry::new(b"a", Kind::File)]).unwrap();
        assert_eq!(String::from_utf8(blob).unwrap(), r#"[{"n":"a","k":1}]"#);
    }

    #[test]
    fn put_then_get_from_memory_and_from_sqlite() {
        let f = fixture(8);
        let u = uri("lautta://user-documents/Photos");
        assert_eq!(f.cache.get(&u).unwrap(), None);
        f.cache.put(&u, &sample()).unwrap();
        let hit = f.cache.get(&u).unwrap().unwrap();
        assert_eq!(hit.entries, sample());
        assert_eq!(hit.fetched_ms, 1_700_000_000_000);
        assert!(!hit.stale);

        // A second cache over the same database starts with an empty memory.
        let clock: Clock = Arc::new(|| 1_700_000_000_000);
        let cold = DirCache::with_clock(f.db.clone(), 8, clock).unwrap();
        assert_eq!(cold.memory_len(), 0);
        assert_eq!(cold.get(&u).unwrap().unwrap().entries, sample());
        assert_eq!(cold.memory_len(), 1, "a database hit is promoted to memory");
    }

    #[test]
    fn stale_flag_follows_the_fresh_window() {
        let mut f = fixture(8);
        let u = uri("lautta://user-documents/a");
        f.cache.put(&u, &sample()).unwrap();
        f.now.fetch_add(DEFAULT_FRESH_MS - 1, Ordering::SeqCst);
        assert!(!f.cache.get(&u).unwrap().unwrap().stale);
        f.now.fetch_add(1, Ordering::SeqCst);
        assert!(f.cache.get(&u).unwrap().unwrap().stale);
        f.cache.set_fresh_window_ms(10 * DAY);
        assert!(!f.cache.get(&u).unwrap().unwrap().stale);
    }

    #[test]
    fn listings_older_than_seven_days_are_dropped() {
        let f = fixture(8);
        let u = uri("lautta://user-documents/a");
        f.cache.put(&u, &sample()).unwrap();
        f.now.fetch_add(7 * DAY, Ordering::SeqCst);
        assert!(
            f.cache.get(&u).unwrap().is_some(),
            "exactly seven days is still shown"
        );
        f.now.fetch_add(1, Ordering::SeqCst);
        assert_eq!(f.cache.get(&u).unwrap(), None);
        assert_eq!(f.cache.memory_len(), 0);
        let rows: i64 =
            f.db.lock()
                .query_row("SELECT count(*) FROM dircache", [], |r| r.get(0))
                .unwrap();
        assert_eq!(rows, 0, "the expired row is deleted too");
    }

    #[test]
    fn expired_rows_in_the_database_are_not_returned_either() {
        let f = fixture(8);
        let u = uri("lautta://user-documents/a");
        f.cache.put(&u, &sample()).unwrap();
        f.cache.lock().map.clear();
        f.now.fetch_add(8 * DAY, Ordering::SeqCst);
        assert_eq!(f.cache.get(&u).unwrap(), None);
    }

    #[test]
    fn purge_expired_keeps_recent_listings() {
        let f = fixture(8);
        let old = uri("lautta://user-documents/old");
        let new = uri("lautta://user-documents/new");
        f.cache.put(&old, &sample()).unwrap();
        f.now.fetch_add(6 * DAY, Ordering::SeqCst);
        f.cache.put(&new, &sample()).unwrap();
        f.now.fetch_add(2 * DAY, Ordering::SeqCst);
        f.cache.purge_expired().unwrap();
        assert_eq!(f.cache.memory_len(), 1);
        assert!(f.cache.get(&new).unwrap().is_some());
        assert_eq!(f.cache.get(&old).unwrap(), None);
    }

    #[test]
    fn memory_is_a_lru() {
        let f = fixture(2);
        let (a, b, c) = (uri("lautta://l/a"), uri("lautta://l/b"), uri("lautta://l/c"));
        f.cache.put(&a, &sample()).unwrap();
        f.cache.put(&b, &sample()).unwrap();
        // Touch a so b is the least recently used.
        assert!(f.cache.get(&a).unwrap().is_some());
        f.cache.put(&c, &sample()).unwrap();
        assert_eq!(f.cache.memory_len(), 2);
        let keys: HashSet<String> = f.cache.lock().map.keys().cloned().collect();
        assert!(keys.contains(&a.to_string()));
        assert!(keys.contains(&c.to_string()));
        assert!(!keys.contains(&b.to_string()));
        // Evicted from memory but still in SQLite.
        assert!(f.cache.get(&b).unwrap().is_some());
    }

    #[test]
    fn zero_capacity_uses_only_sqlite() {
        let f = fixture(0);
        let u = uri("lautta://l/a");
        f.cache.put(&u, &sample()).unwrap();
        assert_eq!(f.cache.memory_len(), 0);
        assert_eq!(f.cache.get(&u).unwrap().unwrap().entries, sample());
    }

    #[test]
    fn put_replaces_the_previous_listing() {
        let f = fixture(8);
        let u = uri("lautta://l/a");
        f.cache.put(&u, &sample()).unwrap();
        f.now.fetch_add(1000, Ordering::SeqCst);
        f.cache.put(&u, &[Entry::new(b"only", Kind::File)]).unwrap();
        let hit = f.cache.get(&u).unwrap().unwrap();
        assert_eq!(hit.entries.len(), 1);
        assert_eq!(hit.fetched_ms, 1_700_000_001_000);
        f.cache.lock().map.clear();
        assert_eq!(f.cache.get(&u).unwrap().unwrap().entries.len(), 1);
    }

    #[test]
    fn invalidate_one_location_and_all() {
        let f = fixture(8);
        let (a, b, c) = (uri("lautta://l1/a"), uri("lautta://l1/b"), uri("lautta://l2/a"));
        for u in [&a, &b, &c] {
            f.cache.put(u, &sample()).unwrap();
        }
        f.cache.invalidate(&a).unwrap();
        assert_eq!(f.cache.get(&a).unwrap(), None);
        assert!(f.cache.get(&b).unwrap().is_some());
        f.cache.invalidate_location("l1").unwrap();
        assert_eq!(f.cache.get(&b).unwrap(), None);
        assert!(f.cache.get(&c).unwrap().is_some(), "other locations stay");
        f.cache.clear().unwrap();
        assert_eq!(f.cache.get(&c).unwrap(), None);
        assert_eq!(f.cache.memory_len(), 0);
    }

    #[test]
    fn invalidate_location_matches_whole_location_ids_only() {
        let f = fixture(8);
        let (a, b) = (uri("lautta://nv-1/a"), uri("lautta://nv-10/a"));
        f.cache.put(&a, &sample()).unwrap();
        f.cache.put(&b, &sample()).unwrap();
        f.cache.invalidate_location("nv-1").unwrap();
        assert_eq!(f.cache.get(&a).unwrap(), None);
        assert!(f.cache.get(&b).unwrap().is_some());
    }

    #[test]
    fn opt_out_purges_blocks_and_persists() {
        let f = fixture(8);
        let u = uri("lautta://nv-1/a");
        let other = uri("lautta://user-documents/a");
        f.cache.put(&u, &sample()).unwrap();
        f.cache.put(&other, &sample()).unwrap();
        f.cache.set_disabled("nv-1", true).unwrap();
        assert!(f.cache.is_disabled("nv-1"));
        assert_eq!(f.cache.get(&u).unwrap(), None);
        f.cache.put(&u, &sample()).unwrap();
        assert_eq!(f.cache.get(&u).unwrap(), None, "puts are ignored while opted out");
        let rows: i64 =
            f.db.lock()
                .query_row(
                    "SELECT count(*) FROM dircache WHERE location_id = 'nv-1'",
                    [],
                    |r| r.get(0),
                )
                .unwrap();
        assert_eq!(rows, 0);
        assert!(f.cache.get(&other).unwrap().is_some());

        // The choice survives a restart.
        let again = DirCache::with_clock(f.db.clone(), 8, Arc::new(|| 1_700_000_000_000)).unwrap();
        assert!(again.is_disabled("nv-1"));
        assert!(!again.is_disabled("user-documents"));

        f.cache.set_disabled("nv-1", false).unwrap();
        assert!(!f.cache.is_disabled("nv-1"));
        f.cache.put(&u, &sample()).unwrap();
        assert!(f.cache.get(&u).unwrap().is_some());
    }

    #[test]
    fn corrupt_rows_are_treated_as_misses_and_removed() {
        let f = fixture(8);
        let u = uri("lautta://l/a");
        f.db.lock()
            .execute(
                "INSERT INTO dircache(uri, location_id, fetched_ms, entries) VALUES (?1, 'l', ?2, X'7B7B')",
                params![u.to_string(), 1_700_000_000_000_i64],
            )
            .unwrap();
        assert_eq!(f.cache.get(&u).unwrap(), None);
        let rows: i64 =
            f.db.lock()
                .query_row("SELECT count(*) FROM dircache", [], |r| r.get(0))
                .unwrap();
        assert_eq!(rows, 0);
        // Bad hex in an otherwise valid document is also rejected.
        assert!(decode(br#"[{"nb":"zz","k":1}]"#).is_err());
        assert!(decode(br#"[{"n":"a","k":1,"e":"q"}]"#).is_err());
    }

    #[test]
    fn real_clock_is_close_to_now() {
        let db = Db::open_in_memory().unwrap();
        let cache = DirCache::new(db, 4).unwrap();
        let u = uri("lautta://l/a");
        cache.put(&u, &sample()).unwrap();
        let hit = cache.get(&u).unwrap().unwrap();
        assert!(hit.fetched_ms > 1_600_000_000_000);
        assert!(!hit.stale);
    }
}
