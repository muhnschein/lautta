// SPDX-License-Identifier: LGPL-2.1-or-later
//! Transfers end to end across two in-memory providers: copy, move, retries,
//! resume, conflict questions, cancel, waiting states and persistence
//! (SPEC §11).

use lautta_core::db::Db;
use lautta_core::entry::{cap, Capabilities, Kind};
use lautta_core::error::{Error, ErrorKind};
use lautta_core::ops::{ConflictChoice, OperationKind, Plan, PlanItem, PlanTotals};
use lautta_core::provider::memory::MemoryProvider;
use lautta_core::provider::StaticResolver;
use lautta_core::transfer::exec::temp_name_for;
use lautta_core::transfer::{
    Engine, EngineConfig, EngineDeps, ItemState, ManualClock, TransferEvent, TransferId, TransferOptions,
    TransferState, TransferSummary, Trasher, WaitReason,
};
use lautta_core::{Uri, VPath};
use std::sync::{Arc, Mutex};
use std::time::Duration;

const LOCAL: &str = "user-documents";
const REMOTE: &str = "nv-srv";

fn vp(s: &str) -> VPath {
    VPath::parse(s.as_bytes()).unwrap()
}

fn item(sl: &str, src: &str, dl: &str, dst: &str, kind: Kind, size: u64) -> PlanItem {
    PlanItem {
        src: Uri::new(sl, vp(src)),
        dst: Uri::new(dl, vp(dst)),
        kind,
        size: Some(size),
        mtime_ms: Some(1_000),
        mode: None,
        link_target: None,
        conflict: None,
        proposed_name: None,
        resolution: None,
    }
}

fn plan(kind: OperationKind, dest: &str, items: Vec<PlanItem>) -> Plan {
    Plan {
        kind,
        destination: Uri::root(dest),
        items,
        totals: PlanTotals::default(),
        needs_summary: false,
    }
}

struct Rig {
    engine: Engine,
    db: Db,
    local: MemoryProvider,
    remote: MemoryProvider,
    clock: Arc<ManualClock>,
}

fn config() -> EngineConfig {
    EngineConfig {
        backoff_base: Duration::from_millis(1),
        ..EngineConfig::default()
    }
}

fn rig() -> Rig {
    rig_on(
        Db::open_in_memory().unwrap(),
        MemoryProvider::default(),
        MemoryProvider::default(),
    )
}

fn rig_on(db: Db, local: MemoryProvider, remote: MemoryProvider) -> Rig {
    rig_with(db, local, remote, config(), None)
}

fn rig_with(
    db: Db,
    local: MemoryProvider,
    remote: MemoryProvider,
    cfg: EngineConfig,
    trasher: Option<Arc<dyn Trasher>>,
) -> Rig {
    let resolver = StaticResolver::default()
        .with(LOCAL, Arc::new(local.clone()))
        .with(REMOTE, Arc::new(remote.clone()));
    let clock = ManualClock::new(1_700_000_000_000);
    let mut deps = EngineDeps::new(db.clone(), Arc::new(resolver));
    deps.clock = clock.clone();
    deps.trasher = trasher;
    Rig {
        engine: Engine::new(deps, cfg),
        db,
        local,
        remote,
        clock,
    }
}

async fn settled(e: &Engine, id: TransferId) -> TransferSummary {
    tokio::time::timeout(Duration::from_secs(10), e.wait_for(id, |s| s.state.is_finished()))
        .await
        .expect("transfer finishes")
        .unwrap()
}

async fn in_state(e: &Engine, id: TransferId, state: TransferState) -> TransferSummary {
    tokio::time::timeout(Duration::from_secs(10), e.wait_for(id, |s| s.state == state))
        .await
        .unwrap_or_else(|_| panic!("transfer never reached {state:?}: {:?}", e.get(id)))
        .unwrap()
}

fn drain(rx: &mut tokio::sync::broadcast::Receiver<TransferEvent>) -> Vec<TransferEvent> {
    let mut out = Vec::new();
    while let Ok(e) = rx.try_recv() {
        out.push(e);
    }
    out
}

fn copy_one(name: &str) -> Plan {
    plan(
        OperationKind::Copy,
        REMOTE,
        vec![item(LOCAL, name, REMOTE, name, Kind::File, 5)],
    )
}

#[tokio::test]
async fn copies_a_tree_between_locations() {
    let r = rig();
    r.local.add_file("Photos/a.jpg", b"aaaaa", 1_000);
    r.local.add_file("Photos/sub/b.jpg", b"bbbbbbbb", 2_000);
    r.local.add_symlink("Photos/link", "a.jpg");
    let items = vec![
        item(LOCAL, "Photos", REMOTE, "Photos", Kind::Dir, 0),
        item(LOCAL, "Photos/a.jpg", REMOTE, "Photos/a.jpg", Kind::File, 5),
        item(LOCAL, "Photos/sub", REMOTE, "Photos/sub", Kind::Dir, 0),
        item(
            LOCAL,
            "Photos/sub/b.jpg",
            REMOTE,
            "Photos/sub/b.jpg",
            Kind::File,
            8,
        ),
        {
            let mut l = item(LOCAL, "Photos/link", REMOTE, "Photos/link", Kind::Symlink, 0);
            l.link_target = Some(b"a.jpg".to_vec());
            l
        },
    ];
    let mut rx = r.engine.subscribe();
    let id = r
        .engine
        .add(
            plan(OperationKind::Copy, REMOTE, items),
            "Copy Photos",
            TransferOptions::default(),
        )
        .await
        .unwrap();
    assert!(r.engine.is_busy());
    let s = settled(&r.engine, id).await;
    assert_eq!(s.state, TransferState::Completed);
    assert_eq!((s.items_total, s.items_done, s.items_failed), (5, 5, 0));
    assert_eq!((s.bytes_total, s.bytes_done), (13, 13));
    assert_eq!(s.title, "Copy Photos");
    assert!(s.finished_ms.is_some());
    assert_eq!(r.remote.read_file("Photos/a.jpg").unwrap(), b"aaaaa");
    assert_eq!(r.remote.read_file("Photos/sub/b.jpg").unwrap(), b"bbbbbbbb");
    assert!(r.remote.exists("Photos/link"));
    assert!(r.local.exists("Photos/a.jpg"), "a copy keeps the source");
    assert!(
        !r.remote.paths().iter().any(|p| p.contains(".lautta-")),
        "no temporary names left: {:?}",
        r.remote.paths()
    );
    assert!(!r.engine.is_busy());
    let events = drain(&mut rx);
    assert!(events.contains(&TransferEvent::Added(id)));
    assert!(events.contains(&TransferEvent::Finished(id)));
    let busy: Vec<bool> = events
        .iter()
        .filter_map(|e| match e {
            TransferEvent::Busy(b) => Some(*b),
            _ => None,
        })
        .collect();
    assert_eq!(busy, vec![true, false], "KeepAlive follows activity");
}

#[tokio::test]
async fn move_across_locations_verifies_then_removes_sources_and_folders() {
    let r = rig();
    r.local.add_file("d/f1", b"11111", 1);
    r.local.add_file("d/e/f2", b"22", 1);
    let items = vec![
        item(LOCAL, "d", REMOTE, "d", Kind::Dir, 0),
        item(LOCAL, "d/f1", REMOTE, "d/f1", Kind::File, 5),
        item(LOCAL, "d/e", REMOTE, "d/e", Kind::Dir, 0),
        item(LOCAL, "d/e/f2", REMOTE, "d/e/f2", Kind::File, 2),
    ];
    let id = r
        .engine
        .add(
            plan(OperationKind::Move, REMOTE, items),
            "",
            TransferOptions::default(),
        )
        .await
        .unwrap();
    let s = settled(&r.engine, id).await;
    assert_eq!(s.state, TransferState::Completed, "{s:?}");
    assert_eq!(r.remote.read_file("d/e/f2").unwrap(), b"22");
    assert!(
        r.local.paths().is_empty(),
        "sources and folders gone: {:?}",
        r.local.paths()
    );
    let sums = r
        .local
        .calls()
        .iter()
        .filter(|c| c.starts_with("checksum"))
        .count();
    assert_eq!(sums, 2, "every moved file was verified first");
}

#[tokio::test]
async fn a_move_keeps_a_folder_that_still_holds_something() {
    let r = rig();
    r.local.add_file("d/f1", b"11111", 1);
    r.local.add_file("d/other", b"x", 1);
    let items = vec![
        item(LOCAL, "d", REMOTE, "d", Kind::Dir, 0),
        item(LOCAL, "d/f1", REMOTE, "d/f1", Kind::File, 5),
    ];
    let id = r
        .engine
        .add(
            plan(OperationKind::Move, REMOTE, items),
            "",
            TransferOptions::default(),
        )
        .await
        .unwrap();
    assert_eq!(settled(&r.engine, id).await.state, TransferState::Completed);
    assert_eq!(r.local.paths(), vec!["d", "d/other"]);
}

#[tokio::test]
async fn same_location_folder_move_is_a_single_rename() {
    let r = rig();
    r.local.add_file("d/f1", b"11111", 1);
    r.local.add_dir("target");
    let items = vec![
        item(LOCAL, "d", LOCAL, "target/d", Kind::Dir, 0),
        item(LOCAL, "d/f1", LOCAL, "target/d/f1", Kind::File, 5),
    ];
    let id = r
        .engine
        .add(
            plan(OperationKind::Move, LOCAL, items),
            "",
            TransferOptions::default(),
        )
        .await
        .unwrap();
    let s = settled(&r.engine, id).await;
    assert_eq!((s.state, s.items_done), (TransferState::Completed, 2));
    assert_eq!(r.local.read_file("target/d/f1").unwrap(), b"11111");
    assert!(!r.local.exists("d"));
    let calls = r.local.calls();
    assert!(!calls.iter().any(|c| c.starts_with("upload_from")), "{calls:?}");
}

#[tokio::test(start_paused = true)]
async fn transient_errors_back_off_one_two_four_seconds() {
    let cfg = EngineConfig::default();
    let r = rig_with(
        Db::open_in_memory().unwrap(),
        MemoryProvider::default(),
        MemoryProvider::default(),
        cfg,
        None,
    );
    r.local.add_file("f", b"hello", 1);
    for _ in 0..3 {
        r.local.fail_next("stat", "f", Error::kind(ErrorKind::TimedOut));
    }
    let start = tokio::time::Instant::now();
    let id = r
        .engine
        .add(copy_one("f"), "", TransferOptions::default())
        .await
        .unwrap();
    let s = settled(&r.engine, id).await;
    assert_eq!(s.state, TransferState::Completed);
    let waited = start.elapsed();
    assert!(
        waited >= Duration::from_secs(7) && waited < Duration::from_secs(8),
        "1 + 2 + 4 s of backoff, got {waited:?}"
    );
    assert_eq!(r.remote.read_file("f").unwrap(), b"hello");
}

#[tokio::test]
async fn a_failing_item_does_not_stop_the_rest_and_can_be_retried() {
    let r = rig();
    r.local.add_file("good", b"12345", 1);
    r.local.add_file("bad", b"12345", 1);
    for _ in 0..4 {
        r.local.fail_next("stat", "bad", Error::kind(ErrorKind::TimedOut));
    }
    let items = vec![
        item(LOCAL, "bad", REMOTE, "bad", Kind::File, 5),
        item(LOCAL, "good", REMOTE, "good", Kind::File, 5),
    ];
    let id = r
        .engine
        .add(
            plan(OperationKind::Copy, REMOTE, items),
            "",
            TransferOptions::default(),
        )
        .await
        .unwrap();
    let s = settled(&r.engine, id).await;
    assert_eq!(s.state, TransferState::Failed);
    assert_eq!((s.items_done, s.items_failed), (1, 1), "completed with 1 failure");
    assert_eq!(s.error.as_deref(), Some("1 items failed"));
    assert!(r.remote.exists("good") && !r.remote.exists("bad"));
    let items = r.engine.items(id).unwrap();
    assert_eq!(items[0].state, ItemState::Failed);
    assert!(items[0].error.as_deref().unwrap().contains("TimedOut"));

    r.engine.retry_failed(id).unwrap();
    let s = settled(&r.engine, id).await;
    assert_eq!(s.state, TransferState::Completed, "{s:?}");
    assert_eq!((s.items_done, s.items_failed), (2, 0));
    assert_eq!(s.error, None);
    let stats_of_good = r.local.calls().iter().filter(|c| *c == "stat good").count();
    assert_eq!(stats_of_good, 1, "only the failed item ran again");
    assert_eq!(r.remote.read_file("bad").unwrap(), b"12345");
}

#[tokio::test]
async fn permanent_errors_fail_at_once_and_remove_the_partial_file() {
    let r = rig();
    r.local.add_file("f", b"12345", 1);
    r.local
        .fail_next("download_into", "f", Error::kind(ErrorKind::PermissionDenied));
    let id = r
        .engine
        .add(copy_one("f"), "", TransferOptions::default())
        .await
        .unwrap();
    let s = settled(&r.engine, id).await;
    assert_eq!(s.state, TransferState::Failed);
    let downloads = r
        .local
        .calls()
        .iter()
        .filter(|c| c.starts_with("download_into"))
        .count();
    assert_eq!(downloads, 1, "not retried");
    assert!(r.remote.paths().is_empty());
}

#[tokio::test]
async fn a_lost_bridge_makes_the_transfer_wait_and_resume_by_itself() {
    let r = rig();
    r.local.add_file("f", b"hello", 1);
    for _ in 0..4 {
        r.local
            .fail_next("stat", "f", Error::kind(ErrorKind::ConnectionLost));
    }
    let id = r
        .engine
        .add(copy_one("f"), "", TransferOptions::default())
        .await
        .unwrap();
    let s = in_state(&r.engine, id, TransferState::Waiting(WaitReason::Bridge)).await;
    assert_eq!(s.items_failed, 0, "waiting is not failing");
    assert!(!r.remote.exists("f"));
    r.engine.bridge_available(false);
    r.engine.bridge_available(true);
    let s = settled(&r.engine, id).await;
    assert_eq!(s.state, TransferState::Completed);
    assert_eq!(r.remote.read_file("f").unwrap(), b"hello");
}

#[tokio::test]
async fn transfers_added_while_the_bridge_is_down_wait_for_it() {
    let r = rig();
    r.local.add_file("f", b"hello", 1);
    r.engine.bridge_available(false);
    let id = r
        .engine
        .add(copy_one("f"), "", TransferOptions::default())
        .await
        .unwrap();
    assert_eq!(
        r.engine.get(id).unwrap().state,
        TransferState::Waiting(WaitReason::Bridge)
    );
    assert!(!r.engine.is_busy(), "nothing to keep the device awake for");
    r.local.add_file("local-only", b"x", 1);
    let local_only = r
        .engine
        .add(
            plan(
                OperationKind::Copy,
                LOCAL,
                vec![item(LOCAL, "local-only", LOCAL, "copy", Kind::File, 1)],
            ),
            "",
            TransferOptions::default(),
        )
        .await
        .unwrap();
    assert_eq!(
        settled(&r.engine, local_only).await.state,
        TransferState::Completed
    );
    r.engine.bridge_available(true);
    assert_eq!(settled(&r.engine, id).await.state, TransferState::Completed);
}

#[tokio::test]
async fn network_loss_waits_with_its_own_reason() {
    let r = rig();
    r.local.add_file("f", b"hello", 1);
    r.engine.network_available(false);
    let id = r
        .engine
        .add(copy_one("f"), "", TransferOptions::default())
        .await
        .unwrap();
    assert_eq!(
        r.engine.get(id).unwrap().state,
        TransferState::Waiting(WaitReason::Network)
    );
    r.engine.bridge_available(false);
    assert_eq!(
        r.engine.get(id).unwrap().state,
        TransferState::Waiting(WaitReason::Bridge),
        "the bridge reason takes over"
    );
    r.engine.bridge_available(true);
    assert_eq!(
        r.engine.get(id).unwrap().state,
        TransferState::Waiting(WaitReason::Network)
    );
    r.engine.network_available(true);
    assert_eq!(settled(&r.engine, id).await.state, TransferState::Completed);
}

#[tokio::test]
async fn a_vanished_volume_waits_until_it_is_back() {
    let r = rig();
    r.local.add_file("f", b"hello", 1);
    let p = plan(
        OperationKind::Copy,
        LOCAL,
        vec![item(LOCAL, "f", LOCAL, "g", Kind::File, 5)],
    );
    r.engine.volume_present(LOCAL, false);
    let id = r.engine.add(p, "", TransferOptions::default()).await.unwrap();
    assert_eq!(
        r.engine.get(id).unwrap().state,
        TransferState::Waiting(WaitReason::Volume)
    );
    assert!(!r.local.exists("g"));
    r.engine.volume_present(LOCAL, true);
    assert_eq!(settled(&r.engine, id).await.state, TransferState::Completed);
    assert_eq!(r.local.read_file("g").unwrap(), b"hello");
}

#[tokio::test]
async fn resumes_a_partial_destination_file_when_capable() {
    // The partial deliberately does not match the source: bytes already
    // committed must not be written again (XFR-12).
    let r = rig();
    r.local.add_file("big", b"0123456789", 1);
    let temp = String::from_utf8(temp_name_for(b"big", 1, 0)).unwrap();
    r.remote.add_file(&temp, b"XXXX", 1);
    let p = plan(
        OperationKind::Copy,
        REMOTE,
        vec![item(LOCAL, "big", REMOTE, "big", Kind::File, 10)],
    );
    let id = r.engine.add(p, "", TransferOptions::default()).await.unwrap();
    assert_eq!(id, 1);
    assert_eq!(settled(&r.engine, id).await.state, TransferState::Completed);
    assert_eq!(r.remote.read_file("big").unwrap(), b"XXXX456789");
    assert!(!r.remote.exists(&temp));
}

#[tokio::test]
async fn restarts_the_file_when_the_destination_cannot_resume() {
    let remote = MemoryProvider::new(Capabilities::with(&[cap::WRITE, cap::SET_MTIME]));
    let r = rig_on(Db::open_in_memory().unwrap(), MemoryProvider::default(), remote);
    r.local.add_file("big", b"0123456789", 1);
    let temp = String::from_utf8(temp_name_for(b"big", 1, 0)).unwrap();
    r.remote.add_file(&temp, b"XXXX", 1);
    let p = plan(
        OperationKind::Copy,
        REMOTE,
        vec![item(LOCAL, "big", REMOTE, "big", Kind::File, 10)],
    );
    let id = r.engine.add(p, "", TransferOptions::default()).await.unwrap();
    assert_eq!(settled(&r.engine, id).await.state, TransferState::Completed);
    assert_eq!(r.remote.read_file("big").unwrap(), b"0123456789");
}

#[tokio::test]
async fn an_existing_destination_asks_and_replace_answers_it() {
    let r = rig();
    r.local.add_file("f", b"new!!", 1);
    r.remote.add_file("f", b"old", 1);
    let mut rx = r.engine.subscribe();
    let id = r
        .engine
        .add(copy_one("f"), "", TransferOptions::default())
        .await
        .unwrap();
    in_state(&r.engine, id, TransferState::Waiting(WaitReason::Question)).await;
    assert_eq!(
        r.remote.read_file("f").unwrap(),
        b"old",
        "nothing overwritten unasked"
    );
    assert!(drain(&mut rx).contains(&TransferEvent::NeedsAnswer { id, item: 0 }));
    let qs = r.engine.questions(id).unwrap();
    assert_eq!(qs.len(), 1);
    assert_eq!(qs[0].0, 0);
    assert!(qs[0].1.choices.contains(&ConflictChoice::Replace));
    assert_ne!(qs[0].1.choices[0], ConflictChoice::Replace, "never the default");

    let bad = r.engine.answer(id, 0, ConflictChoice::Merge, false).unwrap_err();
    assert_eq!(bad.kind, ErrorKind::InvalidArgument);
    assert_eq!(
        r.engine
            .answer(id, 9, ConflictChoice::Skip, false)
            .unwrap_err()
            .kind,
        ErrorKind::InvalidArgument
    );

    r.engine.answer(id, 0, ConflictChoice::Replace, false).unwrap();
    let s = settled(&r.engine, id).await;
    assert_eq!(s.state, TransferState::Completed);
    assert_eq!(r.remote.read_file("f").unwrap(), b"new!!");
}

#[tokio::test]
async fn keep_both_and_skip() {
    let r = rig();
    r.local.add_file("f", b"new!!", 1);
    r.remote.add_file("f", b"old", 1);
    let id = r
        .engine
        .add(copy_one("f"), "", TransferOptions::default())
        .await
        .unwrap();
    in_state(&r.engine, id, TransferState::Waiting(WaitReason::Question)).await;
    r.engine.answer(id, 0, ConflictChoice::KeepBoth, false).unwrap();
    assert_eq!(settled(&r.engine, id).await.state, TransferState::Completed);
    assert_eq!(r.remote.read_file("f").unwrap(), b"old");
    assert_eq!(r.remote.read_file("f 2").unwrap(), b"new!!");

    let id = r
        .engine
        .add(copy_one("f"), "", TransferOptions::default())
        .await
        .unwrap();
    in_state(&r.engine, id, TransferState::Waiting(WaitReason::Question)).await;
    r.engine.answer(id, 0, ConflictChoice::Skip, false).unwrap();
    let s = settled(&r.engine, id).await;
    assert_eq!(
        (s.state, s.items_done, s.bytes_total),
        (TransferState::Completed, 1, 0)
    );
    assert_eq!(r.remote.read_file("f").unwrap(), b"old");
}

#[tokio::test]
async fn apply_to_all_answers_the_other_open_and_future_questions() {
    let r = rig();
    for n in ["a", "b", "c"] {
        r.local.add_file(n, b"new!!", 1);
        r.remote.add_file(n, b"old", 1);
    }
    let items = ["a", "b", "c"]
        .iter()
        .map(|n| item(LOCAL, n, REMOTE, n, Kind::File, 5))
        .collect();
    let id = r
        .engine
        .add(
            plan(OperationKind::Copy, REMOTE, items),
            "",
            TransferOptions::default(),
        )
        .await
        .unwrap();
    in_state(&r.engine, id, TransferState::Waiting(WaitReason::Question)).await;
    // Wait for the in-flight items to report their questions as well.
    tokio::time::timeout(Duration::from_secs(10), async {
        while r.engine.questions(id).unwrap().len() < 2 {
            tokio::task::yield_now().await;
        }
    })
    .await
    .unwrap();
    let first = r.engine.questions(id).unwrap()[0].0;
    r.engine.answer(id, first, ConflictChoice::Replace, true).unwrap();
    let s = settled(&r.engine, id).await;
    assert_eq!(s.state, TransferState::Completed, "{s:?}");
    for n in ["a", "b", "c"] {
        assert_eq!(r.remote.read_file(n).unwrap(), b"new!!", "{n}");
    }
    assert_eq!(s.items_done, 3);
}

#[tokio::test]
async fn answering_skip_on_a_folder_skips_what_is_inside() {
    let r = rig();
    r.local.add_file("d/f", b"12345", 1);
    r.remote.add_dir("d");
    let items = vec![
        item(LOCAL, "d", REMOTE, "d", Kind::Dir, 0),
        item(LOCAL, "d/f", REMOTE, "d/f", Kind::File, 5),
    ];
    let id = r
        .engine
        .add(
            plan(OperationKind::Copy, REMOTE, items),
            "",
            TransferOptions::default(),
        )
        .await
        .unwrap();
    in_state(&r.engine, id, TransferState::Waiting(WaitReason::Question)).await;
    let q = &r.engine.questions(id).unwrap()[0].1;
    assert!(q.choices.contains(&ConflictChoice::Merge));
    r.engine.answer(id, 0, ConflictChoice::Skip, false).unwrap();
    let s = settled(&r.engine, id).await;
    assert_eq!((s.state, s.items_done), (TransferState::Completed, 2));
    assert!(!r.remote.exists("d/f"));
}

#[tokio::test]
async fn keep_both_on_a_folder_moves_the_children_along() {
    let r = rig();
    r.local.add_file("d/f", b"12345", 1);
    r.remote.add_dir("d");
    let items = vec![
        item(LOCAL, "d", REMOTE, "d", Kind::Dir, 0),
        item(LOCAL, "d/f", REMOTE, "d/f", Kind::File, 5),
    ];
    let id = r
        .engine
        .add(
            plan(OperationKind::Copy, REMOTE, items),
            "",
            TransferOptions::default(),
        )
        .await
        .unwrap();
    in_state(&r.engine, id, TransferState::Waiting(WaitReason::Question)).await;
    r.engine.answer(id, 0, ConflictChoice::KeepBoth, false).unwrap();
    assert_eq!(settled(&r.engine, id).await.state, TransferState::Completed);
    assert_eq!(r.remote.read_file("d 2/f").unwrap(), b"12345");
    assert!(!r.remote.exists("d/f"));
}

#[tokio::test]
async fn merge_answer_for_folders() {
    let r = rig();
    r.local.add_file("d/f", b"12345", 1);
    r.remote.add_file("d/keep", b"k", 1);
    let items = vec![
        item(LOCAL, "d", REMOTE, "d", Kind::Dir, 0),
        item(LOCAL, "d/f", REMOTE, "d/f", Kind::File, 5),
    ];
    let id = r
        .engine
        .add(
            plan(OperationKind::Copy, REMOTE, items),
            "",
            TransferOptions::default(),
        )
        .await
        .unwrap();
    in_state(&r.engine, id, TransferState::Waiting(WaitReason::Question)).await;
    r.engine.answer(id, 0, ConflictChoice::Merge, false).unwrap();
    assert_eq!(settled(&r.engine, id).await.state, TransferState::Completed);
    assert_eq!(r.remote.read_file("d/f").unwrap(), b"12345");
    assert_eq!(r.remote.read_file("d/keep").unwrap(), b"k");
}

#[tokio::test]
async fn cancel_during_backoff_stops_the_transfer_and_removes_the_partial_file() {
    let cfg = EngineConfig {
        backoff_base: Duration::from_millis(300),
        ..EngineConfig::default()
    };
    let r = rig_with(
        Db::open_in_memory().unwrap(),
        MemoryProvider::default(),
        MemoryProvider::default(),
        cfg,
        None,
    );
    r.local.add_file("f", b"hello", 1);
    let temp = String::from_utf8(temp_name_for(b"f", 1, 0)).unwrap();
    r.remote.add_file(&temp, b"he", 1);
    r.local.fail_next("stat", "f", Error::kind(ErrorKind::TimedOut));
    let id = r
        .engine
        .add(copy_one("f"), "", TransferOptions::default())
        .await
        .unwrap();
    tokio::time::timeout(Duration::from_secs(5), async {
        while r.engine.items(id).unwrap()[0].attempts == 0 {
            tokio::task::yield_now().await;
        }
    })
    .await
    .unwrap();
    r.engine.cancel(id).unwrap();
    let s = r.engine.get(id).unwrap();
    assert_eq!(s.state, TransferState::Canceled);
    assert!(s.finished_ms.is_some());
    tokio::time::sleep(Duration::from_millis(500)).await;
    assert!(!r.remote.exists("f"), "the retry never ran");
    assert!(!r.remote.exists(&temp), "partial file cleaned up");
    assert_eq!(r.engine.get(id).unwrap().state, TransferState::Canceled);
    assert!(!r.engine.is_busy());
    assert_eq!(r.engine.cancel(id).unwrap_err().kind, ErrorKind::InvalidArgument);
    assert_eq!(r.engine.pending_summary(), 0);
}

#[tokio::test]
async fn pause_during_backoff_and_resume_completes() {
    let cfg = EngineConfig {
        backoff_base: Duration::from_millis(100),
        ..EngineConfig::default()
    };
    let r = rig_with(
        Db::open_in_memory().unwrap(),
        MemoryProvider::default(),
        MemoryProvider::default(),
        cfg,
        None,
    );
    r.local.add_file("f", b"hello", 1);
    r.local.fail_next("stat", "f", Error::kind(ErrorKind::TimedOut));
    let id = r
        .engine
        .add(copy_one("f"), "", TransferOptions::default())
        .await
        .unwrap();
    tokio::time::timeout(Duration::from_secs(5), async {
        while r.engine.items(id).unwrap()[0].attempts == 0 {
            tokio::task::yield_now().await;
        }
    })
    .await
    .unwrap();
    r.engine.pause(id).unwrap();
    assert_eq!(r.engine.get(id).unwrap().state, TransferState::Paused);
    assert_eq!(r.engine.items(id).unwrap()[0].state, ItemState::Pending);
    assert_eq!(r.engine.pending_summary(), 1);
    assert_eq!(r.engine.pause(id).unwrap_err().kind, ErrorKind::InvalidArgument);
    tokio::time::sleep(Duration::from_millis(250)).await;
    assert!(!r.remote.exists("f"), "paused means paused");
    r.engine.resume(id).unwrap();
    assert_eq!(settled(&r.engine, id).await.state, TransferState::Completed);
    assert_eq!(r.engine.resume(id).unwrap_err().kind, ErrorKind::InvalidArgument);
}

#[tokio::test]
async fn pause_all_and_resume_all() {
    let r = rig();
    r.local.add_file("f", b"hello", 1);
    r.local.add_file("g", b"hello", 1);
    r.engine.volume_present(LOCAL, false);
    let mut ids = Vec::new();
    for n in ["f", "g"] {
        let p = plan(
            OperationKind::Copy,
            LOCAL,
            vec![item(LOCAL, n, LOCAL, &format!("out-{n}"), Kind::File, 5)],
        );
        ids.push(r.engine.add(p, "", TransferOptions::default()).await.unwrap());
    }
    r.engine.pause_all();
    for id in &ids {
        assert_eq!(r.engine.get(*id).unwrap().state, TransferState::Paused);
    }
    r.engine.volume_present(LOCAL, true);
    assert_eq!(
        r.engine.get(ids[0]).unwrap().state,
        TransferState::Paused,
        "gates do not unpause"
    );
    r.engine.resume_all();
    for id in &ids {
        assert_eq!(settled(&r.engine, *id).await.state, TransferState::Completed);
    }
    assert!(r.local.exists("out-f") && r.local.exists("out-g"));
}

#[tokio::test]
async fn reordering_changes_which_transfer_starts_first() {
    let r = rig();
    for n in ["a", "b", "c"] {
        r.local.add_file(n, b"hello", 1);
    }
    r.engine.volume_present(LOCAL, false);
    let mut ids = Vec::new();
    for n in ["a", "b", "c"] {
        let p = plan(
            OperationKind::Copy,
            LOCAL,
            vec![item(LOCAL, n, LOCAL, &format!("out-{n}"), Kind::File, 5)],
        );
        ids.push(r.engine.add(p, "", TransferOptions::default()).await.unwrap());
    }
    let positions = |e: &Engine| -> Vec<TransferId> { e.list().iter().map(|s| s.id).collect() };
    assert_eq!(positions(&r.engine), ids);
    assert!(r.engine.move_to_top(ids[2]));
    assert_eq!(positions(&r.engine), vec![ids[2], ids[0], ids[1]]);
    assert!(r.engine.move_down(ids[2]));
    assert!(r.engine.move_up(ids[1]));
    assert_eq!(positions(&r.engine), vec![ids[0], ids[1], ids[2]]);
    assert!(!r.engine.move_up(ids[0]));
    assert!(r.engine.move_to_top(ids[2]));

    r.engine.volume_present(LOCAL, true);
    for id in &ids {
        settled(&r.engine, *id).await;
    }
    let order: Vec<String> = r
        .local
        .calls()
        .into_iter()
        .filter(|c| c.starts_with("stat out-"))
        .collect();
    assert_eq!(order[0], "stat out-c", "{order:?}");
    assert_eq!(order[1], "stat out-a", "{order:?}");
}

#[tokio::test]
async fn deletes_children_first_and_can_use_the_trasher() {
    let r = rig();
    r.local.add_file("d/e/f", b"12345", 1);
    r.local.add_file("d/g", b"1", 1);
    let items = vec![
        item(LOCAL, "d", LOCAL, "d", Kind::Dir, 0),
        item(LOCAL, "d/e", LOCAL, "d/e", Kind::Dir, 0),
        item(LOCAL, "d/e/f", LOCAL, "d/e/f", Kind::File, 5),
        item(LOCAL, "d/g", LOCAL, "d/g", Kind::File, 1),
    ];
    let id = r
        .engine
        .add(
            plan(OperationKind::Delete, LOCAL, items.clone()),
            "",
            TransferOptions::default(),
        )
        .await
        .unwrap();
    let s = settled(&r.engine, id).await;
    assert_eq!((s.state, s.items_done), (TransferState::Completed, 4), "{s:?}");
    assert!(r.local.paths().is_empty());

    struct Recorder(Mutex<Vec<String>>);
    #[async_trait::async_trait]
    impl Trasher for Recorder {
        async fn trash(&self, uri: &Uri) -> lautta_core::Result<bool> {
            self.0.lock().unwrap().push(uri.path.display());
            Ok(true)
        }
    }
    let rec = Arc::new(Recorder(Mutex::default()));
    let r = rig_with(
        Db::open_in_memory().unwrap(),
        MemoryProvider::default(),
        MemoryProvider::default(),
        config(),
        Some(rec.clone()),
    );
    r.local.add_file("d/e/f", b"12345", 1);
    let opts = TransferOptions {
        trash: true,
        ..TransferOptions::default()
    };
    let id = r
        .engine
        .add(plan(OperationKind::Delete, LOCAL, items), "", opts)
        .await
        .unwrap();
    let s = settled(&r.engine, id).await;
    assert_eq!((s.state, s.items_done), (TransferState::Completed, 4));
    assert_eq!(
        *rec.0.lock().unwrap(),
        vec!["d".to_owned()],
        "only the root is trashed"
    );
}

#[tokio::test]
async fn persistence_reload_brings_unfinished_transfers_back_paused() {
    let db = Db::open_in_memory().unwrap();
    let r = rig_on(db.clone(), MemoryProvider::default(), MemoryProvider::default());
    r.local.add_file("done", b"hello", 1);
    r.local.add_file("ask", b"new!!", 1);
    r.remote.add_file("ask", b"old", 1);
    let done = r
        .engine
        .add(copy_one("done"), "", TransferOptions::default())
        .await
        .unwrap();
    settled(&r.engine, done).await;
    let ask = r
        .engine
        .add(copy_one("ask"), "Copy ask", TransferOptions::default())
        .await
        .unwrap();
    in_state(&r.engine, ask, TransferState::Waiting(WaitReason::Question)).await;
    r.engine.flush().await;

    // "Restart": a new engine on the same database.
    let r2 = rig_on(db.clone(), r.local.clone(), r.remote.clone());
    assert_eq!(r2.engine.load(false).await.unwrap(), 1);
    let list = r2.engine.list();
    assert_eq!(list.len(), 2);
    assert_eq!(list[0].state, TransferState::Completed);
    assert_eq!((list[0].items_done, list[0].bytes_done), (1, 5));
    assert_eq!(list[1].state, TransferState::Paused);
    assert_eq!(list[1].title, "Copy ask");
    let items = r2.engine.items(ask).unwrap();
    assert_eq!(items[0].state, ItemState::NeedsAnswer);
    assert!(items[0].plan.conflict.is_some(), "the question survived");
    assert_eq!(r2.engine.pending_summary(), 1);
    assert!(!r2.engine.is_busy(), "paused transfers hold no KeepAlive");
    assert_eq!(r2.remote.read_file("ask").unwrap(), b"old");

    r2.engine.resume(ask).unwrap();
    in_state(&r2.engine, ask, TransferState::Waiting(WaitReason::Question)).await;
    r2.engine.answer(ask, 0, ConflictChoice::Replace, false).unwrap();
    assert_eq!(settled(&r2.engine, ask).await.state, TransferState::Completed);
    assert_eq!(r2.remote.read_file("ask").unwrap(), b"new!!");
    r2.engine.flush().await;

    let r3 = rig_on(db, r.local.clone(), r.remote.clone());
    assert_eq!(r3.engine.load(true).await.unwrap(), 0);
    assert!(r3
        .engine
        .list()
        .iter()
        .all(|s| s.state == TransferState::Completed));
}

#[tokio::test]
async fn auto_resume_runs_unfinished_work_after_a_restart() {
    let db = Db::open_in_memory().unwrap();
    let r = rig_on(db.clone(), MemoryProvider::default(), MemoryProvider::default());
    r.local.add_file("f", b"hello", 1);
    r.engine.volume_present(LOCAL, false);
    let id = r
        .engine
        .add(copy_one("f"), "", TransferOptions::default())
        .await
        .unwrap();
    r.engine.flush().await;
    let r2 = rig_on(db, r.local.clone(), r.remote.clone());
    assert_eq!(r2.engine.load(true).await.unwrap(), 1);
    assert_eq!(settled(&r2.engine, id).await.state, TransferState::Completed);
    assert_eq!(r2.remote.read_file("f").unwrap(), b"hello");
}

#[tokio::test]
async fn failed_transfers_reload_with_their_items_for_retry() {
    let db = Db::open_in_memory().unwrap();
    let r = rig_on(db.clone(), MemoryProvider::default(), MemoryProvider::default());
    r.local.add_file("f", b"hello", 1);
    r.local
        .fail_next("download_into", "f", Error::kind(ErrorKind::PermissionDenied));
    let id = r
        .engine
        .add(copy_one("f"), "", TransferOptions::default())
        .await
        .unwrap();
    assert_eq!(settled(&r.engine, id).await.state, TransferState::Failed);
    r.engine.flush().await;
    let r2 = rig_on(db, r.local.clone(), r.remote.clone());
    r2.engine.load(false).await.unwrap();
    assert_eq!(r2.engine.get(id).unwrap().state, TransferState::Failed);
    r2.engine.retry_failed(id).unwrap();
    assert_eq!(settled(&r2.engine, id).await.state, TransferState::Completed);
    assert_eq!(r2.remote.read_file("f").unwrap(), b"hello");
}

#[tokio::test]
async fn history_is_purged_after_the_retention() {
    let db = Db::open_in_memory().unwrap();
    let r = rig_on(db.clone(), MemoryProvider::default(), MemoryProvider::default());
    r.local.add_file("f", b"hello", 1);
    let id = r
        .engine
        .add(copy_one("f"), "", TransferOptions::default())
        .await
        .unwrap();
    settled(&r.engine, id).await;
    r.clock.advance(29 * 86_400_000);
    assert_eq!(r.engine.purge_history().await.unwrap(), 0);
    assert!(r.engine.get(id).is_some());
    r.clock.advance(2 * 86_400_000);
    assert_eq!(r.engine.purge_history().await.unwrap(), 1);
    assert!(r.engine.get(id).is_none());
    let n: i64 = db
        .lock()
        .query_row("SELECT count(*) FROM transfers", [], |row| row.get(0))
        .unwrap();
    assert_eq!(n, 0);
}

#[tokio::test]
async fn forgetting_history_rows() {
    let r = rig();
    r.local.add_file("f", b"hello", 1);
    let id = r
        .engine
        .add(copy_one("f"), "", TransferOptions::default())
        .await
        .unwrap();
    settled(&r.engine, id).await;
    r.engine.forget(id).unwrap();
    assert!(r.engine.get(id).is_none());
    assert_eq!(r.engine.forget(id).unwrap_err().kind, ErrorKind::NotFound);
    r.engine.flush().await;
    let n: i64 =
        r.db.lock()
            .query_row("SELECT count(*) FROM transfers", [], |row| row.get(0))
            .unwrap();
    assert_eq!(n, 0);
}

#[tokio::test]
async fn progress_events_carry_bytes_rate_and_eta() {
    let r = rig();
    r.local.add_file("f", b"hello", 1);
    let mut rx = r.engine.subscribe();
    let id = r
        .engine
        .add(copy_one("f"), "", TransferOptions::default())
        .await
        .unwrap();
    settled(&r.engine, id).await;
    let progress: Vec<(u64, u64)> = drain(&mut rx)
        .into_iter()
        .filter_map(|e| match e {
            TransferEvent::Progress {
                id: i,
                bytes_done,
                rate,
                ..
            } if i == id => Some((bytes_done, rate)),
            _ => None,
        })
        .collect();
    assert!(!progress.is_empty());
    assert!(progress.iter().all(|(b, _)| *b <= 5));
}

#[tokio::test]
async fn unknown_locations_fail_the_item_not_the_engine() {
    let r = rig();
    let p = plan(
        OperationKind::Copy,
        "nowhere",
        vec![item(LOCAL, "f", "nowhere", "f", Kind::File, 5)],
    );
    let id = r.engine.add(p, "", TransferOptions::default()).await.unwrap();
    let s = settled(&r.engine, id).await;
    assert_eq!((s.state, s.items_failed), (TransferState::Failed, 1));
    assert!(r.engine.items(id).unwrap()[0]
        .error
        .as_deref()
        .unwrap()
        .contains("nowhere"));
}

#[tokio::test]
async fn an_empty_plan_completes_at_once() {
    let r = rig();
    let id = r
        .engine
        .add(
            plan(OperationKind::Copy, LOCAL, vec![]),
            "Nothing",
            TransferOptions::default(),
        )
        .await
        .unwrap();
    assert_eq!(r.engine.get(id).unwrap().state, TransferState::Completed);
    assert_eq!(r.engine.get(id + 1), None);
    assert_eq!(r.engine.items(id + 1).unwrap_err().kind, ErrorKind::NotFound);
    assert_eq!(r.engine.pause(id + 1).unwrap_err().kind, ErrorKind::NotFound);
}

#[tokio::test]
async fn high_priority_write_backs_run_before_queued_transfers() {
    let r = rig();
    for n in ["a", "b", "c", "w"] {
        r.local.add_file(n, b"hello", 1);
    }
    r.engine.volume_present(LOCAL, false);
    let mut ids = Vec::new();
    for (n, high) in [("a", false), ("b", false), ("c", false), ("w", true)] {
        let p = plan(
            OperationKind::Copy,
            LOCAL,
            vec![item(LOCAL, n, LOCAL, &format!("out-{n}"), Kind::File, 5)],
        );
        let opts = TransferOptions {
            high_priority: high,
            ..TransferOptions::default()
        };
        ids.push(r.engine.add(p, "", opts).await.unwrap());
    }
    r.engine.volume_present(LOCAL, true);
    for id in &ids {
        settled(&r.engine, *id).await;
    }
    let first: Vec<String> = r
        .local
        .calls()
        .into_iter()
        .filter(|c| c.starts_with("stat out-"))
        .take(2)
        .collect();
    assert_eq!(first, vec!["stat out-w", "stat out-a"]);
}

#[tokio::test]
async fn the_local_limit_caps_parallel_items_at_two() {
    // Items of one transfer to a local destination start two at a time.
    let r = rig();
    let mut items = Vec::new();
    for n in 0..6 {
        r.local.add_file(&format!("f{n}"), b"hello", 1);
        items.push(item(
            LOCAL,
            &format!("f{n}"),
            LOCAL,
            &format!("g{n}"),
            Kind::File,
            5,
        ));
    }
    let id = r
        .engine
        .add(
            plan(OperationKind::Copy, LOCAL, items),
            "",
            TransferOptions::default(),
        )
        .await
        .unwrap();
    // Right after add, at most two items have been started.
    let running = r
        .engine
        .items(id)
        .unwrap()
        .iter()
        .filter(|i| i.state == ItemState::Running)
        .count();
    assert_eq!(running, 2);
    assert_eq!(settled(&r.engine, id).await.items_done, 6);
}

#[tokio::test]
async fn a_remote_limit_applies_to_remote_destinations() {
    let r = rig();
    r.engine.set_remote_limit(REMOTE, 4);
    let mut items = Vec::new();
    for n in 0..6 {
        r.local.add_file(&format!("f{n}"), b"hello", 1);
        items.push(item(
            LOCAL,
            &format!("f{n}"),
            REMOTE,
            &format!("g{n}"),
            Kind::File,
            5,
        ));
    }
    let id = r
        .engine
        .add(
            plan(OperationKind::Copy, REMOTE, items),
            "",
            TransferOptions::default(),
        )
        .await
        .unwrap();
    let running = r
        .engine
        .items(id)
        .unwrap()
        .iter()
        .filter(|i| i.state == ItemState::Running)
        .count();
    assert_eq!(running, 4);
    assert_eq!(settled(&r.engine, id).await.items_done, 6);
}

#[tokio::test]
async fn persisted_rows_follow_the_state_changes() {
    let r = rig();
    r.local.add_file("f", b"hello", 1);
    let id = r
        .engine
        .add(copy_one("f"), "", TransferOptions::default())
        .await
        .unwrap();
    settled(&r.engine, id).await;
    r.engine.flush().await;
    let (state, done, finished): (String, i64, Option<i64>) =
        r.db.lock()
            .query_row(
                "SELECT state, items_done, finished_ms FROM transfers WHERE id=?",
                [id],
                |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?)),
            )
            .unwrap();
    assert_eq!((state.as_str(), done), ("completed", 1));
    assert_eq!(finished, Some(1_700_000_000_000));
    let item_state: String =
        r.db.lock()
            .query_row(
                "SELECT state FROM transfer_items WHERE transfer_id=?",
                [id],
                |row| row.get(0),
            )
            .unwrap();
    assert_eq!(item_state, "done");
}
