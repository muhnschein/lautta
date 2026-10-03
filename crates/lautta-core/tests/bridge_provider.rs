// SPDX-License-Identifier: LGPL-2.1-or-later
//! The netvfs provider against the fake bridge: listing, metadata, namespace
//! operations, read handles, lanes, capabilities and error mapping
//! (SPEC NVB-7, NVB-8, LOC-7, §20).

mod bridge_support;

use bridge_support::{eventually, Rig};
use lautta_core::entry::{cap, EntryFlags, Kind};
use lautta_core::error::ErrorKind;
use lautta_core::provider::{list_all, read_all, AttributeChanges, CopyOptions, Lane, Provider, RenameMode};
use lautta_core::vpath::VPath;
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
    // An empty folder is a complete listing, not an error.
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
    // Nothing to change is not a round trip.
    rig.fake.clear_calls();
    provider
        .set_attributes(&p("a"), AttributeChanges::default())
        .await
        .unwrap();
    assert!(rig.fake.calls_of("SetAttributes").is_empty());

    provider
        .server_copy(&p("a"), &p("copy"), CopyOptions::default())
        .await
        .unwrap();
    assert_eq!(rig.fake.file("account:1", b"copy").unwrap(), b"abc");
    let e = provider
        .server_copy(&p("a"), &p("copy"), CopyOptions::default())
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
    // `read_all` still reads everything in pieces.
    assert_eq!(read_all(handle.as_ref(), 8 << 20).await.unwrap(), data);
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
            .detail("HTTP 423")
            .retry_after(2500)
            .times(1),
    );
    let e = provider.stat(&p("f"), true, Lane::Interactive).await.unwrap_err();
    assert_eq!(e.kind, ErrorKind::Locked);
    assert_eq!(e.detail.as_deref(), Some("HTTP 423"));
    assert_eq!(e.retry_after_ms, Some(2500));

    // A name from a newer bridge is a protocol error, never a success.
    rig.fake.fail_next("Stat", "Brandnew", "?");
    let e = provider.stat(&p("f"), true, Lane::Interactive).await.unwrap_err();
    assert_eq!(e.kind, ErrorKind::ProtocolError);
    // A path that climbs out cannot even be formed (SEC-2).
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
