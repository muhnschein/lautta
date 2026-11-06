// SPDX-License-Identifier: LGPL-2.1-or-later
//! Own binary, one test: counts this process's inotify instances and watch threads.

use lautta_core::app::Core;
use lautta_core::locations::LocationRegistry;
use lautta_core::paths::AppPaths;
use lautta_core::watch::WatchEvent;
use std::path::Path;
use std::time::Duration;

fn inotify_instances() -> usize {
    std::fs::read_dir("/proc/self/fd")
        .unwrap()
        .filter_map(|e| std::fs::read_link(e.ok()?.path()).ok())
        .filter(|target| target == Path::new("anon_inode:inotify"))
        .count()
}

fn watch_threads() -> Vec<String> {
    std::fs::read_dir("/proc/self/task")
        .unwrap()
        .filter_map(|e| {
            let task = e.ok()?.path();
            let name = std::fs::read_to_string(task.join("comm")).ok()?;
            (name.trim() == "lautta-watch").then(|| task.display().to_string())
        })
        .collect()
}

/// Polls up to 30 s: loaded runners are slow.
async fn eventually(what: &str, cond: impl Fn() -> bool) {
    for _ in 0..3000 {
        if cond() {
            return;
        }
        tokio::time::sleep(Duration::from_millis(10)).await;
    }
    panic!("timed out waiting for {what}");
}

fn asleep(task: &str) -> bool {
    let stat = std::fs::read_to_string(Path::new(task).join("stat")).unwrap();
    stat.rsplit_once(") ")
        .is_some_and(|(_, rest)| rest.starts_with('S'))
}

fn context_switches(task: &str) -> u64 {
    let status = std::fs::read_to_string(Path::new(task).join("status")).unwrap();
    status
        .lines()
        .filter_map(|l| l.strip_prefix("voluntary_ctxt_switches:"))
        .map(|v| v.trim().parse::<u64>().unwrap())
        .sum()
}

#[tokio::test]
async fn open_folders_share_one_inotify_instance_and_an_idle_thread() {
    let home = tempfile::tempdir().unwrap();
    let paths = AppPaths::new(home.path());
    let media = home.path().join("media");
    std::fs::create_dir(&media).unwrap();
    let registry = LocationRegistry::with_media_root(paths.clone(), media);
    let core = Core::open_with(paths, registry).await.unwrap();
    let before = inotify_instances();

    let folders: Vec<_> = (0..20).map(|_| tempfile::tempdir().unwrap()).collect();
    let mut watches: Vec<_> = folders
        .iter()
        .map(|d| core.watch_dir(d.path()).unwrap())
        .collect();
    let (_again, _rx) = core.watch_dir(folders[0].path()).unwrap();
    assert_eq!(inotify_instances(), before + 1, "one instance for all folders");
    // Volume watcher plus folder watcher. std names a thread from inside it: until first
    // scheduled, it carries the spawner's name.
    eventually("watch threads named", || watch_threads().len() >= 2).await;
    let threads = watch_threads();
    assert_eq!(threads.len(), 2, "{threads:?}");

    // One inotify event: a file write is several, and a stall between them wakes the worker late.
    std::fs::create_dir(folders[7].path().join("x")).unwrap();
    let got = tokio::time::timeout(Duration::from_secs(30), watches[7].1.recv())
        .await
        .unwrap();
    assert_eq!(got, Some(WatchEvent::Changed(folders[7].path().to_path_buf())));

    // Thread that sent the event may not have blocked again yet.
    eventually("watch threads asleep", || threads.iter().all(|t| asleep(t))).await;
    let switches: Vec<u64> = threads.iter().map(|t| context_switches(t)).collect();
    tokio::time::sleep(Duration::from_millis(600)).await;
    let after: Vec<u64> = threads.iter().map(|t| context_switches(t)).collect();
    assert_eq!(switches, after, "idle watch threads must not wake");
    assert_eq!(watch_threads(), threads, "no watch thread started late");
}
