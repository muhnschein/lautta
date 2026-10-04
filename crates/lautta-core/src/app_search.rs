// SPDX-License-Identifier: LGPL-2.1-or-later
//! User-level actions of the search area on [`Core`](crate::app::Core):
//! recursive search (SRC-2..4), folder compare and sync (SYN-1..3), cache and
//! app-data clearing (SEC-5, DAT-3) and per-location preferences (§18).

use crate::app::Core;
use crate::compare::{
    build_plans, compare_trees, preview, sync_plan, CompareOptions, CompareResult, LargeOpLimits, SyncAction,
    SyncMode, SyncPreview, MTIME_TOLERANCE_MS,
};
use crate::error::{Error, ErrorKind, Result};
use crate::org::syncpairs::SyncPairSpec;
use crate::search::{
    self, classify_by_extension, MatchMode, SearchHit, SearchOptions, SearchQuery, SearchSummary,
};
use crate::settings::LocationPrefs;
use crate::transfer::TransferId;
use crate::uri::Uri;
use crate::vpath::{display_name, VPath};
use chrono::{DateTime, Datelike, Duration, Local, TimeZone};
use std::collections::BTreeSet;
use std::path::Path;
use std::sync::atomic::AtomicBool;
use std::sync::Arc;
use std::time::SystemTime;
use tokio::sync::mpsc;

const MB: u64 = 1_000_000;
const KB: u64 = 1_000;

/// The categories `classify_by_extension` can return, in filter-chip order.
pub const SEARCH_TYPES: [&str; 7] = ["folder", "image", "video", "audio", "document", "archive", "text"];
/// Size filter presets (SRC-2) and what they mean.
pub const SIZE_PRESETS: [&str; 6] = ["any", "gt1m", "gt10m", "gt100m", "lt100k", "lt1m"];
/// Date filter presets (SRC-2).
pub const DATE_PRESETS: [&str; 5] = ["any", "today", "week", "month", "year"];

/// What the user typed and chose on the search page.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct SearchRequest {
    pub text: String,
    pub mode: MatchMode,
    pub types: BTreeSet<String>,
    pub size_preset: String,
    pub date_preset: String,
    pub include_hidden: bool,
}

/// `(min, max)` bytes of a size preset; unknown presets mean no limit.
pub fn size_bounds(preset: &str) -> (Option<u64>, Option<u64>) {
    match preset {
        "gt1m" => (Some(MB), None),
        "gt10m" => (Some(10 * MB), None),
        "gt100m" => (Some(100 * MB), None),
        "lt100k" => (None, Some(100 * KB)),
        "lt1m" => (None, Some(MB)),
        _ => (None, None),
    }
}

/// The earliest modification time of a date preset as seen from `now`:
/// today is since local midnight, a week and a month are rolling, the year
/// starts on 1 January.
pub fn date_lower_bound(preset: &str, now: DateTime<Local>) -> Option<SystemTime> {
    let midnight =
        |date: chrono::NaiveDate| Local.from_local_datetime(&date.and_hms_opt(0, 0, 0)?).earliest();
    let bound = match preset {
        "today" => midnight(now.date_naive())?,
        "week" => now - Duration::days(7),
        "month" => now - Duration::days(30),
        "year" => midnight(chrono::NaiveDate::from_ymd_opt(now.year(), 1, 1)?)?,
        _ => return None,
    };
    Some(SystemTime::from(bound))
}

impl SearchRequest {
    /// Whether anything restricts the search; an empty request would list the
    /// whole tree, which the UI only does on purpose.
    pub fn is_restricted(&self) -> bool {
        !self.text.trim().is_empty()
            || !self.types.is_empty()
            || size_bounds(&self.size_preset) != (None, None)
            || date_lower_bound(&self.date_preset, Local::now()).is_some()
    }

    pub fn to_query(&self, now: DateTime<Local>) -> SearchQuery {
        let (min_size, max_size) = size_bounds(&self.size_preset);
        SearchQuery {
            text: self.text.trim().to_owned(),
            mode: self.mode,
            types: self
                .types
                .iter()
                .filter(|t| SEARCH_TYPES.contains(&t.as_str()))
                .cloned()
                .collect(),
            min_size,
            max_size,
            modified_after: date_lower_bound(&self.date_preset, now),
            modified_before: None,
            include_hidden: self.include_hidden,
        }
    }
}

fn lower_chars(s: &str) -> Vec<char> {
    s.chars().map(|c| c.to_lowercase().next().unwrap_or(c)).collect()
}

/// The part of `text` that is shown highlighted in a hit's name: the whole
/// text for substring search, the longest literal run for a glob.
fn literal_needle(text: &str, mode: MatchMode) -> String {
    match mode {
        MatchMode::Substring => text.to_owned(),
        MatchMode::Glob => text
            .split(|c| matches!(c, '*' | '?' | '[' | ']'))
            .max_by_key(|piece| piece.chars().count())
            .unwrap_or("")
            .to_owned(),
    }
}

/// `(start, length)` in characters of the first case-insensitive occurrence
/// of the query's literal part in `name`.
pub fn highlight_span(name: &str, text: &str, mode: MatchMode) -> Option<(usize, usize)> {
    let needle = lower_chars(&literal_needle(text.trim(), mode));
    if needle.is_empty() {
        return None;
    }
    let hay = lower_chars(name);
    hay.windows(needle.len())
        .position(|w| w == needle.as_slice())
        .map(|start| (start, needle.len()))
}

/// The group header of a hit's folder: the search root's name and the
/// folders below it, "Documents › Uni › Thesis".
pub fn section_label(root: &Uri, root_name: &str, folder: &Uri) -> String {
    let mut parts = vec![root_name.to_owned()];
    if let Some(rel) = folder.path.strip_prefix(&root.path) {
        parts.extend(rel.components().map(display_name));
    }
    parts.join(" \u{203a} ")
}

/// Exclusion patterns typed as "*.tmp, .thumbnails/" (SYN-2): separated by
/// commas or new lines, blanks and repeats dropped.
pub fn parse_excludes(text: &str) -> Vec<String> {
    let mut out: Vec<String> = Vec::new();
    for piece in text.split(|c| c == ',' || c == '\n') {
        let piece = piece.trim();
        if !piece.is_empty() && !out.iter().any(|p| p == piece) {
            out.push(piece.to_owned());
        }
    }
    out
}

pub fn excludes_text(excludes: &[String]) -> String {
    excludes.join(", ")
}

/// Compare options of a saved pair.
pub fn options_of(spec: &SyncPairSpec) -> CompareOptions {
    CompareOptions {
        mtime_tolerance_ms: MTIME_TOLERANCE_MS,
        dst_tolerance: spec.dst_tolerance,
        checksums: spec.checksums,
        excludes: spec.excludes.clone(),
    }
}

/// The pair to store for a compare setup (SYN-3).
pub fn pair_spec(
    label: &str,
    left: &Uri,
    right: &Uri,
    mode: SyncMode,
    options: &CompareOptions,
) -> SyncPairSpec {
    SyncPairSpec {
        label: label.to_owned(),
        left: left.clone(),
        right: right.clone(),
        mode,
        excludes: options.excludes.clone(),
        checksums: options.checksums,
        dst_tolerance: options.dst_tolerance,
    }
}

/// What the app's caches occupy, in bytes (Settings → Storage).
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct CacheSizes {
    pub thumbnails: u64,
    pub listings: u64,
    pub archives: u64,
}

impl CacheSizes {
    pub fn total(&self) -> u64 {
        self.thumbnails + self.listings + self.archives
    }
}

/// Bytes of the regular files below `path` (symlinks are not followed).
pub fn dir_size(path: &Path) -> u64 {
    let Ok(read) = std::fs::read_dir(path) else {
        return 0;
    };
    read.filter_map(Result::ok)
        .map(|e| match e.file_type() {
            Ok(t) if t.is_dir() => dir_size(&e.path()),
            Ok(t) if t.is_file() => e.metadata().map(|m| m.len()).unwrap_or(0),
            _ => 0,
        })
        .sum()
}

/// Deletes everything inside `dir` and keeps `dir`. Returns the bytes freed.
fn remove_contents(dir: &Path) -> Result<u64> {
    let freed = dir_size(dir);
    let read = match std::fs::read_dir(dir) {
        Ok(r) => r,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(0),
        Err(e) => return Err(e.into()),
    };
    for entry in read {
        let entry = entry?;
        if entry.file_type()?.is_dir() {
            std::fs::remove_dir_all(entry.path())?;
        } else {
            std::fs::remove_file(entry.path())?;
        }
    }
    Ok(freed)
}

fn remove_tree(dir: &Path) -> Result<()> {
    match std::fs::remove_dir_all(dir) {
        Ok(()) => Ok(()),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(()),
        Err(e) => Err(e.into()),
    }
}

impl Core {
    /// SRC-3: unlimited on local locations, the setting on remote ones.
    pub fn search_depth(&self, root: &Uri) -> Option<u32> {
        if self.locations.to_local_path(root).is_some() {
            None
        } else {
            Some(self.settings().search_remote_depth)
        }
    }

    /// Searches below `root` and streams one batch of hits per folder (SRC-2).
    /// A non-blank query is remembered for the location (SRC-4).
    pub async fn search_tree(
        &self,
        root: Uri,
        request: &SearchRequest,
        cancel: Arc<AtomicBool>,
        out: mpsc::Sender<Vec<SearchHit>>,
    ) -> Result<SearchSummary> {
        let query = request.to_query(Local::now());
        let options = SearchOptions::new(self.search_depth(&root), cancel);
        if !query.text.is_empty() {
            let (searches, location, text) = (
                self.recent_searches.clone(),
                root.location.clone(),
                query.text.clone(),
            );
            let _ = tokio::task::spawn_blocking(move || searches.add(&location, &text)).await;
        }
        search::search(
            &self.locations,
            root,
            &query,
            options,
            &classify_by_extension,
            out,
        )
        .await
    }

    /// Recent searches of the root's location, newest first (SRC-4).
    pub fn recent_searches_of(&self, root: &Uri) -> Vec<String> {
        self.recent_searches.list(&root.location).unwrap_or_default()
    }

    /// Compares two folders (SYN-1).
    pub async fn compare_folders(
        &self,
        left: &Uri,
        right: &Uri,
        options: &CompareOptions,
        cancel: &AtomicBool,
    ) -> Result<CompareResult> {
        compare_trees(&self.locations, left, right, options, cancel).await
    }

    /// The preview counts of a sync run (SYN-2).
    pub fn sync_preview(result: &CompareResult, mode: SyncMode, excluded: &BTreeSet<VPath>) -> SyncPreview {
        preview(&sync_plan(result, mode, excluded))
    }

    /// Runs a sync as ordinary transfers (SYN-3): one copy plan per
    /// destination side, then the deletes. Returns the transfer ids.
    pub async fn run_sync(
        &self,
        result: &CompareResult,
        mode: SyncMode,
        excluded: &BTreeSet<VPath>,
    ) -> Result<Vec<TransferId>> {
        let actions: Vec<SyncAction> = sync_plan(result, mode, excluded);
        let settings = self.settings();
        let limits = LargeOpLimits {
            items: settings.large_op_items,
            bytes: settings.large_op_bytes,
        };
        let plans = build_plans(&actions, &result.left, &result.right, limits);
        let mut ids = Vec::new();
        for plan in plans.copies.into_iter().chain(plans.deletes) {
            ids.push(self.start_plan(plan).await?);
        }
        Ok(ids)
    }

    /// Sizes of the thumbnail, listing and archive caches.
    pub fn cache_sizes(&self) -> CacheSizes {
        let listings: i64 = self
            .db
            .lock()
            .query_row(
                "SELECT coalesce(sum(length(entries)), 0) FROM dircache",
                [],
                |r| r.get(0),
            )
            .unwrap_or(0);
        CacheSizes {
            thumbnails: dir_size(&self.paths.thumbs_dir()),
            listings: u64::try_from(listings).unwrap_or(0),
            archives: dir_size(&self.paths.cache_dir().join("archives")),
        }
    }

    /// *Clear cache* (SEC-5): thumbnails, stored listings and extracted
    /// archives. Crash reports and all user data stay. Returns bytes freed.
    pub fn clear_cache(&self) -> Result<u64> {
        let before = self.cache_sizes();
        remove_contents(&self.paths.thumbs_dir())?;
        remove_contents(&self.paths.cache_dir().join("archives"))?;
        self.dircache.clear()?;
        Ok(before.total())
    }

    /// *Clear app data* (DAT-3): every table of the database, Recently
    /// deleted, all caches and the netvfs socket folder (netvfs recreates
    /// it). Refused while transfers are unfinished.
    pub fn clear_app_data(&self) -> Result<()> {
        if self.engine.pending_summary() > 0 {
            return Err(Error::new(ErrorKind::Locked, "transfers are still running"));
        }
        self.wipe_tables()?;
        self.dircache.clear()?;
        remove_contents(&self.paths.trash_dir())?;
        remove_contents(&self.paths.cache_dir())?;
        remove_tree(&self.paths.data_dir().join("netvfs"))
    }

    fn wipe_tables(&self) -> Result<()> {
        let mut conn = self.db.lock();
        let tx = conn.transaction()?;
        let names = {
            let mut stmt = tx.prepare(
                "SELECT name FROM sqlite_master WHERE type = 'table' AND name NOT LIKE 'sqlite_%'",
            )?;
            let rows = stmt.query_map([], |r| r.get::<_, String>(0))?;
            rows.collect::<std::result::Result<Vec<_>, _>>()?
        };
        for name in names {
            tx.execute(&format!("DELETE FROM \"{name}\""), [])?;
        }
        tx.commit()?;
        Ok(())
    }

    /// Stores a location's preferences and applies the ones that act at
    /// once: the listing cache opt-out (SEC-5) and the remote lanes (XFR-2).
    pub fn set_location_prefs(&self, prefs: &LocationPrefs) -> Result<()> {
        // The opt-out first: switching it off writes a row, which the store
        // removes again below when everything is default.
        self.dircache
            .set_disabled(&prefs.location_id, prefs.no_listing_cache)?;
        self.location_prefs.set(prefs)?;
        let lanes = prefs.bulk_lanes.unwrap_or(self.settings().remote_lanes);
        self.engine.set_remote_limit(&prefs.location_id, lanes as usize);
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn at(y: i32, m: u32, d: u32, h: u32) -> DateTime<Local> {
        Local.with_ymd_and_hms(y, m, d, h, 30, 0).earliest().unwrap()
    }

    #[test]
    fn size_presets_have_the_documented_bounds() {
        assert_eq!(size_bounds("any"), (None, None));
        assert_eq!(size_bounds("nonsense"), (None, None));
        assert_eq!(size_bounds("gt1m"), (Some(1_000_000), None));
        assert_eq!(size_bounds("gt10m"), (Some(10_000_000), None));
        assert_eq!(size_bounds("gt100m"), (Some(100_000_000), None));
        assert_eq!(size_bounds("lt100k"), (None, Some(100_000)));
        assert_eq!(size_bounds("lt1m"), (None, Some(1_000_000)));
        for p in SIZE_PRESETS {
            assert_eq!(p == "any", size_bounds(p) == (None, None), "{p}");
        }
    }

    #[test]
    fn date_presets_are_relative_to_now() {
        let now = at(2026, 10, 3, 15);
        assert_eq!(date_lower_bound("any", now), None);
        let day = SystemTime::from(Local.with_ymd_and_hms(2026, 10, 3, 0, 0, 0).earliest().unwrap());
        assert_eq!(date_lower_bound("today", now), Some(day));
        assert_eq!(
            date_lower_bound("week", now),
            Some(SystemTime::from(now - Duration::days(7)))
        );
        assert_eq!(
            date_lower_bound("month", now),
            Some(SystemTime::from(now - Duration::days(30)))
        );
        let jan = Local.with_ymd_and_hms(2026, 1, 1, 0, 0, 0).earliest().unwrap();
        assert_eq!(date_lower_bound("year", now), Some(SystemTime::from(jan)));
    }

    #[test]
    fn requests_become_queries() {
        let now = at(2026, 10, 3, 12);
        let req = SearchRequest {
            text: "  thesis* ".to_owned(),
            mode: MatchMode::Glob,
            types: ["image".to_owned(), "bogus".to_owned()].into_iter().collect(),
            size_preset: "gt10m".to_owned(),
            date_preset: "year".to_owned(),
            include_hidden: true,
        };
        let q = req.to_query(now);
        assert_eq!(q.text, "thesis*");
        assert_eq!(q.mode, MatchMode::Glob);
        assert_eq!(q.types.iter().collect::<Vec<_>>(), ["image"]);
        assert_eq!((q.min_size, q.max_size), (Some(10_000_000), None));
        assert!(q.modified_after.is_some() && q.modified_before.is_none());
        assert!(q.include_hidden);
    }

    #[test]
    fn restricted_means_something_narrows_the_search() {
        let mut req = SearchRequest::default();
        assert!(!req.is_restricted());
        req.text = "  ".to_owned();
        assert!(!req.is_restricted(), "blank text does not narrow");
        req.text = "a".to_owned();
        assert!(req.is_restricted());
        req.text.clear();
        req.types.insert("image".to_owned());
        assert!(req.is_restricted());
        req.types.clear();
        req.size_preset = "lt1m".to_owned();
        assert!(req.is_restricted());
        req.size_preset.clear();
        req.date_preset = "week".to_owned();
        assert!(req.is_restricted());
    }

    #[test]
    fn highlight_finds_the_literal_part() {
        let sub = MatchMode::Substring;
        let glob = MatchMode::Glob;
        assert_eq!(highlight_span("My Thesis_final.pdf", "thesis", sub), Some((3, 6)));
        assert_eq!(highlight_span("thesis_final.pdf", "thesis*", glob), Some((0, 6)));
        assert_eq!(highlight_span("IMG_2026.raw", "*.raw", glob), Some((8, 4)));
        assert_eq!(highlight_span("a_long_name.txt", "a*long?", glob), Some((2, 4)));
        assert_eq!(highlight_span("abc", "", sub), None);
        assert_eq!(highlight_span("abc", "*", glob), None);
        assert_eq!(highlight_span("abc", "xyz", sub), None);
    }

    #[test]
    fn section_labels_walk_down_from_the_root() {
        let root = Uri::parse("lautta://user-documents/").unwrap();
        let deep = Uri::parse("lautta://user-documents/Uni/Thesis%20b").unwrap();
        assert_eq!(
            section_label(&root, "Documents", &deep),
            "Documents \u{203a} Uni \u{203a} Thesis b"
        );
        assert_eq!(section_label(&root, "Documents", &root), "Documents");
        let sub = Uri::parse("lautta://user-documents/Uni").unwrap();
        assert_eq!(section_label(&sub, "Uni", &deep), "Uni \u{203a} Thesis b");
    }

    #[test]
    fn excludes_parse_and_print() {
        assert_eq!(
            parse_excludes("*.tmp, .thumbnails/ ,\n*.tmp,,  x "),
            ["*.tmp", ".thumbnails/", "x"]
        );
        assert!(parse_excludes(" , ").is_empty());
        assert_eq!(excludes_text(&["a".to_owned(), "b/".to_owned()]), "a, b/");
    }

    #[test]
    fn pairs_round_trip_options() {
        let l = Uri::parse("lautta://user-pictures/").unwrap();
        let r = Uri::parse("lautta://user-downloads/x").unwrap();
        let opts = CompareOptions {
            dst_tolerance: true,
            checksums: true,
            excludes: vec!["*.tmp".to_owned()],
            ..CompareOptions::default()
        };
        let spec = pair_spec("Camera", &l, &r, SyncMode::MirrorLeftToRight, &opts);
        assert_eq!(
            (spec.label.as_str(), spec.mode),
            ("Camera", SyncMode::MirrorLeftToRight)
        );
        assert_eq!(options_of(&spec), opts);
    }

    #[test]
    fn dir_size_sums_files_and_ignores_links() {
        let dir = tempfile::tempdir().unwrap();
        std::fs::create_dir(dir.path().join("a")).unwrap();
        std::fs::write(dir.path().join("a/x"), [0u8; 10]).unwrap();
        std::fs::write(dir.path().join("y"), [0u8; 5]).unwrap();
        std::os::unix::fs::symlink(dir.path().join("y"), dir.path().join("link")).unwrap();
        assert_eq!(dir_size(dir.path()), 15);
        assert_eq!(dir_size(&dir.path().join("missing")), 0);
    }

    #[test]
    fn remove_contents_keeps_the_folder() {
        let dir = tempfile::tempdir().unwrap();
        std::fs::create_dir(dir.path().join("a")).unwrap();
        std::fs::write(dir.path().join("a/x"), [0u8; 10]).unwrap();
        std::fs::write(dir.path().join("y"), [0u8; 5]).unwrap();
        assert_eq!(remove_contents(dir.path()).unwrap(), 15);
        assert!(dir.path().is_dir());
        assert_eq!(std::fs::read_dir(dir.path()).unwrap().count(), 0);
        assert_eq!(remove_contents(&dir.path().join("missing")).unwrap(), 0);
    }

    #[test]
    fn cache_total_adds_up() {
        let c = CacheSizes {
            thumbnails: 1,
            listings: 20,
            archives: 300,
        };
        assert_eq!(c.total(), 321);
    }
}
