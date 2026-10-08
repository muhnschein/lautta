// SPDX-License-Identifier: LGPL-2.1-or-later
//! Settings (SPEC §18): the global preferences with their defaults and
//! limits, and the per-location preferences kept in the app database.
//!
//! The Qt layer keeps the simple global values in dconf
//! (`Nemo.Configuration`); [`Settings::to_map`] and [`Settings::from_map`]
//! are the generic bridge: flat string keys, JSON values, bad or unknown
//! entries ignored.

use crate::db::Db;
use crate::error::Result;
use crate::uri::Uri;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::collections::BTreeMap;

pub const REMORSE_RANGE: (u32, u32) = (3, 10);
pub const LOCAL_CONCURRENCY_RANGE: (u32, u32) = (1, 4);
pub const REMOTE_LANES_RANGE: (u32, u32) = (1, 6);
const RETENTION_DAYS_MAX: u32 = 365;
const MB: u64 = 1_000_000;
const GB: u64 = 1_000_000_000;

pub const SORT_KEYS: [&str; 4] = ["name", "size", "modified", "type"];
pub const VIEW_MODES: [&str; 2] = ["list", "grid"];

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "lowercase")]
pub enum DateFormat {
    #[default]
    Relative,
    Iso,
    Locale,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct Settings {
    // Default view (BRW-4 global defaults).
    pub sort_key: String,
    pub sort_descending: bool,
    pub folders_first: bool,
    pub show_hidden: bool,
    pub view_mode: String,

    // Thumbnails (PRV).
    pub thumbnails_local: bool,
    pub thumbnails_remote: bool,
    pub thumbnail_max_remote_mb: u64,
    /// Largest remote file thumbnailed on a metered connection.
    pub thumbnail_max_metered_mb: u64,

    // Caches.
    pub thumbnail_cache_mb: u64,
    pub listing_cache_days: u32,

    // Recently deleted (§11) and remorse timer.
    pub recently_deleted: bool,
    pub recently_deleted_retention_days: u32,
    pub remorse_seconds: u32,

    // Transfers (§15).
    pub local_concurrency: u32,
    pub remote_lanes: u32,
    pub preserve_mtimes: bool,
    pub preserve_permissions: bool,
    pub verify_checksums: bool,
    pub auto_resume: bool,
    pub history_retention_days: u32,
    pub large_op_items: u64,
    pub large_op_bytes: u64,

    pub date_format: DateFormat,
    pub recents_enabled: bool,
    /// SRC-3: depth limit of remote searches.
    pub search_remote_depth: u32,
}

impl Default for Settings {
    fn default() -> Self {
        Settings {
            sort_key: "name".to_owned(),
            sort_descending: false,
            folders_first: true,
            show_hidden: false,
            view_mode: "list".to_owned(),
            thumbnails_local: true,
            thumbnails_remote: true,
            thumbnail_max_remote_mb: 20,
            thumbnail_max_metered_mb: 5,
            thumbnail_cache_mb: 200,
            listing_cache_days: 7,
            recently_deleted: true,
            recently_deleted_retention_days: 30,
            remorse_seconds: 5,
            local_concurrency: 2,
            remote_lanes: 2,
            preserve_mtimes: true,
            preserve_permissions: false,
            verify_checksums: false,
            auto_resume: true,
            history_retention_days: 30,
            large_op_items: 1_000,
            large_op_bytes: GB,
            date_format: DateFormat::Relative,
            recents_enabled: true,
            search_remote_depth: 8,
        }
    }
}

fn clamp<T: Ord>(v: T, range: (T, T)) -> T {
    v.clamp(range.0, range.1)
}

impl Settings {
    /// Forces every value into its allowed range (§18 limits).
    pub fn sanitise(&mut self) {
        let defaults = Settings::default();
        if !SORT_KEYS.contains(&self.sort_key.as_str()) {
            self.sort_key = defaults.sort_key;
        }
        if !VIEW_MODES.contains(&self.view_mode.as_str()) {
            self.view_mode = defaults.view_mode;
        }
        self.thumbnail_max_remote_mb = self.thumbnail_max_remote_mb.clamp(1, 1_000);
        self.thumbnail_max_metered_mb = self.thumbnail_max_metered_mb.clamp(0, 1_000);
        self.thumbnail_cache_mb = self.thumbnail_cache_mb.clamp(10, 10_000);
        self.listing_cache_days = self.listing_cache_days.clamp(1, RETENTION_DAYS_MAX);
        self.recently_deleted_retention_days =
            self.recently_deleted_retention_days.clamp(1, RETENTION_DAYS_MAX);
        self.history_retention_days = self.history_retention_days.clamp(1, RETENTION_DAYS_MAX);
        self.remorse_seconds = clamp(self.remorse_seconds, REMORSE_RANGE);
        self.local_concurrency = clamp(self.local_concurrency, LOCAL_CONCURRENCY_RANGE);
        self.remote_lanes = clamp(self.remote_lanes, REMOTE_LANES_RANGE);
        self.large_op_items = self.large_op_items.max(1);
        self.large_op_bytes = self.large_op_bytes.max(MB);
        self.search_remote_depth = self.search_remote_depth.clamp(1, 64);
    }

    pub fn sanitised(mut self) -> Settings {
        self.sanitise();
        self
    }

    pub fn to_json(&self) -> String {
        // Plain data with string keys: serialisation cannot fail.
        serde_json::to_string(self).unwrap_or_else(|_| "{}".to_owned())
    }

    /// Parses JSON; a document that is not an object yields defaults, bad
    /// keys are ignored one by one.
    pub fn from_json(json: &str) -> Settings {
        match serde_json::from_str::<Value>(json) {
            Ok(Value::Object(obj)) => Settings::from_map(&obj.into_iter().collect::<BTreeMap<_, _>>()),
            _ => Settings::default(),
        }
    }

    /// Flat key to value map for the dconf bridge.
    pub fn to_map(&self) -> BTreeMap<String, Value> {
        match serde_json::to_value(self) {
            Ok(Value::Object(obj)) => obj.into_iter().collect(),
            _ => BTreeMap::new(),
        }
    }

    /// Starts from the defaults and applies every entry that has a known key
    /// and the right type; then sanitises. Unknown keys and mistyped values
    /// are ignored so a stale or hand-edited store never breaks the app.
    pub fn from_map(map: &BTreeMap<String, Value>) -> Settings {
        let mut current = Settings::default();
        for (key, value) in map {
            let mut candidate = current.to_map();
            if !candidate.contains_key(key) {
                continue;
            }
            candidate.insert(key.clone(), value.clone());
            let doc = Value::Object(candidate.into_iter().collect());
            if let Ok(next) = serde_json::from_value::<Settings>(doc) {
                current = next;
            }
        }
        current.sanitised()
    }

    /// Sets one key from the UI; `false` when the key is unknown or the
    /// value has the wrong type (settings unchanged).
    pub fn set(&mut self, key: &str, value: Value) -> bool {
        let mut map = self.to_map();
        if !map.contains_key(key) {
            return false;
        }
        map.insert(key.to_owned(), value);
        let doc = Value::Object(map.into_iter().collect());
        match serde_json::from_value::<Settings>(doc) {
            Ok(next) => {
                *self = next.sanitised();
                true
            }
            Err(_) => false,
        }
    }
}

/// Per-location preferences (§18, `location_prefs`).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LocationPrefs {
    pub location_id: String,
    pub display_name: Option<String>,
    pub start_folder: Option<Uri>,
    pub no_listing_cache: bool,
    pub no_thumb_cache: bool,
    /// Bulk lanes for this remote location (1..=6); `None` uses the global.
    pub bulk_lanes: Option<u32>,
}

impl LocationPrefs {
    pub fn new(location_id: &str) -> LocationPrefs {
        LocationPrefs {
            location_id: location_id.to_owned(),
            display_name: None,
            start_folder: None,
            no_listing_cache: false,
            no_thumb_cache: false,
            bulk_lanes: None,
        }
    }

    /// Blank names count as no override; lane sizes are clamped.
    fn normalised(&self) -> LocationPrefs {
        let mut p = self.clone();
        p.display_name = p
            .display_name
            .map(|n| n.trim().to_owned())
            .filter(|n| !n.is_empty());
        p.bulk_lanes = p.bulk_lanes.map(|n| clamp(n, REMOTE_LANES_RANGE));
        p
    }

    fn is_default(&self) -> bool {
        *self == LocationPrefs::new(&self.location_id)
    }
}

#[derive(Clone)]
pub struct LocationPrefsStore {
    db: Db,
}

type RawPrefs = (String, Option<String>, Option<String>, i64, i64, Option<i64>);

fn build(raw: RawPrefs) -> LocationPrefs {
    LocationPrefs {
        location_id: raw.0,
        display_name: raw.1,
        // A malformed stored URI is dropped rather than failing the page.
        start_folder: raw.2.and_then(|s| Uri::parse(&s).ok()),
        no_listing_cache: raw.3 != 0,
        no_thumb_cache: raw.4 != 0,
        bulk_lanes: raw.5.and_then(|n| u32::try_from(n).ok()),
    }
}

fn raw_row(r: &rusqlite::Row<'_>) -> rusqlite::Result<RawPrefs> {
    Ok((r.get(0)?, r.get(1)?, r.get(2)?, r.get(3)?, r.get(4)?, r.get(5)?))
}

const PREF_COLUMNS: &str =
    "location_id, display_name, start_folder, no_listing_cache, no_thumb_cache, bulk_lanes";

impl LocationPrefsStore {
    pub fn new(db: Db) -> LocationPrefsStore {
        LocationPrefsStore { db }
    }

    /// The stored preferences, or the defaults when none are stored.
    pub fn get(&self, location_id: &str) -> Result<LocationPrefs> {
        let conn = self.db.lock();
        let mut stmt = conn.prepare(&format!(
            "SELECT {PREF_COLUMNS} FROM location_prefs WHERE location_id = ?1"
        ))?;
        let mut rows = stmt.query_map([location_id], raw_row)?;
        match rows.next() {
            Some(raw) => Ok(build(raw?)),
            None => Ok(LocationPrefs::new(location_id)),
        }
    }

    /// Stores `prefs`; all-default preferences remove the row.
    pub fn set(&self, prefs: &LocationPrefs) -> Result<()> {
        let p = prefs.normalised();
        let conn = self.db.lock();
        if p.is_default() {
            conn.execute(
                "DELETE FROM location_prefs WHERE location_id = ?1",
                [&p.location_id],
            )?;
            return Ok(());
        }
        conn.execute(
            "INSERT INTO location_prefs(location_id, display_name, start_folder,
                                        no_listing_cache, no_thumb_cache, bulk_lanes)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6)
             ON CONFLICT(location_id) DO UPDATE SET display_name = excluded.display_name,
                 start_folder = excluded.start_folder,
                 no_listing_cache = excluded.no_listing_cache,
                 no_thumb_cache = excluded.no_thumb_cache,
                 bulk_lanes = excluded.bulk_lanes",
            rusqlite::params![
                p.location_id,
                p.display_name,
                p.start_folder.as_ref().map(ToString::to_string),
                i64::from(p.no_listing_cache),
                i64::from(p.no_thumb_cache),
                p.bulk_lanes.map(i64::from),
            ],
        )?;
        Ok(())
    }

    pub fn remove(&self, location_id: &str) -> Result<bool> {
        let n = self
            .db
            .lock()
            .execute("DELETE FROM location_prefs WHERE location_id = ?1", [location_id])?;
        Ok(n > 0)
    }

    /// Every location with non-default preferences.
    pub fn list(&self) -> Result<Vec<LocationPrefs>> {
        let conn = self.db.lock();
        let mut stmt = conn.prepare(&format!(
            "SELECT {PREF_COLUMNS} FROM location_prefs ORDER BY location_id"
        ))?;
        let raw = stmt
            .query_map([], raw_row)?
            .collect::<std::result::Result<Vec<_>, _>>()?;
        Ok(raw.into_iter().map(build).collect())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn defaults_match_the_spec() {
        let s = Settings::default();
        assert_eq!(
            (s.sort_key.as_str(), s.sort_descending, s.folders_first),
            ("name", false, true)
        );
        assert!(!s.show_hidden && s.thumbnails_local && s.thumbnails_remote);
        assert_eq!((s.thumbnail_max_remote_mb, s.thumbnail_max_metered_mb), (20, 5));
        assert_eq!((s.thumbnail_cache_mb, s.listing_cache_days), (200, 7));
        assert!(s.recently_deleted);
        assert_eq!((s.recently_deleted_retention_days, s.remorse_seconds), (30, 5));
        assert_eq!((s.local_concurrency, s.remote_lanes), (2, 2));
        assert!(s.preserve_mtimes && !s.preserve_permissions && !s.verify_checksums && s.auto_resume);
        assert_eq!(s.history_retention_days, 30);
        assert_eq!((s.large_op_items, s.large_op_bytes), (1_000, 1_000_000_000));
        assert_eq!(s.date_format, DateFormat::Relative);
        assert!(s.recents_enabled);
        assert_eq!(s.search_remote_depth, 8);
        assert_eq!(s.clone().sanitised(), s, "defaults are already valid");
    }

    #[test]
    fn clamping_hits_both_ends() {
        let mut s = Settings {
            remorse_seconds: 1,
            local_concurrency: 9,
            remote_lanes: 0,
            thumbnail_cache_mb: 1,
            listing_cache_days: 0,
            history_retention_days: 100_000,
            large_op_items: 0,
            large_op_bytes: 0,
            search_remote_depth: 0,
            sort_key: "bogus".into(),
            view_mode: "".into(),
            ..Settings::default()
        };
        s.sanitise();
        assert_eq!(
            (s.remorse_seconds, s.local_concurrency, s.remote_lanes),
            (3, 4, 1)
        );
        assert_eq!((s.thumbnail_cache_mb, s.listing_cache_days), (10, 1));
        assert_eq!(s.history_retention_days, 365);
        assert_eq!((s.large_op_items, s.large_op_bytes), (1, MB));
        assert_eq!(s.search_remote_depth, 1);
        assert_eq!((s.sort_key.as_str(), s.view_mode.as_str()), ("name", "list"));
        s.remorse_seconds = 99;
        s.remote_lanes = 99;
        s.sanitise();
        assert_eq!((s.remorse_seconds, s.remote_lanes), (10, 6));
    }

    #[test]
    fn json_and_map_round_trip() {
        let s = Settings {
            sort_key: "size".into(),
            sort_descending: true,
            show_hidden: true,
            date_format: DateFormat::Iso,
            remote_lanes: 5,
            ..Settings::default()
        }
        .sanitised();
        assert_eq!(Settings::from_json(&s.to_json()), s);
        let map = s.to_map();
        assert_eq!(map["date_format"], json!("iso"));
        assert_eq!(map["remote_lanes"], json!(5));
        assert_eq!(Settings::from_map(&map), s);
    }

    #[test]
    fn bad_input_is_ignored_key_by_key() {
        let mut map = BTreeMap::new();
        map.insert("remorse_seconds".to_owned(), json!(8));
        map.insert("local_concurrency".to_owned(), json!("many"));
        map.insert("date_format".to_owned(), json!("martian"));
        map.insert("show_hidden".to_owned(), json!(true));
        map.insert("not_a_setting".to_owned(), json!(1));
        map.insert("remote_lanes".to_owned(), json!(100));
        let s = Settings::from_map(&map);
        assert_eq!(s.remorse_seconds, 8);
        assert_eq!(s.local_concurrency, 2);
        assert_eq!(s.date_format, DateFormat::Relative);
        assert!(s.show_hidden);
        assert_eq!(s.remote_lanes, 6, "out-of-range values are clamped");
        assert_eq!(Settings::from_json("not json"), Settings::default());
        assert_eq!(Settings::from_json("[1]"), Settings::default());
        assert_eq!(Settings::from_json("{}"), Settings::default());
    }

    #[test]
    fn set_validates_type_and_key() {
        let mut s = Settings::default();
        assert!(s.set("remorse_seconds", json!(7)));
        assert_eq!(s.remorse_seconds, 7);
        assert!(s.set("remorse_seconds", json!(50)));
        assert_eq!(s.remorse_seconds, 10);
        assert!(!s.set("remorse_seconds", json!("x")));
        assert_eq!(s.remorse_seconds, 10);
        assert!(!s.set("nope", json!(1)));
    }

    fn store() -> LocationPrefsStore {
        LocationPrefsStore::new(Db::open_in_memory().unwrap())
    }

    #[test]
    fn location_prefs_crud() {
        let st = store();
        assert_eq!(st.get("nv-1").unwrap(), LocationPrefs::new("nv-1"));
        let mut p = LocationPrefs::new("nv-1");
        p.display_name = Some("  Home NAS ".into());
        p.start_folder = Some(Uri::parse("lautta://nv-1/Photos").unwrap());
        p.no_listing_cache = true;
        p.no_thumb_cache = true;
        p.bulk_lanes = Some(40);
        st.set(&p).unwrap();
        let got = st.get("nv-1").unwrap();
        assert_eq!(got.display_name.as_deref(), Some("Home NAS"));
        assert_eq!(
            got.start_folder.as_ref().unwrap().to_string(),
            "lautta://nv-1/Photos"
        );
        assert!(got.no_listing_cache && got.no_thumb_cache);
        assert_eq!(got.bulk_lanes, Some(6));
        p.display_name = Some("Renamed".into());
        p.bulk_lanes = None;
        p.no_thumb_cache = false;
        st.set(&p).unwrap();
        let got = st.get("nv-1").unwrap();
        assert_eq!(got.display_name.as_deref(), Some("Renamed"));
        assert!(!got.no_thumb_cache && got.no_listing_cache);
        assert_eq!(got.bulk_lanes, None);
        assert_eq!(st.list().unwrap().len(), 1);
        assert!(st.remove("nv-1").unwrap());
        assert!(!st.remove("nv-1").unwrap());
    }

    #[test]
    fn all_default_prefs_are_not_stored() {
        let st = store();
        let mut p = LocationPrefs::new("sd");
        p.display_name = Some("Card".into());
        st.set(&p).unwrap();
        assert_eq!(st.list().unwrap().len(), 1);
        p.display_name = Some("   ".into());
        st.set(&p).unwrap();
        assert!(st.list().unwrap().is_empty());
    }

    #[test]
    fn malformed_stored_start_folder_is_dropped() {
        let st = store();
        st.db
            .lock()
            .execute(
                "INSERT INTO location_prefs(location_id, start_folder) VALUES ('x', 'garbage')",
                [],
            )
            .unwrap();
        assert_eq!(st.get("x").unwrap().start_folder, None);
    }
}
