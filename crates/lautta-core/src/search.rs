// SPDX-License-Identifier: LGPL-2.1-or-later
//! Recursive name search (SRC-2..4): substring or glob, type, size and date
//! filters, results streamed per folder, cancelable. The tree is walked by
//! the generic [`ListWalker`] (BFS over `Provider::list`) or by a
//! provider-specific [`TreeWalker`] such as the bridge's `Walk`.

use crate::db::Db;
use crate::entry::{Entry, Kind};
use crate::error::{Error, ErrorKind, Result};
use crate::provider::{list_all, Lane, Provider, ProviderResolver};
use crate::uri::Uri;
use crate::vpath::VPath;
use async_trait::async_trait;
use futures::stream::{FuturesUnordered, StreamExt};
use std::collections::{BTreeSet, HashSet, VecDeque};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::time::SystemTime;
use tokio::sync::mpsc;

/// Listings in flight at once; remote locations cap their own lanes too.
const WALK_CONCURRENCY: usize = 2;
/// SRC-4.
pub const RECENT_SEARCHES_PER_LOCATION: usize = 10;
/// SRC-3 defaults; the caller decides which applies to a location.
pub const REMOTE_DEPTH_DEFAULT: u32 = 8;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum MatchMode {
    #[default]
    Substring,
    Glob,
}

#[derive(Debug, Clone, Default)]
pub struct SearchQuery {
    pub text: String,
    pub mode: MatchMode,
    /// Categories as returned by the `classify` closure; empty means any.
    pub types: BTreeSet<String>,
    pub min_size: Option<u64>,
    pub max_size: Option<u64>,
    pub modified_after: Option<SystemTime>,
    pub modified_before: Option<SystemTime>,
    pub include_hidden: bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SearchHit {
    pub uri: Uri,
    pub entry: Entry,
    pub folder_uri: Uri,
}

/// One folder's listing as produced by a walker.
#[derive(Debug, Clone)]
pub struct FolderListing {
    pub folder: Uri,
    pub entries: Vec<Entry>,
}

#[derive(Debug, Clone)]
pub struct WalkOptions {
    /// Deepest folder level listed, root = 0; `None` is unlimited (SRC-3).
    pub depth_limit: Option<u32>,
    pub include_hidden: bool,
    pub cancel: Arc<AtomicBool>,
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct WalkStats {
    pub folders: u64,
    /// Sub-folders that could not be listed; the search goes on without them.
    pub errors: u64,
}

/// Produces folder listings below `root`. A failure to list `root` itself is
/// an error; unreadable sub-folders are counted in the stats.
#[async_trait]
pub trait TreeWalker: Send + Sync {
    async fn walk(
        &self,
        root: &Uri,
        opts: &WalkOptions,
        out: mpsc::Sender<FolderListing>,
    ) -> Result<WalkStats>;
}

/// Generic walker: breadth first over `Provider::list` with two listings in
/// flight. Symlinked folders are never followed (loop safety) and a folder is
/// listed at most once.
pub struct ListWalker<'a> {
    resolver: &'a dyn ProviderResolver,
}

impl<'a> ListWalker<'a> {
    pub fn new(resolver: &'a dyn ProviderResolver) -> ListWalker<'a> {
        ListWalker { resolver }
    }
}

type Listed = (VPath, u32, Result<Vec<Entry>>);

async fn list_dir(provider: Arc<dyn Provider>, dir: VPath, depth: u32) -> Listed {
    let res = list_all(provider.as_ref(), &dir, Lane::Bulk).await;
    (dir, depth, res)
}

fn descend(entry: &Entry, opts: &WalkOptions) -> bool {
    entry.kind == Kind::Dir && (opts.include_hidden || !entry.is_hidden())
}

#[async_trait]
impl TreeWalker for ListWalker<'_> {
    async fn walk(
        &self,
        root: &Uri,
        opts: &WalkOptions,
        out: mpsc::Sender<FolderListing>,
    ) -> Result<WalkStats> {
        let provider = self.resolver.provider(&root.location)?;
        let mut stats = WalkStats::default();
        let mut queue: VecDeque<(VPath, u32)> = VecDeque::from([(root.path.clone(), 0)]);
        let mut seen: HashSet<VPath> = HashSet::from([root.path.clone()]);
        let mut running = FuturesUnordered::new();
        loop {
            while running.len() < WALK_CONCURRENCY && !opts.cancel.load(Ordering::Relaxed) {
                let Some((dir, depth)) = queue.pop_front() else {
                    break;
                };
                running.push(list_dir(provider.clone(), dir, depth));
            }
            let Some((dir, depth, res)) = running.next().await else {
                return Ok(stats);
            };
            if opts.cancel.load(Ordering::Relaxed) {
                return Ok(stats);
            }
            let entries = match res {
                Ok(entries) => entries,
                Err(e) if dir == root.path => return Err(e),
                Err(_) => {
                    stats.errors += 1;
                    continue;
                }
            };
            stats.folders += 1;
            enqueue_children(&dir, depth, &entries, opts, &mut seen, &mut queue);
            let folder = Uri::new(root.location.clone(), dir);
            if out.send(FolderListing { folder, entries }).await.is_err() {
                return Ok(stats);
            }
        }
    }
}

fn enqueue_children(
    dir: &VPath,
    depth: u32,
    entries: &[Entry],
    opts: &WalkOptions,
    seen: &mut HashSet<VPath>,
    queue: &mut VecDeque<(VPath, u32)>,
) {
    if opts.depth_limit.is_some_and(|limit| depth >= limit) {
        return;
    }
    for entry in entries.iter().filter(|e| descend(e, opts)) {
        if let Ok(child) = dir.join(&entry.name) {
            if seen.insert(child.clone()) {
                queue.push_back((child, depth + 1));
            }
        }
    }
}

/// A query ready to match entries.
struct Matcher<'q> {
    query: &'q SearchQuery,
    needle: String,
    glob: Option<glob::Pattern>,
}

impl<'q> Matcher<'q> {
    fn new(query: &'q SearchQuery) -> Result<Matcher<'q>> {
        let glob = match query.mode {
            MatchMode::Glob if !query.text.is_empty() => Some(
                glob::Pattern::new(&query.text.to_lowercase())
                    .map_err(|_| Error::new(ErrorKind::InvalidArgument, "invalid search pattern"))?,
            ),
            _ => None,
        };
        Ok(Matcher {
            query,
            needle: query.text.to_lowercase(),
            glob,
        })
    }

    fn name_matches(&self, entry: &Entry) -> bool {
        let name = entry.display_name().to_lowercase();
        match &self.glob {
            Some(pat) => pat.matches(&name),
            None => name.contains(&self.needle),
        }
    }

    fn attributes_match(&self, entry: &Entry) -> bool {
        let q = self.query;
        let size_ok = match entry.size {
            _ if q.min_size.is_none() && q.max_size.is_none() => true,
            Some(s) => q.min_size.map_or(true, |m| s >= m) && q.max_size.map_or(true, |m| s <= m),
            None => false,
        };
        size_ok && self.dates_match(entry)
    }

    fn dates_match(&self, entry: &Entry) -> bool {
        let q = self.query;
        if q.modified_after.is_none() && q.modified_before.is_none() {
            return true;
        }
        let Some(t) = entry.modified else {
            return false;
        };
        q.modified_after.map_or(true, |a| t >= a) && q.modified_before.map_or(true, |b| t <= b)
    }

    fn matches(&self, entry: &Entry, classify: &(dyn Fn(&Entry) -> String + Send + Sync)) -> bool {
        (self.query.include_hidden || !entry.is_hidden())
            && self.name_matches(entry)
            && self.attributes_match(entry)
            && (self.query.types.is_empty() || self.query.types.contains(&classify(entry)))
    }
}

/// What a finished search reports.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct SearchSummary {
    pub folders: u64,
    pub hits: u64,
    /// Sub-folders that could not be read.
    pub errors: u64,
    pub canceled: bool,
}

/// How a search runs: depth (SRC-3), cancellation and an optional
/// provider-specific walker (bridge `Walk`) replacing the generic listing.
#[derive(Clone)]
pub struct SearchOptions {
    /// `None` is unlimited (local); remote locations use
    /// [`REMOTE_DEPTH_DEFAULT`] unless the setting says otherwise.
    pub depth_limit: Option<u32>,
    pub cancel: Arc<AtomicBool>,
    pub walker: Option<Arc<dyn TreeWalker>>,
}

impl SearchOptions {
    pub fn new(depth_limit: Option<u32>, cancel: Arc<AtomicBool>) -> SearchOptions {
        SearchOptions {
            depth_limit,
            cancel,
            walker: None,
        }
    }

    pub fn with_walker(mut self, walker: Arc<dyn TreeWalker>) -> SearchOptions {
        self.walker = Some(walker);
        self
    }
}

/// Searches below `root`, sending one batch of hits per folder that has any
/// (SRC-2). Cancelling (the flag, or dropping the receiver) ends the search
/// early with `canceled` set; it is not an error.
pub async fn search(
    resolver: &dyn ProviderResolver,
    root: Uri,
    query: &SearchQuery,
    options: SearchOptions,
    classify: &(dyn Fn(&Entry) -> String + Send + Sync),
    out: mpsc::Sender<Vec<SearchHit>>,
) -> Result<SearchSummary> {
    let matcher = Matcher::new(query)?;
    let walk_opts = WalkOptions {
        depth_limit: options.depth_limit,
        include_hidden: query.include_hidden,
        cancel: options.cancel.clone(),
    };
    let (tx, mut rx) = mpsc::channel::<FolderListing>(4);
    let list_walker = ListWalker::new(resolver);
    let walker_ref: &dyn TreeWalker = match &options.walker {
        Some(w) => w.as_ref(),
        None => &list_walker,
    };
    let (matcher_ref, out_ref, cancel_ref) = (&matcher, &out, &options.cancel);
    // `rx` moves in so it is dropped when the consumer stops; otherwise a
    // walker blocked on a full channel would never finish.
    let consume = async move {
        let mut hits = 0u64;
        while let Some(listing) = rx.recv().await {
            let batch = hits_in(matcher_ref, classify, &listing);
            if batch.is_empty() {
                continue;
            }
            hits += batch.len() as u64;
            if out_ref.send(batch).await.is_err() {
                cancel_ref.store(true, Ordering::Relaxed);
                break;
            }
        }
        hits
    };
    let (walked, hits) = tokio::join!(walker_ref.walk(&root, &walk_opts, tx), consume);
    let stats = walked?;
    Ok(SearchSummary {
        folders: stats.folders,
        hits,
        errors: stats.errors,
        canceled: options.cancel.load(Ordering::Relaxed),
    })
}

fn hits_in(
    matcher: &Matcher<'_>,
    classify: &(dyn Fn(&Entry) -> String + Send + Sync),
    listing: &FolderListing,
) -> Vec<SearchHit> {
    listing
        .entries
        .iter()
        .filter(|e| matcher.matches(e, classify))
        .filter_map(|e| {
            Some(SearchHit {
                uri: listing.folder.join(&e.name).ok()?,
                entry: e.clone(),
                folder_uri: listing.folder.clone(),
            })
        })
        .collect()
}

/// A local, extension-based classification for the type filter: `folder`,
/// `image`, `video`, `audio`, `document`, `archive`, `text` or `other`.
pub fn classify_by_extension(entry: &Entry) -> String {
    if entry.is_dir() {
        return "folder".to_owned();
    }
    let name = entry.display_name().to_lowercase();
    let ext = name.rsplit_once('.').map_or("", |(_, e)| e);
    let category = match ext {
        "jpg" | "jpeg" | "png" | "gif" | "webp" | "bmp" | "svg" | "heic" | "tiff" => "image",
        "mp4" | "mkv" | "avi" | "mov" | "webm" | "3gp" => "video",
        "mp3" | "ogg" | "oga" | "flac" | "wav" | "m4a" | "opus" | "aac" => "audio",
        "pdf" | "doc" | "docx" | "odt" | "xls" | "xlsx" | "ods" | "ppt" | "pptx" | "odp" => "document",
        "zip" | "tar" | "gz" | "bz2" | "xz" | "zst" | "7z" | "rar" => "archive",
        "txt" | "md" | "log" | "csv" | "json" | "xml" | "ini" | "conf" => "text",
        _ => "other",
    };
    category.to_owned()
}

/// Recent search strings per location (SRC-4).
#[derive(Clone)]
pub struct RecentSearches {
    db: Db,
}

impl RecentSearches {
    pub fn new(db: Db) -> RecentSearches {
        RecentSearches { db }
    }

    /// Adds `query` (trimmed; blank is ignored) as the newest entry of
    /// `location`, keeping the newest ten.
    pub fn add(&self, location: &str, query: &str) -> Result<()> {
        self.add_at(location, query, crate::org::now_ms())
    }

    pub fn add_at(&self, location: &str, query: &str, at_ms: i64) -> Result<()> {
        let query = query.trim();
        if query.is_empty() {
            return Ok(());
        }
        let conn = self.db.lock();
        conn.execute(
            "INSERT INTO recent_searches(location_id, query, at_ms) VALUES (?1, ?2, ?3)
             ON CONFLICT(location_id, query) DO UPDATE SET at_ms = excluded.at_ms",
            (location, query, at_ms),
        )?;
        conn.execute(
            "DELETE FROM recent_searches WHERE location_id = ?1 AND id NOT IN
             (SELECT id FROM recent_searches WHERE location_id = ?1
              ORDER BY at_ms DESC, id DESC LIMIT ?2)",
            (location, RECENT_SEARCHES_PER_LOCATION as i64),
        )?;
        Ok(())
    }

    /// Newest first.
    pub fn list(&self, location: &str) -> Result<Vec<String>> {
        let conn = self.db.lock();
        let mut stmt = conn.prepare(
            "SELECT query FROM recent_searches WHERE location_id = ?1 ORDER BY at_ms DESC, id DESC",
        )?;
        let rows = stmt
            .query_map([location], |r| r.get::<_, String>(0))?
            .collect::<std::result::Result<Vec<_>, _>>()?;
        Ok(rows)
    }

    pub fn clear(&self, location: &str) -> Result<usize> {
        let n = self
            .db
            .lock()
            .execute("DELETE FROM recent_searches WHERE location_id = ?1", [location])?;
        Ok(n)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::provider::memory::MemoryProvider;
    use crate::provider::StaticResolver;
    use std::time::Duration;

    fn fixture() -> (MemoryProvider, StaticResolver) {
        let m = MemoryProvider::default();
        let res = StaticResolver::default().with("l", Arc::new(m.clone()));
        (m, res)
    }

    fn at(s: u64) -> SystemTime {
        SystemTime::UNIX_EPOCH + Duration::from_secs(s)
    }

    async fn run(
        res: &StaticResolver,
        root: &str,
        query: &SearchQuery,
        depth: Option<u32>,
    ) -> (Vec<Vec<SearchHit>>, SearchSummary) {
        run_with(res, root, query, depth, Arc::new(AtomicBool::new(false)), None).await
    }

    async fn run_with(
        res: &StaticResolver,
        root: &str,
        query: &SearchQuery,
        depth: Option<u32>,
        cancel: Arc<AtomicBool>,
        walker: Option<Arc<dyn TreeWalker>>,
    ) -> (Vec<Vec<SearchHit>>, SearchSummary) {
        let (tx, mut rx) = mpsc::channel(64);
        let uri = Uri::parse(&format!("lautta://l/{root}")).unwrap();
        let mut options = SearchOptions::new(depth, cancel);
        options.walker = walker;
        let summary = search(res, uri, query, options, &classify_by_extension, tx)
            .await
            .unwrap();
        let mut batches = Vec::new();
        while let Some(b) = rx.recv().await {
            batches.push(b);
        }
        (batches, summary)
    }

    fn names(batches: &[Vec<SearchHit>]) -> Vec<String> {
        let mut v: Vec<String> = batches.iter().flatten().map(|h| h.uri.path.display()).collect();
        v.sort();
        v
    }

    fn q(text: &str) -> SearchQuery {
        SearchQuery {
            text: text.to_owned(),
            ..SearchQuery::default()
        }
    }

    #[tokio::test]
    async fn substring_is_case_insensitive_and_results_group_by_folder() {
        let (m, res) = fixture();
        m.add_file("Report.pdf", b"x", 0);
        m.add_file("a/report-2.txt", b"x", 0);
        m.add_file("a/b/REPORT.md", b"x", 0);
        m.add_file("a/other", b"x", 0);
        let (batches, summary) = run(&res, "", &q("report"), None).await;
        assert_eq!(names(&batches), ["Report.pdf", "a/b/REPORT.md", "a/report-2.txt"]);
        assert_eq!(batches.len(), 3, "one batch per folder with hits");
        for batch in &batches {
            let folders: HashSet<_> = batch.iter().map(|h| h.folder_uri.clone()).collect();
            assert_eq!(folders.len(), 1);
        }
        assert_eq!(summary.hits, 3);
        assert_eq!(summary.folders, 3);
        assert!(!summary.canceled);
    }

    #[tokio::test]
    async fn glob_matches_the_whole_name() {
        let (m, res) = fixture();
        m.add_file("a.jpg", b"x", 0);
        m.add_file("a.jpg.bak", b"x", 0);
        m.add_file("d/B.JPG", b"x", 0);
        let query = SearchQuery {
            text: "*.jpg".into(),
            mode: MatchMode::Glob,
            ..SearchQuery::default()
        };
        let (batches, _) = run(&res, "", &query, None).await;
        assert_eq!(names(&batches), ["a.jpg", "d/B.JPG"]);
        let bad = SearchQuery {
            text: "[".into(),
            mode: MatchMode::Glob,
            ..SearchQuery::default()
        };
        let (tx, _rx) = mpsc::channel(1);
        let err = search(
            &res,
            Uri::root("l"),
            &bad,
            SearchOptions::new(None, Arc::new(AtomicBool::new(false))),
            &classify_by_extension,
            tx,
        )
        .await
        .unwrap_err();
        assert_eq!(err.kind, ErrorKind::InvalidArgument);
    }

    #[tokio::test]
    async fn empty_text_with_filters_lists_everything_that_passes() {
        let (m, res) = fixture();
        m.add_file("small", b"1", 0);
        m.add_file("big", &[0u8; 100], 0);
        m.add_file("d/mid", &[0u8; 50], 0);
        let query = SearchQuery {
            min_size: Some(50),
            max_size: Some(100),
            ..SearchQuery::default()
        };
        let (batches, _) = run(&res, "", &query, None).await;
        assert_eq!(names(&batches), ["big", "d/mid"]);
        let query = SearchQuery {
            min_size: Some(51),
            ..SearchQuery::default()
        };
        let (batches, _) = run(&res, "", &query, None).await;
        assert_eq!(names(&batches), ["big"]);
    }

    #[tokio::test]
    async fn date_filters_are_inclusive_and_exclude_unknown_dates() {
        let (m, res) = fixture();
        m.add_file("old", b"x", 1_000_000);
        m.add_file("mid", b"x", 2_000_000);
        m.add_file("new", b"x", 3_000_000);
        let query = SearchQuery {
            modified_after: Some(at(2_000)),
            modified_before: Some(at(3_000)),
            ..SearchQuery::default()
        };
        let (batches, _) = run(&res, "", &query, None).await;
        assert_eq!(names(&batches), ["mid", "new"]);
        // Folders carry no mtime in the memory provider's fixture (epoch).
        let dated = SearchQuery {
            modified_after: Some(at(1)),
            ..SearchQuery::default()
        };
        m.add_dir("folder");
        let (batches, _) = run(&res, "", &dated, None).await;
        assert!(!names(&batches).contains(&"folder".to_owned()));
    }

    #[tokio::test]
    async fn type_filter_uses_the_classifier() {
        let (m, res) = fixture();
        m.add_file("a.png", b"x", 0);
        m.add_file("b.txt", b"x", 0);
        m.add_file("c.zip", b"x", 0);
        m.add_dir("pics");
        let query = SearchQuery {
            types: ["image".to_owned(), "folder".to_owned()].into_iter().collect(),
            ..SearchQuery::default()
        };
        let (batches, _) = run(&res, "", &query, None).await;
        assert_eq!(names(&batches), ["a.png", "pics"]);
        assert_eq!(
            classify_by_extension(&Entry::new(b"x.TAR", Kind::File)),
            "archive"
        );
        assert_eq!(classify_by_extension(&Entry::new(b"noext", Kind::File)), "other");
        assert_eq!(classify_by_extension(&Entry::new(b"s.mp3", Kind::File)), "audio");
        assert_eq!(classify_by_extension(&Entry::new(b"s.mkv", Kind::File)), "video");
        assert_eq!(
            classify_by_extension(&Entry::new(b"s.odt", Kind::File)),
            "document"
        );
        assert_eq!(classify_by_extension(&Entry::new(b"s.md", Kind::File)), "text");
    }

    #[tokio::test]
    async fn hidden_entries_and_folders_need_include_hidden() {
        let (m, res) = fixture();
        m.add_file(".secret", b"x", 0);
        m.add_file(".git/config-note", b"x", 0);
        m.add_file("note", b"x", 0);
        let (batches, _) = run(&res, "", &q("o"), None).await;
        assert_eq!(names(&batches), ["note"]);
        let query = SearchQuery {
            text: "o".into(),
            include_hidden: true,
            ..SearchQuery::default()
        };
        let (batches, _) = run(&res, "", &query, None).await;
        assert_eq!(names(&batches), [".git/config-note", "note"]);
    }

    #[tokio::test]
    async fn depth_limit_counts_folder_levels_below_the_root() {
        let (m, res) = fixture();
        m.add_file("hit0", b"x", 0);
        m.add_file("a/hit1", b"x", 0);
        m.add_file("a/b/hit2", b"x", 0);
        m.add_file("a/b/c/hit3", b"x", 0);
        for (limit, expect) in [
            (Some(0), vec!["hit0"]),
            (Some(1), vec!["a/hit1", "hit0"]),
            (Some(2), vec!["a/b/hit2", "a/hit1", "hit0"]),
            (None, vec!["a/b/c/hit3", "a/b/hit2", "a/hit1", "hit0"]),
        ] {
            let (batches, _) = run(&res, "", &q("hit"), limit).await;
            assert_eq!(names(&batches), expect, "limit {limit:?}");
        }
    }

    #[tokio::test]
    async fn searches_below_a_sub_root_only() {
        let (m, res) = fixture();
        m.add_file("a/hit", b"x", 0);
        m.add_file("b/hit", b"x", 0);
        let (batches, _) = run(&res, "a", &q("hit"), None).await;
        assert_eq!(names(&batches), ["a/hit"]);
    }

    #[tokio::test]
    async fn symlinked_folders_are_not_followed() {
        let (m, res) = fixture();
        m.add_file("real/hit", b"x", 0);
        m.add_symlink("loop", "/");
        m.add_symlink("alias", "real");
        let (batches, summary) = run(&res, "", &q("hit"), None).await;
        assert_eq!(names(&batches), ["real/hit"]);
        assert_eq!(summary.folders, 2);
    }

    #[tokio::test]
    async fn unreadable_subfolders_are_counted_not_fatal_but_a_bad_root_is() {
        let (m, res) = fixture();
        m.add_file("ok/hit", b"x", 0);
        m.add_file("bad/hit", b"x", 0);
        m.fail_next("list", "bad", Error::kind(ErrorKind::PermissionDenied));
        let (batches, summary) = run(&res, "", &q("hit"), None).await;
        assert_eq!(names(&batches), ["ok/hit"]);
        assert_eq!(summary.errors, 1);
        m.fail_next("list", "", Error::kind(ErrorKind::ConnectionLost));
        let (tx, _rx) = mpsc::channel(1);
        let err = search(
            &res,
            Uri::root("l"),
            &q("x"),
            SearchOptions::new(None, Arc::new(AtomicBool::new(false))),
            &classify_by_extension,
            tx,
        )
        .await
        .unwrap_err();
        assert_eq!(err.kind, ErrorKind::ConnectionLost);
    }

    #[tokio::test]
    async fn cancel_flag_stops_the_walk_without_error() {
        let (m, res) = fixture();
        m.add_file("a/hit", b"x", 0);
        let cancel = Arc::new(AtomicBool::new(true));
        let (batches, summary) = run_with(&res, "", &q("hit"), None, cancel, None).await;
        assert!(batches.is_empty());
        assert!(summary.canceled);
    }

    #[tokio::test]
    async fn dropping_the_receiver_cancels() {
        let (m, res) = fixture();
        for i in 0..20 {
            m.add_file(&format!("d{i}/hit"), b"x", 0);
        }
        let (tx, rx) = mpsc::channel(1);
        drop(rx);
        let summary = search(
            &res,
            Uri::root("l"),
            &q("hit"),
            SearchOptions::new(None, Arc::new(AtomicBool::new(false))),
            &classify_by_extension,
            tx,
        )
        .await
        .unwrap();
        assert!(summary.canceled);
        assert!(summary.folders < 21);
    }

    #[tokio::test]
    async fn each_folder_is_listed_exactly_once() {
        let (m, res) = fixture();
        for i in 0..10 {
            m.add_file(&format!("d{i}/hit"), b"x", 0);
        }
        let (batches, summary) = run(&res, "", &q("hit"), None).await;
        assert_eq!(batches.len(), 10);
        assert_eq!(summary.folders, 11);
        let lists = m.calls().iter().filter(|c| c.starts_with("list")).count();
        assert_eq!(lists, 11);
    }

    struct FixedWalker;

    #[async_trait]
    impl TreeWalker for FixedWalker {
        async fn walk(
            &self,
            root: &Uri,
            opts: &WalkOptions,
            out: mpsc::Sender<FolderListing>,
        ) -> Result<WalkStats> {
            assert_eq!(opts.depth_limit, Some(8));
            let entries = vec![
                Entry::new(b"server-hit", Kind::File),
                Entry::new(b"miss", Kind::File),
            ];
            let _ = out
                .send(FolderListing {
                    folder: root.clone(),
                    entries,
                })
                .await;
            Ok(WalkStats {
                folders: 1,
                errors: 0,
            })
        }
    }

    #[tokio::test]
    async fn a_provider_walker_replaces_generic_listing() {
        let (m, res) = fixture();
        m.add_file("local-hit", b"x", 0);
        let walker: Arc<dyn TreeWalker> = Arc::new(FixedWalker);
        let (batches, summary) = run_with(
            &res,
            "",
            &q("hit"),
            Some(8),
            Arc::new(AtomicBool::new(false)),
            Some(walker),
        )
        .await;
        assert_eq!(names(&batches), ["server-hit"]);
        assert_eq!(summary.folders, 1);
        assert!(m.calls().is_empty());
    }

    #[test]
    fn recent_searches_are_per_location_capped_deduped_and_clearable() {
        let rs = RecentSearches::new(Db::open_in_memory().unwrap());
        for i in 0..12 {
            rs.add_at("nas", &format!("q{i}"), i).unwrap();
        }
        rs.add_at("nas", "  q5 ", 100).unwrap();
        rs.add_at("nas", "   ", 101).unwrap();
        rs.add_at("sd", "other", 1).unwrap();
        let list = rs.list("nas").unwrap();
        assert_eq!(list.len(), RECENT_SEARCHES_PER_LOCATION);
        assert_eq!(&list[..3], ["q5", "q11", "q10"]);
        assert!(!list.contains(&"q0".to_owned()) && !list.contains(&"q1".to_owned()));
        assert_eq!(rs.list("sd").unwrap(), ["other"]);
        assert_eq!(rs.clear("nas").unwrap(), 10);
        assert!(rs.list("nas").unwrap().is_empty());
        assert_eq!(rs.list("sd").unwrap().len(), 1);
    }
}
