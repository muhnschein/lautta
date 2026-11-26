// SPDX-License-Identifier: LGPL-2.1-or-later

use super::store::{Folder, IndexStore, RootInfo, RootState};
use crate::entry::Entry;
use crate::error::{ErrorKind, Result};
use crate::provider::{list_all, Lane, Provider};
use crate::vpath::VPath;
use std::sync::atomic::{AtomicBool, Ordering};

pub const RACY_MS: i64 = 2000;
const BATCH: usize = 64;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CrawlOutcome {
    Done,
    Stopped,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum CrawlEvent {
    Folder { root: i64, path: VPath, listed: bool },
    Ready { root: i64 },
}

pub fn version_of(entry: &Entry, now_ms: i64) -> Option<Vec<u8>> {
    if let Some(etag) = entry.etag.as_ref().filter(|e| !e.is_empty()) {
        return Some(etag.clone());
    }
    let mtime = entry.modified_ms()?;
    if mtime > now_ms - RACY_MS {
        return None;
    }
    Some(mtime.to_be_bytes().to_vec())
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

    pub async fn run(&self, root: &RootInfo) -> Result<CrawlOutcome> {
        let pass = self.store.begin_pass(root.id)?;
        self.store.set_state(root.id, RootState::Crawling, None)?;
        let (top, checked) = self.store.root_folder(root.id)?;
        if checked < pass {
            if let Err(e) = self.check(root, &top, pass).await {
                self.store
                    .set_state(root.id, RootState::Failed, Some(&e.to_string()))?;
                return Err(e);
            }
        }
        loop {
            let batch = self.store.pending_folders(root.id, pass, BATCH)?;
            if batch.is_empty() {
                break;
            }
            for folder in batch {
                if self.stop.load(Ordering::Relaxed) {
                    return Ok(CrawlOutcome::Stopped);
                }
                if let Err(e) = self.check(root, &folder, pass).await {
                    if e.kind == ErrorKind::NoSpace {
                        return Err(e);
                    }
                    self.store
                        .mark_checked(root.id, &root.path, &folder.path, pass, None)?;
                }
            }
        }
        self.store.finish_pass(root.id, (self.now_ms)())?;
        self.emit(CrawlEvent::Ready { root: root.id });
        Ok(CrawlOutcome::Done)
    }

    async fn check(&self, root: &RootInfo, folder: &Folder, pass: i64) -> Result<()> {
        let stat = self.provider.stat(&folder.path, false, Lane::Bulk).await?;
        let version = version_of(&stat, (self.now_ms)());
        let unchanged = version.is_some() && version == folder.version;
        if !unchanged {
            let entries = list_all(self.provider, &folder.path, Lane::Bulk).await?;
            self.store.replace_folder(root.id, &folder.path, &entries)?;
        }
        self.store
            .mark_checked(root.id, &root.path, &folder.path, pass, version.as_deref())?;
        self.emit(CrawlEvent::Folder {
            root: root.id,
            path: folder.path.clone(),
            listed: !unchanged,
        });
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::entry::Capabilities;
    use crate::error::Error;
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
        assert_eq!(version_of(&e, NOW), Some(OLD.to_be_bytes().to_vec()));
        e.modified = Some(crate::entry::ms_to_system_time(NOW - 10));
        assert_eq!(version_of(&e, NOW), None);
        e.etag = Some(b"abc".to_vec());
        assert_eq!(version_of(&e, NOW), Some(b"abc".to_vec()));
        e.etag = Some(Vec::new());
        e.modified = None;
        assert_eq!(version_of(&e, NOW), None);
    }
}
