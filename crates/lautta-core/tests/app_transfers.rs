// SPDX-License-Identifier: LGPL-2.1-or-later
//! The transfers area on a temporary home folder: the grouped list with
//! questions and history, and restore at start (XFR-11).

use lautta_core::app::{Core, Started};
use lautta_core::app_transfers::{ProgressBook, TransferGroup};
use lautta_core::locations::LocationRegistry;
use lautta_core::ops::OperationKind;
use lautta_core::paths::AppPaths;
use lautta_core::transfer::{TransferState, WaitReason};
use lautta_core::{ErrorKind, Uri};
use std::path::Path;
use std::sync::Arc;

struct Home {
    _dir: tempfile::TempDir,
    root: std::path::PathBuf,
    paths: AppPaths,
    core: Arc<Core>,
}

async fn open(paths: &AppPaths, root: &Path) -> Arc<Core> {
    let registry = LocationRegistry::with_media_root(paths.clone(), root.join("media"));
    Core::open_with(paths.clone(), registry).await.unwrap()
}

async fn home() -> Home {
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path().to_path_buf();
    for d in ["Documents", "Downloads"] {
        std::fs::create_dir_all(root.join(d)).unwrap();
    }
    let paths = AppPaths::new(&root);
    let core = open(&paths, &root).await;
    Home {
        _dir: dir,
        root,
        paths,
        core,
    }
}

fn uri(s: &str) -> Uri {
    Uri::parse(s).unwrap()
}

fn write(path: &Path, data: &[u8]) {
    std::fs::create_dir_all(path.parent().unwrap()).unwrap();
    std::fs::write(path, data).unwrap();
}

/// Queues a copy of Documents/a.txt into Downloads, which already holds a
/// different a.txt: the transfer stops with a question.
async fn conflicting_copy(h: &Home) -> i64 {
    write(&h.root.join("Documents/a.txt"), b"new content");
    write(&h.root.join("Downloads/a.txt"), b"old");
    let started = h
        .core
        .copy_or_move(
            OperationKind::Copy,
            vec![uri("lautta://user-documents/a.txt")],
            uri("lautta://user-downloads/"),
        )
        .await
        .unwrap();
    let Started::Transfer(id) = started else {
        panic!("small copies start at once")
    };
    h.core
        .engine
        .wait_for(id, |s| s.state == TransferState::Waiting(WaitReason::Question))
        .await
        .unwrap();
    id
}

#[tokio::test]
async fn the_list_groups_questions_and_history() {
    let h = home().await;
    let id = conflicting_copy(&h).await;
    let book = ProgressBook::default();

    let rows = h.core.transfer_rows(&book);
    assert_eq!(rows.len(), 1);
    assert_eq!(rows[0].group, TransferGroup::Waiting);
    assert_eq!(rows[0].questions, 1);
    assert_eq!(rows[0].direction.name(), "local");
    assert_eq!(rows[0].kind_name(), "copy");
    assert_eq!(rows[0].summary.title, "a.txt");
    assert!(
        rows[0].dest_address.starts_with("file://"),
        "{}",
        rows[0].dest_address
    );

    let questions = h.core.question_list(id).unwrap();
    assert_eq!(questions.len(), 1);
    assert_eq!(questions[0]["name"], "a.txt");
    assert_eq!(questions[0]["src_size"], 11);
    assert_eq!(questions[0]["dst_size"], 3);

    let bad = h.core.answer_question(id, 0, "obliterate", false).unwrap_err();
    assert_eq!(bad.kind, ErrorKind::InvalidArgument);
    h.core.answer_question(id, 0, "Replace", false).unwrap();
    let done = h
        .core
        .engine
        .wait_for(id, |s| s.state.is_finished())
        .await
        .unwrap();
    assert_eq!(done.state, TransferState::Completed);
    assert_eq!(
        std::fs::read(h.root.join("Downloads/a.txt")).unwrap(),
        b"new content"
    );

    let rows = h.core.transfer_rows(&book);
    assert_eq!(rows[0].group, TransferGroup::History);
    assert_eq!(rows[0].questions, 0);
    assert_eq!(h.core.clear_history(), 1);
    assert!(h.core.transfer_rows(&book).is_empty());
    assert_eq!(h.core.clear_history(), 0);
}

#[tokio::test]
async fn unfinished_transfers_come_back_paused_or_resumed() {
    let h = home().await;
    let id = conflicting_copy(&h).await;
    h.core.engine.flush().await;
    assert_eq!(h.core.pending_at_close(), 1, "XFR-6 counts unfinished work");

    let second = open(&h.paths, &h.root).await;
    let report = second.start_transfers(false).await.unwrap();
    assert_eq!((report.restored, report.paused), (1, 1));
    let rows = second.transfer_rows(&ProgressBook::default());
    assert_eq!(rows[0].group, TransferGroup::Paused);
    assert_eq!(rows[0].summary.id, id);

    let third = open(&h.paths, &h.root).await;
    let report = third.start_transfers(true).await.unwrap();
    assert_eq!((report.restored, report.paused), (1, 0));
    third
        .engine
        .wait_for(id, |s| s.state == TransferState::Waiting(WaitReason::Question))
        .await
        .unwrap();
}

#[tokio::test]
async fn standalone_mode_counts_as_bridge_reachable() {
    let h = home().await;
    assert!(h.core.bridge.is_none());
    assert!(h.core.bridge_reachable(), "no bridge: nothing to wait for");
}
