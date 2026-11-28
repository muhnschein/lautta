// SPDX-License-Identifier: LGPL-2.1-or-later

mod bridge_support;

use bridge_support::{eventually, Rig};
use lautta_core::bridge::Network;
use lautta_core::entry::{cap, EntryFlags, Kind};
use lautta_core::error::ErrorKind;
use lautta_core::index::crawl::{Crawl, CrawlOutcome};
use lautta_core::index::store::{IndexStore, RootState, QUERY_LIMIT};
use lautta_core::provider::{
    list_all, no_progress, read_all, AttributeChanges, CopyOptions, FolderCheck, KnownFolder, Lane, Provider,
    RenameMode,
};
use lautta_core::vpath::VPath;
use std::sync::atomic::AtomicBool;
use std::time::{Duration, UNIX_EPOCH};
use tokio::sync::mpsc;

fn p(s: &str) -> VPath {
    VPath::parse(s.as_bytes()).unwrap()
}

fn raw(bytes: &[u8]) -> VPath {
    VPath::parse(bytes).unwrap()
}

#[tokio::test]
async fn capabilities_come_from_the_location() {
    let rig = Rig::ready().await;
    rig.fake.set_capabilities(
        "account:1",
        &["Symlinks", "PosixModes", "WriteResume", "ServerCopy"],
        &["sha256"],
        255,
    );
    let provider = rig.provider();
    assert!(
        !provider.capabilities().has(cap::SYMLINKS),
        "nothing is known before the first fetch"
    );
    assert!(provider.capabilities().writable());
    provider.refresh_capabilities().await.unwrap();
    let caps = provider.capabilities();
    for flag in [
        cap::SYMLINKS,
        cap::PERMISSIONS,
        cap::RESUME_UPLOAD,
        cap::SERVER_COPY,
        "PosixModes",
    ] {
        assert!(caps.has(flag), "{flag}");
    }
    assert!(!caps.has(cap::HARDLINKS));
    assert_eq!(caps.checksum_algorithms, vec!["sha256".to_owned()]);
    assert_eq!(caps.max_name_bytes, Some(255));
    assert_eq!(provider.location(), "nv-account:1");
}

#[tokio::test]
async fn known_capabilities_are_fetched_on_first_use() {
    let rig = Rig::ready().await;
    rig.fake
        .set_capabilities("account:1", &["WindowsNames"], &[], 255);
    let provider = rig.provider();
    assert!(!provider.has_capabilities());
    let caps = provider.known_capabilities().await;
    assert!(caps.has(cap::RESTRICTED_NAMES) && caps.writable());
    assert!(provider.has_capabilities());
    rig.fake.set_capabilities("account:1", &["Symlinks"], &[], 255);
    assert!(!provider.known_capabilities().await.has(cap::SYMLINKS));
    let unknown = rig.client.provider("nv-account:9");
    assert!(unknown.known_capabilities().await.writable());
    assert!(!unknown.has_capabilities());
}

#[tokio::test]
async fn listings_arrive_in_batches_with_names_as_bytes() {
    let rig = Rig::ready().await;
    for i in 0..600 {
        rig.fake
            .put_file("account:1", format!("big/f{i:04}").as_bytes(), b"x");
    }
    let odd: &[u8] = b"big/caf\xe9 \xff.txt";
    rig.fake.put_file("account:1", odd, b"odd");
    rig.fake.put_file("account:1", "big/Zoë.txt".as_bytes(), b"z");
    let provider = rig.provider();

    let (tx, mut rx) = mpsc::channel(64);
    let big = p("big");
    let list = provider.list(&big, Lane::Interactive, tx);
    let collect = async {
        let mut batches = Vec::new();
        while let Some(b) = rx.recv().await {
            batches.push(b);
        }
        batches
    };
    let (done, batches) = tokio::join!(list, collect);
    done.unwrap();
    assert!(
        batches.len() >= 3,
        "602 entries come in several batches, got {}",
        batches.len()
    );
    assert!(batches.iter().all(|b| b.len() <= 512));
    let all: Vec<_> = batches.into_iter().flatten().collect();
    assert_eq!(all.len(), 602);

    let lossy = all
        .iter()
        .find(|e| e.name == odd[4..])
        .expect("the non-UTF-8 name arrives byte for byte");
    assert!(lossy.name_is_lossy());
    assert!(lossy.flags.contains(EntryFlags::NAME_NOT_UTF8));
    assert_eq!(lossy.size, Some(3));
    let utf8 = all.iter().find(|e| e.name == "Zoë.txt".as_bytes()).unwrap();
    assert!(!utf8.name_is_lossy());
    assert_eq!(utf8.display_name(), "Zoë.txt");
    assert_eq!(lossy.display_name(), "caf\u{fffd} \u{fffd}.txt");
}

#[tokio::test]
async fn a_path_with_odd_bytes_is_sent_unchanged() {
    let rig = Rig::ready().await;
    let dir: &[u8] = b"d\xe9\xff";
    rig.fake.put_file("account:1", b"d\xe9\xff/inner", b"1");
    let all = list_all(&rig.provider(), &raw(dir), Lane::Interactive)
        .await
        .unwrap();
    assert_eq!(all.len(), 1);
    assert_eq!(all[0].name, b"inner");
    let e = rig
        .provider()
        .stat(&raw(b"d\xe9\xff/inner"), true, Lane::Interactive)
        .await
        .unwrap();
    assert_eq!(e.size, Some(1));
}

#[tokio::test]
async fn listing_errors_come_with_the_done_signal() {
    let rig = Rig::ready().await;
    rig.fake.put_file("account:1", b"f", b"x");
    let provider = rig.provider();
    let e = list_all(&provider, &p("missing"), Lane::Interactive)
        .await
        .unwrap_err();
    assert_eq!(e.kind, ErrorKind::NotFound);
    let e = list_all(&provider, &p("f"), Lane::Interactive).await.unwrap_err();
    assert_eq!(e.kind, ErrorKind::NotADirectory);
    rig.fake.put_dir("account:1", b"empty");
    assert!(list_all(&provider, &p("empty"), Lane::Interactive)
        .await
        .unwrap()
        .is_empty());
}

#[tokio::test]
async fn a_reader_that_goes_away_stops_the_listing() {
    let rig = Rig::ready().await;
    for i in 0..40 {
        rig.fake.put_file("account:1", format!("d/f{i}").as_bytes(), b"x");
    }
    rig.fake.set_latency(Duration::from_millis(5));
    let (tx, rx) = mpsc::channel(1);
    drop(rx);
    let e = rig
        .provider()
        .list(&p("d"), Lane::Interactive, tx)
        .await
        .unwrap_err();
    assert_eq!(e.kind, ErrorKind::Canceled);
    eventually("the Cancel", || !rig.fake.calls_of("Cancel").is_empty()).await;
}

#[tokio::test]
async fn stat_reads_every_field() {
    let rig = Rig::ready().await;
    rig.fake.put_file("account:1", b"docs/a.txt", b"hello");
    rig.fake.put_symlink("account:1", b"ln", b"docs/a.txt");
    rig.fake.put_symlink("account:1", b"dangling", b"nope");
    let provider = rig.provider();

    let file = provider
        .stat(&p("docs/a.txt"), true, Lane::Interactive)
        .await
        .unwrap();
    assert_eq!(
        (file.kind, file.size, file.mode),
        (Kind::File, Some(5), Some(0o644))
    );
    assert_eq!(
        file.modified,
        Some(UNIX_EPOCH + Duration::from_millis(1_700_000_000_000))
    );
    assert_eq!(file.name, b"a.txt");

    let dir = provider.stat(&p("docs"), true, Lane::Interactive).await.unwrap();
    assert!(dir.is_dir());
    assert_eq!(dir.size, None);

    let link = provider.stat(&p("ln"), false, Lane::Interactive).await.unwrap();
    assert!(link.is_symlink());
    assert_eq!(link.target_kind, Kind::File);
    assert_eq!(
        provider
            .stat(&p("ln"), true, Lane::Interactive)
            .await
            .unwrap()
            .kind,
        Kind::File
    );
    assert_eq!(provider.read_link(&p("ln")).await.unwrap(), b"docs/a.txt");

    let dangling = provider
        .stat(&p("dangling"), false, Lane::Interactive)
        .await
        .unwrap();
    assert!(dangling.flags.contains(EntryFlags::TARGET_UNKNOWN));
    assert_eq!(
        provider
            .stat(&p("dangling"), true, Lane::Interactive)
            .await
            .unwrap_err()
            .kind,
        ErrorKind::NotFound
    );
}

#[tokio::test]
async fn every_request_carries_a_lane_hint() {
    let rig = Rig::ready().await;
    rig.fake.put_file("account:1", b"d/f", b"data");
    let provider = rig.provider();
    provider.stat(&p("d/f"), true, Lane::Bulk).await.unwrap();
    provider.stat(&p("d/f"), true, Lane::Interactive).await.unwrap();
    list_all(&provider, &p("d"), Lane::Bulk).await.unwrap();
    drop(provider.open_read(&p("d/f"), Lane::Stream).await.unwrap());
    drop(provider.open_read(&p("d/f"), Lane::Interactive).await.unwrap());
    let lanes = |m: &str| {
        rig.fake
            .calls_of(m)
            .into_iter()
            .map(|c| c.lane)
            .collect::<Vec<_>>()
    };
    assert_eq!(lanes("Stat"), vec!["bulk", "interactive"]);
    assert_eq!(lanes("List"), vec!["bulk"]);
    assert_eq!(lanes("OpenRead"), vec!["stream", "interactive"]);
}

#[tokio::test]
async fn namespace_operations() {
    let rig = Rig::ready().await;
    rig.fake.put_file("account:1", b"f.txt", b"12345");
    let provider = rig.provider();

    provider.make_dir(&p("new"), true).await.unwrap();
    assert_eq!(
        provider.make_dir(&p("new"), true).await.unwrap_err().kind,
        ErrorKind::AlreadyExists
    );
    provider.make_dir(&p("new"), false).await.unwrap();

    provider
        .rename(&p("new"), &p("renamed"), RenameMode::NoReplace)
        .await
        .unwrap();
    assert!(rig.fake.exists("account:1", b"renamed"));
    assert!(!rig.fake.exists("account:1", b"new"));
    assert_eq!(
        provider
            .rename(&p("f.txt"), &p("renamed"), RenameMode::NoReplace)
            .await
            .unwrap_err()
            .kind,
        ErrorKind::AlreadyExists
    );
    rig.fake.put_file("account:1", b"g.txt", b"other");
    provider
        .rename(&p("g.txt"), &p("f.txt"), RenameMode::Replace)
        .await
        .unwrap();
    assert_eq!(rig.fake.file("account:1", b"f.txt").unwrap(), b"other");

    provider.remove_dir(&p("renamed")).await.unwrap();
    provider.remove_file(&p("f.txt")).await.unwrap();
    assert_eq!(
        provider.remove_file(&p("f.txt")).await.unwrap_err().kind,
        ErrorKind::NotFound
    );
    assert_eq!(
        provider.remove_dir(&p("nope")).await.unwrap_err().kind,
        ErrorKind::NotFound
    );
}

#[tokio::test]
async fn links_attributes_copies_checksums_and_space() {
    let rig = Rig::ready().await;
    rig.fake.put_file("account:1", b"a", b"abc");
    rig.fake.set_space("account:1", 1000);
    let provider = rig.provider();

    provider.make_symlink(b"a", &p("sym")).await.unwrap();
    assert_eq!(provider.read_link(&p("sym")).await.unwrap(), b"a");
    provider.make_hardlink(&p("a"), &p("hard")).await.unwrap();
    assert_eq!(rig.fake.file("account:1", b"hard").unwrap(), b"abc");

    provider
        .set_attributes(
            &p("a"),
            AttributeChanges {
                mode: Some(0o100600),
                modified: Some(UNIX_EPOCH + Duration::from_millis(42_000)),
            },
        )
        .await
        .unwrap();
    let e = provider.stat(&p("a"), true, Lane::Interactive).await.unwrap();
    assert_eq!(e.mode, Some(0o600), "only the permission bits travel");
    assert_eq!(e.modified, Some(UNIX_EPOCH + Duration::from_millis(42_000)));
    rig.fake.clear_calls();
    provider
        .set_attributes(&p("a"), AttributeChanges::default())
        .await
        .unwrap();
    assert!(rig.fake.calls_of("SetAttributes").is_empty());

    provider
        .server_copy(&p("a"), &p("copy"), CopyOptions::default(), no_progress())
        .await
        .unwrap();
    assert_eq!(rig.fake.file("account:1", b"copy").unwrap(), b"abc");
    let e = provider
        .server_copy(&p("a"), &p("copy"), CopyOptions::default(), no_progress())
        .await
        .unwrap_err();
    assert_eq!(e.kind, ErrorKind::AlreadyExists);
    provider
        .server_copy(
            &p("a"),
            &p("copy"),
            CopyOptions {
                replace: true,
                preserve_mtime: false,
            },
            no_progress(),
        )
        .await
        .unwrap();

    let digest = provider.checksum(&p("a"), "sha256").await.unwrap();
    assert_eq!(digest.len(), 32);
    assert_eq!(
        provider.checksum(&p("a"), "crc32").await.unwrap_err().kind,
        ErrorKind::Unsupported
    );

    let space = provider.space(&VPath::root()).await.unwrap();
    assert_eq!((space.total, space.used, space.free), (1000, 9, 991));
}

#[tokio::test]
async fn read_handles_read_ranges_and_close_on_drop() {
    let rig = Rig::ready().await;
    rig.fake.put_file("account:1", b"v.bin", b"0123456789");
    let provider = rig.provider();
    let handle = provider.open_read(&p("v.bin"), Lane::Stream).await.unwrap();
    assert_eq!(handle.size(), Some(10));
    assert_eq!(handle.read_at(3, 4).await.unwrap(), b"3456");
    assert_eq!(handle.read_at(8, 100).await.unwrap(), b"89");
    assert!(handle.read_at(10, 5).await.unwrap().is_empty(), "end of file");
    assert!(handle.read_at(2, 0).await.unwrap().is_empty());
    handle.read_ahead(0, 1024).await.unwrap();
    assert_eq!(read_all(handle.as_ref(), 1 << 20).await.unwrap(), b"0123456789");
    assert_eq!(rig.fake.open_handles(), 1);
    drop(handle);
    eventually("the handle to close", || rig.fake.open_handles() == 0).await;
}

#[tokio::test]
async fn reads_are_capped_at_one_mebibyte() {
    let rig = Rig::ready().await;
    let data: Vec<u8> = (0..(3u32 << 19)).map(|i| (i % 251) as u8).collect();
    rig.fake.put_file("account:1", b"big.bin", &data);
    let handle = rig
        .provider()
        .open_read(&p("big.bin"), Lane::Stream)
        .await
        .unwrap();
    let first = handle.read_at(0, 4 << 20).await.unwrap();
    assert_eq!(
        first.len(),
        1 << 20,
        "a larger request is answered with at most 1 MiB"
    );
    assert_eq!(first, data[..1 << 20]);
    assert_eq!(read_all(handle.as_ref(), 8 << 20).await.unwrap(), data);
}

fn without_read_handles(rig: &Rig) {
    let flags: Vec<&str> = lautta_bridge_proto::fake::DEFAULT_CAPS
        .iter()
        .copied()
        .filter(|f| *f != "ReadHandles")
        .collect();
    rig.fake.set_capabilities("account:1", &flags, &[], 255);
}

#[tokio::test]
async fn a_location_without_read_handles_is_read_through_ranged_downloads() {
    let rig = Rig::ready().await;
    without_read_handles(&rig);
    let data: Vec<u8> = (0..(5u32 << 20)).map(|i| (i % 251) as u8).collect();
    rig.fake.put_file("account:1", b"big.bin", &data);
    let provider = rig.provider();
    let handle = provider
        .open_read(&p("big.bin"), Lane::Interactive)
        .await
        .unwrap();
    assert_eq!(handle.size(), Some(data.len() as u64));
    assert_eq!(handle.read_at(3, 4).await.unwrap(), data[3..7]);
    assert_eq!(
        handle.read_at(100, 4).await.unwrap(),
        data[100..104],
        "served from the range read"
    );
    assert_eq!(rig.fake.calls_of("Download").len(), 1);
    let far = 4usize << 20;
    assert_eq!(handle.read_at(far as u64, 10).await.unwrap(), data[far..far + 10]);
    assert_eq!(rig.fake.calls_of("Download").len(), 2, "a jump is a new range");
    assert!(
        handle.read_at(data.len() as u64, 5).await.unwrap().is_empty(),
        "end of file"
    );
    assert!(handle.read_at(2, 0).await.unwrap().is_empty());
    assert_eq!(
        rig.fake.calls_of("Download").len(),
        2,
        "nothing is fetched past the end"
    );
    rig.fake.clear_calls();
    assert_eq!(read_all(handle.as_ref(), 8 << 20).await.unwrap(), data);
    assert_eq!(
        rig.fake.calls_of("Download").len(),
        3,
        "a read from the start takes ranges growing from 1 MiB: 1 + 2 + 4 MiB cover 5 MiB"
    );
    assert_eq!(rig.fake.open_handles(), 0);
    assert!(
        rig.fake.calls_of("OpenRead").is_empty(),
        "no read handle is asked for"
    );
}

#[tokio::test]
async fn a_location_without_read_handles_still_fails_cleanly() {
    let rig = Rig::ready().await;
    without_read_handles(&rig);
    rig.fake.put_dir("account:1", b"dir");
    let provider = rig.provider();
    let kind = |path: &'static str| {
        let provider = &provider;
        async move {
            provider
                .open_read(&p(path), Lane::Stream)
                .await
                .err()
                .unwrap()
                .kind
        }
    };
    assert_eq!(kind("missing").await, ErrorKind::NotFound);
    assert_eq!(kind("dir").await, ErrorKind::IsADirectory);
}

#[tokio::test]
async fn opening_a_missing_file_or_a_folder_fails_cleanly() {
    let rig = Rig::ready().await;
    rig.fake.put_dir("account:1", b"dir");
    let provider = rig.provider();
    assert_eq!(
        provider
            .open_read(&p("missing"), Lane::Stream)
            .await
            .err()
            .unwrap()
            .kind,
        ErrorKind::NotFound
    );
    assert_eq!(
        provider
            .open_read(&p("dir"), Lane::Stream)
            .await
            .err()
            .unwrap()
            .kind,
        ErrorKind::IsADirectory
    );
}

#[tokio::test]
async fn too_many_open_handles_report_a_retry_hint() {
    let rig = Rig::ready().await;
    rig.fake.put_file("account:1", b"f", b"x");
    let provider = rig.provider();
    let mut open = Vec::new();
    for _ in 0..16 {
        open.push(provider.open_read(&p("f"), Lane::Stream).await.unwrap());
    }
    let e = provider.open_read(&p("f"), Lane::Stream).await.err().unwrap();
    assert_eq!(e.kind, ErrorKind::TooManyConnections);
    assert_eq!(e.retry_after_ms, Some(1000));
    assert!(e.kind.is_transient());
}

#[tokio::test]
async fn bridge_errors_keep_their_name_message_detail_and_retry_hint() {
    let rig = Rig::ready().await;
    rig.fake.put_file("account:1", b"f", b"x");
    let provider = rig.provider();
    rig.fake.fail_next("Stat", "RateLimited", "slow down");
    let e = provider.stat(&p("f"), true, Lane::Interactive).await.unwrap_err();
    assert_eq!(
        (e.kind, e.message.as_str(), e.detail, e.retry_after_ms),
        (ErrorKind::RateLimited, "slow down", None, None)
    );

    rig.fake.fail(
        lautta_bridge_proto::fake::Failure::new("Stat", "Locked", "busy")
            .detail("sharing violation")
            .retry_after(2500)
            .times(1),
    );
    let e = provider.stat(&p("f"), true, Lane::Interactive).await.unwrap_err();
    assert_eq!(e.kind, ErrorKind::Locked);
    assert_eq!(e.detail.as_deref(), Some("sharing violation"));
    assert_eq!(e.retry_after_ms, Some(2500));

    rig.fake.fail_next("Stat", "Brandnew", "?");
    let e = provider.stat(&p("f"), true, Lane::Interactive).await.unwrap_err();
    assert_eq!(e.kind, ErrorKind::ProtocolError);
    assert_eq!(VPath::parse(b"a/../b").unwrap_err().kind, ErrorKind::InvalidName);
}

#[tokio::test]
async fn an_unknown_location_is_not_found() {
    let rig = Rig::ready().await;
    let e = rig
        .client
        .provider("nv-account:9")
        .stat(&VPath::root(), true, Lane::Interactive)
        .await
        .unwrap_err();
    assert_eq!(e.kind, ErrorKind::NotFound);
    let e = rig
        .client
        .provider("nv-account:9")
        .refresh_capabilities()
        .await
        .unwrap_err();
    assert_eq!(e.kind, ErrorKind::NotFound);
}

#[tokio::test]
async fn a_dropped_connection_is_connection_lost() {
    let rig = Rig::ready().await;
    rig.fake.put_file("account:1", b"f", b"x");
    let provider = rig.provider();
    let handle = provider.open_read(&p("f"), Lane::Stream).await.unwrap();
    rig.fake.disconnect_clients().await;
    let e = provider.stat(&p("f"), true, Lane::Interactive).await.unwrap_err();
    assert_eq!(e.kind, ErrorKind::ConnectionLost, "{e}");
    let e = handle.read_at(0, 1).await.unwrap_err();
    assert_eq!(e.kind, ErrorKind::ConnectionLost, "{e}");
}

const T0: i64 = 1_700_000_000_000;

fn known(path: &str, etag: Option<&[u8]>, modified_ms: Option<i64>) -> KnownFolder {
    KnownFolder {
        path: p(path),
        etag: etag.map(<[u8]>::to_vec),
        modified_ms,
    }
}

fn outcome(check: &FolderCheck) -> String {
    match check {
        FolderCheck::Failed(e) => format!("failed {:?}", e.kind),
        FolderCheck::Unchanged(e) => format!("unchanged {}", String::from_utf8_lossy(&e.name)),
        FolderCheck::Listed(e, children) => {
            let mut names: Vec<String> = children
                .iter()
                .map(|c| String::from_utf8_lossy(&c.name).into_owned())
                .collect();
            names.sort();
            format!("listed {} {}", String::from_utf8_lossy(&e.name), names.join(","))
        }
    }
}

#[tokio::test]
async fn folders_are_checked_in_one_bridge_call() {
    let rig = Rig::ready().await;
    rig.fake.put_file("account:1", b"t/a/1", b"x");
    rig.fake.put_file("account:1", b"t/b/2", b"x");
    rig.fake.set_now_ms("account:1", T0 + 10_000);
    let provider = rig.provider();
    let folders = [
        known("t/a", None, Some(T0)),
        known("t/gone", None, None),
        known("t/b", None, Some(T0 - 1)),
    ];
    let checks = provider.check_folders(&folders, T0, Lane::Bulk).await.unwrap();
    let seen: Vec<String> = checks.iter().map(outcome).collect();
    assert_eq!(seen, vec!["unchanged a", "failed NotFound", "listed b 2"]);
    assert_eq!(rig.fake.calls_of("Check").len(), 1);
    assert_eq!(rig.fake.calls_of("Check")[0].lane, "bulk");
    assert!(rig.fake.calls_of("Stat").is_empty() && rig.fake.calls_of("List").is_empty());
    match &checks[0] {
        FolderCheck::Unchanged(e) => assert_eq!(e.modified_ms(), Some(T0)),
        other => panic!("{other:?}"),
    }
}

#[tokio::test]
async fn a_bridge_without_check_is_asked_folder_by_folder() {
    let rig = Rig::ready().await;
    rig.fake.set_features(Some(&["storage-settings"]));
    rig.fake.put_file("account:1", b"t/a/1", b"x");
    let rig = {
        rig.fake.disconnect_clients().await;
        rig
    };
    eventually("the features are read again", || !rig.client.offers("check")).await;
    bridge_support::status(&rig.client, lautta_core::bridge::BridgeStatus::Ready).await;
    let provider = rig.provider();
    let checks = provider
        .check_folders(&[known("t/a", None, None)], T0 + 10_000, Lane::Bulk)
        .await
        .unwrap();
    assert_eq!(outcome(&checks[0]), "listed a 1");
    assert!(rig.fake.calls_of("Check").is_empty());
    assert_eq!(rig.fake.calls_of("Stat").len(), 1);
    assert_eq!(rig.fake.calls_of("List").len(), 1);
}

#[tokio::test]
async fn more_folders_than_one_call_takes_are_sent_in_parts_and_a_failed_call_fails() {
    let rig = Rig::ready().await;
    rig.fake.put_dir("account:1", b"t");
    let provider = rig.provider();
    let many: Vec<KnownFolder> = (0..1500).map(|_| known("t", None, None)).collect();
    let checks = provider.check_folders(&many, T0, Lane::Bulk).await.unwrap();
    assert_eq!(checks.len(), 1500);
    assert!(checks.iter().all(|c| matches!(c, FolderCheck::Listed(..))));
    assert_eq!(rig.fake.calls_of("Check").len(), 2);
    rig.fake.fail_next("Check", "ConnectionLost", "gone");
    let e = provider
        .check_folders(&many[..2], T0, Lane::Bulk)
        .await
        .unwrap_err();
    assert_eq!(e.kind, ErrorKind::ConnectionLost);
}

#[tokio::test]
async fn a_server_is_crawled_through_check_and_deep_etags_skip_what_did_not_change() {
    let rig = Rig::ready().await;
    for path in ["a/x/1.txt", "a/y/deep/2.txt", "b/3.txt", "b/z/4.txt", "top.txt"] {
        rig.fake.put_file("account:1", path.as_bytes(), b"x");
    }
    rig.fake.set_deep_etags("account:1", true);
    let mut flags: Vec<&str> = lautta_bridge_proto::fake::DEFAULT_CAPS.to_vec();
    flags.push("DeepETags");
    rig.fake.set_capabilities("account:1", &flags, &["sha256"], 255);
    let provider = rig.provider();
    let dir = tempfile::tempdir().unwrap();
    let store = IndexStore::open(&dir.path().join("names.db")).unwrap();
    let root = store.root("nv-account:1", &VPath::root()).unwrap();
    let stop = AtomicBool::new(false);
    let now = || T0;
    let crawl = || Crawl {
        store: &store,
        provider: &provider,
        stop: &stop,
        now_ms: &now,
        events: None,
    };
    assert_eq!(crawl().run(&root).await.unwrap(), CrawlOutcome::Done);
    assert_eq!(store.query(&[root.id], ".txt", QUERY_LIMIT).unwrap().len(), 5);
    let folders = |calls: usize| rig.fake.calls_of("Check").len() - calls;
    let first = rig.fake.calls_of("Check").len();
    assert!(first <= 5, "one call per level, not per folder: {first}");

    assert_eq!(crawl().run(&root).await.unwrap(), CrawlOutcome::Done);
    assert_eq!(
        folders(first),
        1,
        "an unchanged server costs one call for one folder"
    );

    rig.fake.put_file("account:1", b"a/y/deep/new.txt", b"x");
    let before = rig.fake.calls_of("Check").len();
    assert_eq!(crawl().run(&root).await.unwrap(), CrawlOutcome::Done);
    assert_eq!(
        folders(before),
        4,
        "the top folder, then one call per level down to the change"
    );
    let found = store.query(&[root.id], "new.txt", QUERY_LIMIT).unwrap();
    assert_eq!(found.len(), 1);
    assert_eq!(found[0].folder, p("a/y/deep"));
    assert!(rig.fake.calls_of("Stat").is_empty() && rig.fake.calls_of("List").is_empty());

    rig.fake.fail_next("Check", "ConnectionLost", "gone");
    let e = crawl().run(&root).await.unwrap_err();
    assert_eq!(e.kind, ErrorKind::ConnectionLost);
    assert_eq!(
        store.root_info(root.id).unwrap().unwrap().state,
        RootState::Failed
    );
    assert_eq!(
        store.query(&[root.id], ".txt", QUERY_LIMIT).unwrap().len(),
        6,
        "the index is kept"
    );
}

#[tokio::test]
async fn a_server_that_offers_search_is_asked_for_names() {
    let rig = Rig::ready().await;
    for path in ["docs/Tax/tax-2026.pdf", "docs/other.txt", "taxi.jpg"] {
        rig.fake.put_file("account:1", path.as_bytes(), b"12");
    }
    let provider = rig.provider_with_caps().await;
    let mut found = provider.search_names(&p("docs"), "TAX", 100).await.unwrap();
    found.sort_by(|a, b| a.0.as_bytes().cmp(b.0.as_bytes()));
    let paths: Vec<String> = found.iter().map(|(path, _)| path.display()).collect();
    assert_eq!(paths, vec!["docs/Tax", "docs/Tax/tax-2026.pdf"]);
    assert_eq!((found[0].1.kind, found[1].1.kind), (Kind::Dir, Kind::File));
    assert_eq!(found[1].1.size, Some(2));
    assert_eq!(
        provider
            .search_names(&VPath::root(), "tax", 2)
            .await
            .unwrap()
            .len(),
        2
    );
    let e = provider.search_names(&p("gone"), "tax", 5).await.unwrap_err();
    assert_eq!(e.kind, ErrorKind::NotFound);

    rig.fake.set_capabilities("account:1", &["ReadHandles"], &[], 255);
    provider.refresh_capabilities().await.unwrap();
    rig.fake.clear_calls();
    let e = provider.search_names(&p("docs"), "tax", 5).await.unwrap_err();
    assert_eq!(e.kind, ErrorKind::Unsupported);
    assert!(
        rig.fake.calls_of("Search").is_empty(),
        "a server without Search is not asked"
    );
}

#[tokio::test]
async fn the_network_kind_follows_the_bridge() {
    let rig = Rig::ready().await;
    eventually("the kind is read", || rig.client.network() == Network::Wifi).await;
    let watch = rig.client.watch_network();
    rig.fake.set_network("mobile").await;
    eventually("mobile data is seen", || rig.client.network() == Network::Mobile).await;
    assert!(watch.has_changed().unwrap());
    rig.fake.set_network("none").await;
    eventually("no network is seen", || rig.client.network() == Network::Offline).await;
    assert!(!Network::Mobile.unmetered() && Network::Wifi.unmetered() && Network::Ethernet.unmetered());
    assert!(!Network::Offline.unmetered() && !Network::Unknown.unmetered() && !Network::Other.unmetered());
    assert_eq!(Network::from_wire("ethernet"), Network::Ethernet);
    assert_eq!(Network::from_wire("other"), Network::Other);
    assert_eq!(Network::from_wire("carrier pigeon"), Network::Unknown);
    rig.fake.disconnect_clients().await;
    eventually("a lost bridge leaves the kind unknown or reads it again", || {
        matches!(rig.client.network(), Network::Unknown | Network::Offline)
    })
    .await;
}
