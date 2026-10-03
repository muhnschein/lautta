// SPDX-License-Identifier: LGPL-2.1-or-later
//! The transfers area on a temporary home folder: the grouped list with
//! questions and history, restore at start (XFR-11), working copies and
//! their write-back (EDT-1..3).

use lautta_core::app::{Core, Started};
use lautta_core::app_transfers::{ProgressBook, TransferGroup, WriteBack};
use lautta_core::locations::LocationRegistry;
use lautta_core::ops::OperationKind;
use lautta_core::paths::AppPaths;
use lautta_core::transfer::{TransferState, WaitReason};
use lautta_core::workcopy::{EditConflictChoice, Resolution};
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
    assert_eq!(questions[0]["dstSize"], 3);

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
async fn working_copies_upload_conflict_and_resolve() {
    let h = home().await;
    let remote_file = h.root.join("Documents/r.txt");
    write(&remote_file, b"one");
    let remote = uri("lautta://user-documents/r.txt");
    let provider = h.core.provider("user-documents").unwrap();
    let copy = h
        .core
        .working_copies
        .open_for_edit(provider.as_ref(), &remote)
        .await
        .unwrap();

    let edited = h.core.edited_files().await.unwrap();
    assert_eq!(edited.len(), 1);
    assert!(!edited[0].dirty, "nothing changed yet");
    assert_eq!(h.core.working_copy_name(&edited[0].copy), "r.txt");
    assert_eq!(h.core.write_back(copy.id).await.unwrap(), WriteBack::Unchanged);

    std::fs::write(&copy.local_path, b"two two").unwrap();
    assert!(h.core.edited_files().await.unwrap()[0].dirty, "upload pending");
    match h.core.write_back(copy.id).await.unwrap() {
        WriteBack::Uploaded(c) => assert_eq!(c.id, copy.id),
        other => panic!("expected an upload, got {other:?}"),
    }
    assert_eq!(std::fs::read(&remote_file).unwrap(), b"two two");
    assert!(!h.core.edited_files().await.unwrap()[0].dirty);
    assert!(h.core.edit_conflict(copy.id).await.unwrap().is_none());

    std::fs::write(&copy.local_path, b"mine, longer").unwrap();
    std::fs::write(&remote_file, b"theirs").unwrap();
    let WriteBack::Conflict(conflict) = h.core.write_back(copy.id).await.unwrap() else {
        panic!("the remote changed meanwhile")
    };
    assert_eq!(conflict.remote_size, Some(6));
    assert_eq!(conflict.local_size, 12);
    assert_eq!(
        h.core.edit_conflict(copy.id).await.unwrap().map(|c| c.local_size),
        Some(12)
    );
    assert_eq!(
        std::fs::read(&remote_file).unwrap(),
        b"theirs",
        "never overwritten unasked"
    );

    let res = h
        .core
        .resolve_edit_conflict(copy.id, EditConflictChoice::SaveMineAsCopy)
        .await
        .unwrap();
    let Resolution::SavedCopy(saved) = res else {
        panic!("saved as a copy")
    };
    let saved_path = h.core.locations.to_local_path(&saved).unwrap();
    assert_eq!(std::fs::read(saved_path).unwrap(), b"mine, longer");
    assert_eq!(std::fs::read(&remote_file).unwrap(), b"theirs");
    assert!(
        h.core.edited_files().await.unwrap().is_empty(),
        "the copy is gone"
    );
}

#[tokio::test]
async fn start_reports_edits_changed_while_closed() {
    let h = home().await;
    write(&h.root.join("Documents/r.txt"), b"one");
    let provider = h.core.provider("user-documents").unwrap();
    let copy = h
        .core
        .working_copies
        .open_for_edit(provider.as_ref(), &uri("lautta://user-documents/r.txt"))
        .await
        .unwrap();
    std::fs::write(&copy.local_path, b"edited while closed").unwrap();

    let second = open(&h.paths, &h.root).await;
    let report = second.start_transfers(true).await.unwrap();
    assert_eq!(report.restored, 0);
    assert_eq!(report.dirty_edits.len(), 1);
    assert_eq!(report.dirty_edits[0].id, copy.id);
}

#[tokio::test]
async fn unknown_working_copies_are_not_found() {
    let h = home().await;
    assert_eq!(h.core.write_back(99).await.unwrap_err().kind, ErrorKind::NotFound);
    assert_eq!(
        h.core.edit_conflict(99).await.unwrap_err().kind,
        ErrorKind::NotFound
    );
    assert_eq!(
        h.core
            .resolve_edit_conflict(99, EditConflictChoice::DiscardMine)
            .await
            .unwrap_err()
            .kind,
        ErrorKind::NotFound
    );
}

#[tokio::test]
async fn standalone_mode_counts_as_bridge_reachable() {
    let h = home().await;
    assert!(h.core.bridge.is_none());
    assert!(h.core.bridge_reachable(), "no bridge: nothing to wait for");
}
