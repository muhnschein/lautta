// SPDX-License-Identifier: LGPL-2.1-or-later

use super::store::{Folder, IndexStore, RootInfo, RootState};
use crate::entry::{cap, Entry};
use crate::error::{Error, ErrorKind, Result};
use crate::provider::{FolderCheck, KnownFolder, Lane, Provider, RACY_MS};
use crate::vpath::VPath;
use std::sync::atomic::{AtomicBool, Ordering};

const BATCH: usize = 64;
const ETAG: u8 = b'e';
const DATE: u8 = b'm';

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CrawlOutcome {
    Done,
    Stopped,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum CrawlEvent {
    Folder { root: i64, path: VPath, listed: bool },
    Skipped { root: i64, path: VPath, folders: usize },
    Ready { root: i64 },
}

pub fn version_of(entry: &Entry, now_ms: i64) -> Option<Vec<u8>> {
    if let Some(etag) = entry.etag.as_ref().filter(|e| !e.is_empty()) {
        return Some([&[ETAG][..], etag].concat());
    }
    let mtime = entry.modified_ms()?;
    if mtime < 0 || mtime > now_ms - RACY_MS {
        return None;
    }
    Some([&[DATE][..], &mtime.to_be_bytes()].concat())
}

pub fn known_folder(folder: &Folder) -> KnownFolder {
    let (etag, modified_ms) = match folder.version.as_deref() {
        Some([ETAG, etag @ ..]) if !etag.is_empty() => (Some(etag.to_vec()), None),
        Some([DATE, date @ ..]) => (None, <[u8; 8]>::try_from(date).ok().map(i64::from_be_bytes)),
        _ => (None, None),
    };
    KnownFolder {
        path: folder.path.clone(),
        etag,
        modified_ms,
    }
}

pub struct Crawl<'a> {
    pub store: &'a IndexStore,
    pub provider: &'a dyn Provider,
    pub stop: &'a AtomicBool,
    pub now_ms: &'a (dyn Fn() -> i64 + Send + Sync),
    pub events: Option<&'a (dyn Fn(CrawlEvent) + Send + Sync)>,
}

impl Crawl<'_> {
    fn emit(&self, event: CrawlEvent) {
        if let Some(events) = self.events {
            events(event);
        }
    }

    fn failed(&self, root: &RootInfo, e: Error) -> Result<CrawlOutcome> {
        self.store
            .set_state(root.id, RootState::Failed, Some(&e.to_string()))?;
        Err(e)
    }

    pub async fn run(&self, root: &RootInfo) -> Result<CrawlOutcome> {
        let pass = self.store.begin_pass(root.id)?;
        self.store.set_state(root.id, RootState::Crawling, None)?;
        let deep = self
            .provider
            .known_capabilities()
            .await
            .raw
            .contains(cap::DEEP_ETAGS);
        let (top, checked) = self.store.root_folder(root.id)?;
        if checked < pass {
            let first = self
                .provider
                .check_folders(&[known_folder(&top)], (self.now_ms)(), Lane::Bulk)
                .await
                .map(|mut checks| checks.pop());
            let check = match first {
                Ok(Some(FolderCheck::Failed(e))) | Err(e) => return self.failed(root, e),
                Ok(Some(check)) => check,
                Ok(None) => {
                    return self.failed(
                        root,
                        Error::new(ErrorKind::ProtocolError, "no answer for the top folder"),
                    )
                }
            };
            let mark = self.apply(root, top, check, pass, deep)?;
            self.store.mark_checked_many(root.id, &root.path, pass, &[mark])?;
        }
        loop {
            let mut batch = self.store.pending_folders(root.id, pass, BATCH)?;
            let Some(level) = batch.iter().map(|f| f.path.depth()).min() else {
                break;
            };
            batch.retain(|f| f.path.depth() == level);
            if self.stop.load(Ordering::Relaxed) {
                return Ok(CrawlOutcome::Stopped);
            }
            let known: Vec<KnownFolder> = batch.iter().map(known_folder).collect();
            let checks = match self
                .provider
                .check_folders(&known, (self.now_ms)(), Lane::Bulk)
                .await
            {
                Ok(checks) => checks,
                Err(e) => return self.failed(root, e),
            };
            let mut marks = Vec::with_capacity(batch.len());
            let mut checks = checks.into_iter();
            for folder in batch {
                let check = checks.next().unwrap_or_else(|| {
                    FolderCheck::Failed(Error::new(ErrorKind::ProtocolError, "no answer for a folder"))
                });
                match self.apply(root, folder, check, pass, deep) {
                    Ok(mark) => marks.push(mark),
                    Err(e) => {
                        let _ = self.store.mark_checked_many(root.id, &root.path, pass, &marks);
                        return Err(e);
                    }
                }
            }
            self.store.mark_checked_many(root.id, &root.path, pass, &marks)?;
        }
        self.store.finish_pass(root.id, (self.now_ms)())?;
        self.emit(CrawlEvent::Ready { root: root.id });
        Ok(CrawlOutcome::Done)
    }

    fn apply(
        &self,
        root: &RootInfo,
        folder: Folder,
        check: FolderCheck,
        pass: i64,
        deep: bool,
    ) -> Result<(VPath, Option<Vec<u8>>)> {
        let (entry, listed) = match check {
            FolderCheck::Failed(_) => return Ok((folder.path, None)),
            FolderCheck::Unchanged(entry) => (entry, false),
            FolderCheck::Listed(entry, children) => {
                self.store.replace_folder(root.id, &folder.path, &children)?;
                (entry, true)
            }
        };
        if !listed && deep && entry.etag.as_ref().is_some_and(|e| !e.is_empty()) {
            let folders = self.store.mark_subtree_checked(root.id, &folder.path, pass)?;
            self.emit(CrawlEvent::Skipped {
                root: root.id,
                path: folder.path.clone(),
                folders,
            });
        }
        self.emit(CrawlEvent::Folder {
            root: root.id,
            path: folder.path.clone(),
            listed,
        });
        Ok((folder.path, version_of(&entry, (self.now_ms)())))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::entry::Capabilities;
    use crate::index::store::QUERY_LIMIT;
    use crate::provider::memory::MemoryProvider;
    use std::sync::Mutex;

    const NOW: i64 = 1_800_000_000_000;
    const OLD: i64 = 1_700_000_000_000;

    struct Rig {
        _dir: tempfile::TempDir,
        store: IndexStore,
        mem: MemoryProvider,
        root: RootInfo,
    }

    fn rig() -> Rig {
        let dir = tempfile::tempdir().unwrap();
        let store = IndexStore::open(&dir.path().join("names.db")).unwrap();
        let mem = MemoryProvider::new(Capabilities::with(&[]));
        mem.add_file("a/note-a.txt", b"x", OLD);
        mem.add_file("a/deep/note-deep.txt", b"x", OLD);
        mem.add_file("b/note-b.txt", b"x", OLD);
        mem.add_file("note-top.txt", b"x", OLD);
        for d in ["", "a", "a/deep", "b"] {
            mem.set_mtime(d, OLD);
        }
        let root = store.root("l", &VPath::root()).unwrap();
        Rig {
            _dir: dir,
            store,
            mem,
            root,
        }
    }

    async fn crawl(r: &Rig, stop: &AtomicBool) -> (Result<CrawlOutcome>, Vec<String>) {
        let lists = Mutex::new(Vec::new());
        let events = |e: CrawlEvent| {
            if let CrawlEvent::Folder {
                path, listed: true, ..
            } = e
            {
                lists.lock().unwrap().push(path.display());
            }
        };
        let now = || NOW;
        let out = Crawl {
            store: &r.store,
            provider: &r.mem,
            stop,
            now_ms: &now,
            events: Some(&events),
        }
        .run(&r.root)
        .await;
        let mut listed = lists.into_inner().unwrap();
        listed.sort();
        (out, listed)
    }

    fn names(r: &Rig, text: &str) -> Vec<String> {
        let mut out: Vec<String> = r
            .store
            .query(&[r.root.id], text, QUERY_LIMIT)
            .unwrap()
            .into_iter()
            .map(|h| h.folder.join(&h.entry.name).unwrap().display())
            .collect();
        out.sort();
        out
    }

    fn list_calls(r: &Rig) -> usize {
        r.mem.calls().iter().filter(|c| c.starts_with("list ")).count()
    }

    #[tokio::test]
    async fn a_first_crawl_lists_every_folder_once() {
        let r = rig();
        let (out, listed) = crawl(&r, &AtomicBool::new(false)).await;
        assert_eq!(out.unwrap(), CrawlOutcome::Done);
        assert_eq!(listed, vec!["", "a", "a/deep", "b"]);
        assert_eq!(list_calls(&r), 4);
        assert_eq!(
            names(&r, "note"),
            vec![
                "a/deep/note-deep.txt",
                "a/note-a.txt",
                "b/note-b.txt",
                "note-top.txt"
            ]
        );
        assert_eq!(
            r.store.root_info(r.root.id).unwrap().unwrap().state,
            RootState::Ready
        );
    }

    #[tokio::test]
    async fn an_unchanged_tree_is_only_stat_ed_and_a_changed_folder_listed_again() {
        let r = rig();
        crawl(&r, &AtomicBool::new(false)).await.0.unwrap();
        let before = list_calls(&r);
        let (out, listed) = crawl(&r, &AtomicBool::new(false)).await;
        assert_eq!(out.unwrap(), CrawlOutcome::Done);
        assert!(listed.is_empty(), "nothing changed, nothing listed: {listed:?}");
        assert_eq!(list_calls(&r), before);
        r.mem.add_file("a/deep/new-note.txt", b"x", OLD);
        r.mem.set_mtime("a/deep", OLD + 5_000);
        let (_, listed) = crawl(&r, &AtomicBool::new(false)).await;
        assert_eq!(listed, vec!["a/deep"], "only the changed folder");
        assert!(names(&r, "new-note").contains(&"a/deep/new-note.txt".to_owned()));
    }

    #[tokio::test]
    async fn a_folder_changed_just_now_is_listed_again_next_time() {
        let r = rig();
        r.mem.set_mtime("b", NOW - RACY_MS / 2);
        crawl(&r, &AtomicBool::new(false)).await.0.unwrap();
        let (_, listed) = crawl(&r, &AtomicBool::new(false)).await;
        assert_eq!(listed, vec!["b"], "a racy date is never trusted");
    }

    #[tokio::test]
    async fn a_stopped_crawl_resumes_where_it_stopped() {
        let r = rig();
        let stop = AtomicBool::new(false);
        let lists = Mutex::new(Vec::new());
        let events = |e: CrawlEvent| {
            if let CrawlEvent::Folder { path, .. } = e {
                lists.lock().unwrap().push(path.display());
                if path.display() == "a" {
                    stop.store(true, Ordering::Relaxed);
                }
            }
        };
        let now = || NOW;
        let out = Crawl {
            store: &r.store,
            provider: &r.mem,
            stop: &stop,
            now_ms: &now,
            events: Some(&events),
        }
        .run(&r.root)
        .await
        .unwrap();
        assert_eq!(out, CrawlOutcome::Stopped);
        assert_ne!(
            r.store.root_info(r.root.id).unwrap().unwrap().state,
            RootState::Ready
        );
        let first = lists.into_inner().unwrap();
        let (out, second) = crawl(&r, &AtomicBool::new(false)).await;
        assert_eq!(out.unwrap(), CrawlOutcome::Done);
        let mut all = first.clone();
        all.extend(second.clone());
        all.sort();
        assert_eq!(
            all,
            vec!["", "a", "a/deep", "b"],
            "every folder once over both runs: {first:?} then {second:?}"
        );
    }

    fn deep_rig() -> Rig {
        let dir = tempfile::tempdir().unwrap();
        let store = IndexStore::open(&dir.path().join("names.db")).unwrap();
        let mem = MemoryProvider::new(Capabilities::with(&[])).with_deep_etags();
        for path in ["a/x/1.txt", "a/y/deep/2.txt", "b/3.txt", "b/z/4.txt", "top.txt"] {
            mem.add_file(path, b"x", NOW);
        }
        let root = store.root("l", &VPath::root()).unwrap();
        Rig {
            _dir: dir,
            store,
            mem,
            root,
        }
    }

    fn stats(r: &Rig) -> Vec<String> {
        let mut calls: Vec<String> = r
            .mem
            .calls()
            .iter()
            .filter_map(|c| c.strip_prefix("stat ").map(str::to_owned))
            .collect();
        calls.sort();
        calls
    }

    #[tokio::test]
    async fn with_deep_etags_an_unchanged_tree_costs_one_request() {
        let r = deep_rig();
        let (out, listed) = crawl(&r, &AtomicBool::new(false)).await;
        assert_eq!(out.unwrap(), CrawlOutcome::Done);
        assert_eq!(listed.len(), 7, "every folder once: {listed:?}");
        r.mem.clear_calls();
        let (out, listed) = crawl(&r, &AtomicBool::new(false)).await;
        assert_eq!(out.unwrap(), CrawlOutcome::Done);
        assert!(listed.is_empty());
        assert_eq!(stats(&r), vec![""], "the top folder's etag covers everything");
        assert_eq!(names(&r, "2.txt"), vec!["a/y/deep/2.txt"], "the index is kept");
    }

    #[tokio::test]
    async fn with_deep_etags_only_the_changed_branch_is_walked() {
        let r = deep_rig();
        crawl(&r, &AtomicBool::new(false)).await.0.unwrap();
        r.mem.add_file("a/y/deep/new.txt", b"x", NOW);
        r.mem.clear_calls();
        let skipped = Mutex::new(Vec::new());
        let events = |e: CrawlEvent| {
            if let CrawlEvent::Skipped { path, folders, .. } = e {
                skipped.lock().unwrap().push((path.display(), folders));
            }
        };
        let now = || NOW;
        let stop = AtomicBool::new(false);
        Crawl {
            store: &r.store,
            provider: &r.mem,
            stop: &stop,
            now_ms: &now,
            events: Some(&events),
        }
        .run(&r.root)
        .await
        .unwrap();
        assert_eq!(
            stats(&r),
            vec!["", "a", "a/x", "a/y", "a/y/deep", "b"],
            "b/z is never asked"
        );
        let lists: Vec<String> = r
            .mem
            .calls()
            .iter()
            .filter_map(|c| c.strip_prefix("list ").map(str::to_owned))
            .collect();
        assert_eq!(
            lists,
            vec!["", "a", "a/y", "a/y/deep"],
            "the path to the change, nothing else"
        );
        let mut skipped = skipped.into_inner().unwrap();
        skipped.sort();
        assert_eq!(skipped, vec![("a/x".to_owned(), 0), ("b".to_owned(), 1)]);
        assert_eq!(names(&r, "new.txt"), vec!["a/y/deep/new.txt"]);
        assert_eq!(names(&r, "4.txt"), vec!["b/z/4.txt"]);
    }

    #[tokio::test]
    async fn without_deep_etags_an_etag_covers_its_own_folder_only() {
        let dir = tempfile::tempdir().unwrap();
        let store = IndexStore::open(&dir.path().join("names.db")).unwrap();
        let mem = MemoryProvider::new(Capabilities::with(&[]));
        for path in ["a/x/1.txt", "b/2.txt"] {
            mem.add_file(path, b"x", OLD);
        }
        for d in ["", "a", "a/x", "b"] {
            mem.set_mtime(d, OLD);
        }
        let root = store.root("l", &VPath::root()).unwrap();
        let r = Rig {
            _dir: dir,
            store,
            mem,
            root,
        };
        crawl(&r, &AtomicBool::new(false)).await.0.unwrap();
        r.mem.clear_calls();
        crawl(&r, &AtomicBool::new(false)).await.0.unwrap();
        assert_eq!(stats(&r), vec!["", "a", "a/x", "b"], "every folder is asked");
    }

    #[tokio::test]
    async fn links_to_folders_are_not_followed() {
        let r = rig();
        r.mem.add_symlink("a/loop", "..");
        let (out, listed) = crawl(&r, &AtomicBool::new(false)).await;
        assert_eq!(out.unwrap(), CrawlOutcome::Done);
        assert_eq!(listed, vec!["", "a", "a/deep", "b"]);
    }

    #[tokio::test]
    async fn an_unreadable_root_fails_and_an_unreadable_folder_is_skipped() {
        let r = rig();
        r.mem
            .fail_next("list", "b", Error::kind(ErrorKind::PermissionDenied));
        let (out, _) = crawl(&r, &AtomicBool::new(false)).await;
        assert_eq!(out.unwrap(), CrawlOutcome::Done);
        assert!(names(&r, "note-b").is_empty());
        assert_eq!(names(&r, "note-a"), vec!["a/note-a.txt"]);
        let r = rig();
        r.mem
            .fail_next("stat", "", Error::kind(ErrorKind::ConnectionLost));
        let (out, _) = crawl(&r, &AtomicBool::new(false)).await;
        assert_eq!(out.unwrap_err().kind, ErrorKind::ConnectionLost);
        assert_eq!(
            r.store.root_info(r.root.id).unwrap().unwrap().state,
            RootState::Failed
        );
    }

    #[test]
    fn versions_prefer_etags_and_distrust_fresh_dates() {
        let mut e = Entry::new(b"x", crate::entry::Kind::Dir);
        e.modified = Some(crate::entry::ms_to_system_time(OLD));
        let dated = version_of(&e, NOW).unwrap();
        assert_eq!(dated, [&b"m"[..], &OLD.to_be_bytes()].concat());
        let folder = |version: Option<Vec<u8>>| Folder {
            path: VPath::root(),
            version,
        };
        assert_eq!(known_folder(&folder(Some(dated))).modified_ms, Some(OLD));
        e.modified = Some(crate::entry::ms_to_system_time(NOW - 10));
        assert_eq!(version_of(&e, NOW), None);
        e.modified = Some(crate::entry::ms_to_system_time(NOW - RACY_MS));
        assert!(
            version_of(&e, NOW).is_some(),
            "exactly two seconds old is settled"
        );
        e.etag = Some(b"abc".to_vec());
        assert_eq!(version_of(&e, NOW), Some(b"eabc".to_vec()));
        let known = known_folder(&folder(Some(b"eabc".to_vec())));
        assert_eq!(
            (known.etag.as_deref(), known.modified_ms),
            (Some(&b"abc"[..]), None)
        );
        e.etag = Some(Vec::new());
        e.modified = None;
        assert_eq!(version_of(&e, NOW), None);
        for odd in [
            None,
            Some(Vec::new()),
            Some(b"e".to_vec()),
            Some(b"m123".to_vec()),
            Some(b"x".to_vec()),
        ] {
            let known = known_folder(&folder(odd.clone()));
            assert_eq!((known.etag, known.modified_ms), (None, None), "{odd:?}");
        }
    }
}
