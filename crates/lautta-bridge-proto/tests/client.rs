// SPDX-License-Identifier: LGPL-2.1-or-later
//! The client side of the protocol against the fake: errors with details,
//! malformed calls, ordered signals, disconnects and secret handling
//! (SPEC NVB-2, NVB-6, SEC-1, TST-2).
#![cfg(feature = "fake")]

use futures::StreamExt;
use lautta_bridge_proto::fake::{Failure, FakeBridge};
use lautta_bridge_proto::{connect, BridgeError, OptsBuilder, Signal, SignalStream, Zeroizing};
use std::time::Duration;

async fn fake_with_account() -> FakeBridge {
    let fake = FakeBridge::new();
    fake.add_account(1, "fake", "Fake 1", "fake.example").await;
    fake
}

async fn hello(conn: &lautta_bridge_proto::Connection) {
    conn.proxy().hello("test").await.unwrap();
}

#[tokio::test]
async fn connects_to_a_socket_path_without_a_bus() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("netvfs/bridge.sock");
    let fake = fake_with_account().await;
    let _listener = fake.listen(&path).unwrap();
    let conn = connect(&path).await.unwrap();
    let version = conn.proxy().hello("test").await.unwrap();
    assert_eq!(version, "0.2.0-fake");
    let locations = conn.proxy().list_locations().await.unwrap();
    assert_eq!(locations.len(), 1);
    assert_eq!(
        (locations[0].id.as_str(), locations[0].provider.as_str()),
        ("account:1", "fake")
    );
}

#[tokio::test]
async fn hello_answers_the_bridge_version_for_a_client_name_of_up_to_256_bytes() {
    let fake = fake_with_account().await;
    fake.set_hello("0.6.0");
    let conn = fake.connect().await.unwrap();
    let e = BridgeError::from(conn.proxy().hello(&"x".repeat(257)).await.unwrap_err());
    assert_eq!(e, BridgeError::InvalidArgs("Argument 1 is too long".into()));
    let version = conn.proxy().hello(&"x".repeat(256)).await.unwrap();
    assert_eq!(version, "0.6.0");
}

#[tokio::test]
async fn a_missing_socket_is_a_disconnect() {
    let dir = tempfile::tempdir().unwrap();
    let e = connect(&dir.path().join("none.sock")).await.err().unwrap();
    assert!(matches!(e, BridgeError::Disconnected(_)), "{e:?}");
}

#[tokio::test]
async fn errors_carry_name_message_detail_and_retry_hint() {
    let fake = fake_with_account().await;
    fake.put_file("account:1", b"f", b"x");
    let conn = fake.connect().await.unwrap();
    hello(&conn).await;

    fake.fail(
        Failure::new("Stat", "RateLimited", "slow down")
            .detail("server busy")
            .retry_after(1500)
            .times(1),
    );
    let e = BridgeError::from(conn.proxy().stat("account:1", b"f", true, "").await.unwrap_err());
    match e {
        BridgeError::Remote {
            name,
            message,
            detail,
            retry_after_ms,
        } => {
            assert_eq!(name, "org.netvfs.Error.RateLimited");
            assert_eq!(message, "slow down");
            assert_eq!(detail.as_deref(), Some("server busy"));
            assert_eq!(retry_after_ms, Some(1500));
        }
        other => panic!("unexpected {other:?}"),
    }

    // Without details the second argument is absent and the message still reads.
    let e = BridgeError::from(
        conn.proxy()
            .stat("account:1", b"missing", true, "")
            .await
            .unwrap_err(),
    );
    assert_eq!(e.netvfs_name(), Some("NotFound"));
    match e {
        BridgeError::Remote {
            detail,
            retry_after_ms,
            ..
        } => assert_eq!((detail, retry_after_ms), (None, None)),
        other => panic!("unexpected {other:?}"),
    }
}

#[tokio::test]
async fn malformed_calls_are_invalid_args() {
    let fake = fake_with_account().await;
    let conn = fake.connect().await.unwrap();
    hello(&conn).await;
    let p = conn.proxy();
    let invalid = |e: zbus::Error| matches!(BridgeError::from(e), BridgeError::InvalidArgs(_));
    assert!(invalid(p.stat("account:1", b"", true, "fast").await.unwrap_err()));
    assert!(invalid(p.stat("no such", b"", true, "").await.unwrap_err()));
    assert!(invalid(p.list("account:1", b"", "", 513).await.unwrap_err()));
    assert!(invalid(p.read(1, 0, (1 << 20) + 1).await.unwrap_err()));
    assert!(invalid(p.read(1, -1, 1).await.unwrap_err()));
    let bad_opts = OptsBuilder::new().str("owner", "root").build();
    assert!(invalid(
        p.set_attributes("account:1", b"a", &bad_opts).await.unwrap_err()
    ));
    // A path that climbs out is a name error, not an argument error.
    let e = BridgeError::from(p.stat("account:1", b"a/../b", true, "").await.unwrap_err());
    assert_eq!(e.netvfs_name(), Some("InvalidName"));
}

#[tokio::test]
async fn calls_before_hello_are_refused() {
    let fake = fake_with_account().await;
    let conn = fake.connect().await.unwrap();
    let e = BridgeError::from(conn.proxy().list_locations().await.unwrap_err());
    assert_eq!(e.netvfs_name(), Some("ProtocolError"));
    hello(&conn).await;
    assert_eq!(conn.proxy().list_locations().await.unwrap().len(), 1);
}

#[tokio::test]
async fn listing_signals_arrive_in_order() {
    let fake = fake_with_account().await;
    for name in ["a", "b", "c"] {
        fake.put_file("account:1", name.as_bytes(), b"x");
    }
    let conn = fake.connect().await.unwrap();
    let mut signals = conn.signals();
    hello(&conn).await;
    let req = conn.proxy().list("account:1", b"", "", 1).await.unwrap();
    let mut names = Vec::new();
    loop {
        match signals.next().await.expect("the stream stays open").unwrap() {
            Signal::ListBatch { req: r, entries } => {
                assert_eq!(r, req);
                assert_eq!(entries.len(), 1, "batches of one were asked for");
                names.push(entries[0].name.clone());
            }
            Signal::ListDone { req: r, error, .. } => {
                assert_eq!((r, error.as_str()), (req, ""));
                break;
            }
            // The listing connected the location: its state is told.
            Signal::LocationsChanged => {}
            other => panic!("unexpected {other:?}"),
        }
    }
    assert_eq!(
        names,
        vec![b"a".to_vec(), b"b".to_vec(), b"c".to_vec()],
        "ListDone came after the last batch"
    );
}

#[tokio::test]
async fn the_signal_stream_ends_when_the_bridge_goes_away() {
    let fake = fake_with_account().await;
    let conn = fake.connect().await.unwrap();
    let mut signals = conn.signals();
    hello(&conn).await;
    fake.disconnect_clients().await;
    let ended = tokio::time::timeout(Duration::from_secs(5), async {
        loop {
            match signals.next().await {
                None | Some(Err(_)) => return,
                Some(Ok(_)) => {}
            }
        }
    })
    .await;
    assert!(ended.is_ok(), "the stream never ended");
    assert!(signals.next().await.is_none(), "and stays ended");
    let e = BridgeError::from(conn.proxy().get_consent().await.unwrap_err());
    assert!(matches!(e, BridgeError::Disconnected(_)), "{e:?}");
}

#[tokio::test]
async fn the_secret_is_wiped_after_the_call_whatever_happens() {
    let fake = fake_with_account().await;
    let conn = fake.connect().await.unwrap();
    hello(&conn).await;
    let none = OptsBuilder::new().build();

    let mut secret = Zeroizing::new(b"hunter2".to_vec());
    let loc = conn
        .connect_ad_hoc_wiping("sftp://host/", &mut secret, &none)
        .await
        .unwrap();
    assert_eq!(loc, "adhoc:1");
    assert!(secret.is_empty(), "wiped after success");
    assert_eq!(
        fake.received_secrets(),
        vec![b"hunter2".to_vec()],
        "the bridge did get it"
    );

    let mut secret = Zeroizing::new(b"hunter3".to_vec());
    let e = conn
        .connect_ad_hoc_wiping("sftp://u:pw@host/", &mut secret, &none)
        .await
        .unwrap_err();
    assert_eq!(e.netvfs_name(), Some("SecurityPolicy"));
    assert!(secret.is_empty(), "wiped after a refusal");

    // The caller gives up while the bridge is still working.
    fake.set_latency(Duration::from_secs(5));
    let mut secret = Zeroizing::new(b"hunter4".to_vec());
    let call = conn.connect_ad_hoc_wiping("sftp://host/", &mut secret, &none);
    assert!(tokio::time::timeout(Duration::from_millis(50), call)
        .await
        .is_err());
    assert!(secret.is_empty(), "wiped when the future is dropped");
}

#[tokio::test]
async fn keyboard_answers_are_wiped_after_the_call() {
    let fake = fake_with_account().await;
    let conn = fake.connect().await.unwrap();
    hello(&conn).await;
    let mut answers = Zeroizing::new(vec![b"123456".to_vec()]);
    // No such question: the bridge refuses, the answers are gone anyway.
    let e = conn.answer_wiping("q1", true, &mut answers).await.unwrap_err();
    assert_eq!(e.netvfs_name(), Some("NotFound"));
    assert!(answers.is_empty());
}

/// `state` and `stateError` of `loc` in `ListLocations`.
async fn state_of(conn: &lautta_bridge_proto::Connection, loc: &str) -> (String, Option<String>) {
    let locations = conn.proxy().list_locations().await.unwrap();
    let info = &locations.iter().find(|l| l.id == loc).unwrap().info;
    let text = |key: &str| info.get(key).and_then(|v| lautta_bridge_proto::value_str(v));
    (text("state").unwrap(), text("stateError"))
}

/// Waits for the next `LocationsChanged`, skipping other signals.
async fn locations_changed(signals: &mut SignalStream) {
    let wait = async {
        while !matches!(
            signals.next().await.expect("the stream stays open").unwrap(),
            Signal::LocationsChanged
        ) {}
    };
    tokio::time::timeout(Duration::from_secs(5), wait)
        .await
        .expect("LocationsChanged arrives");
}

/// The fake reports connection states as netvfs does (XB-10, XB-12):
/// disconnected until a call needs a connection, connected then,
/// disconnected after `Disconnect`; a scripted failure carries its error.
#[tokio::test]
async fn connection_states_follow_use_disconnect_and_the_script() {
    let fake = fake_with_account().await;
    let conn = fake.connect().await.unwrap();
    let mut signals = conn.signals();
    hello(&conn).await;
    let s = |state: &str, error: Option<&str>| (state.to_owned(), error.map(str::to_owned));
    assert_eq!(state_of(&conn, "account:1").await, s("disconnected", None));
    // A call that needs a connection connects it, and tells.
    conn.proxy().capabilities("account:1").await.unwrap();
    locations_changed(&mut signals).await;
    assert_eq!(state_of(&conn, "account:1").await, s("connected", None));
    conn.proxy().disconnect("account:1").await.unwrap();
    locations_changed(&mut signals).await;
    assert_eq!(state_of(&conn, "account:1").await, s("disconnected", None));
    // Scripted: a failure with its error, told once.
    fake.set_state("account:1", "failed", Some("AuthFailed")).await;
    locations_changed(&mut signals).await;
    assert_eq!(
        state_of(&conn, "account:1").await,
        s("failed", Some("AuthFailed"))
    );
    fake.set_state("account:1", "failed", Some("AuthFailed")).await;
    conn.proxy().stat("account:1", b"", true, "").await.unwrap();
    locations_changed(&mut signals).await;
    assert_eq!(state_of(&conn, "account:1").await, s("connected", None));
    // Work on a connected location tells nothing more.
    conn.proxy().stat("account:1", b"", true, "").await.unwrap();
    let quiet = tokio::time::timeout(Duration::from_millis(200), signals.next()).await;
    assert!(quiet.is_err(), "{quiet:?}");
    // A connected ad-hoc location.
    let loc = conn
        .proxy()
        .connect_ad_hoc("sftp://h/", b"", &OptsBuilder::new().build())
        .await
        .unwrap();
    assert_eq!(state_of(&conn, &loc).await, s("connected", None));
}

#[tokio::test]
async fn features_and_the_storage_handoff_need_no_consent() {
    let fake = fake_with_account().await;
    fake.set_consent(lautta_bridge_proto::fake::Consent::Denied).await;
    let conn = fake.connect().await.unwrap();
    hello(&conn).await;
    let p = conn.proxy();
    assert_eq!(p.get_features().await.unwrap(), ["storage-settings"]);
    p.open_storage_settings().await.unwrap();
    assert_eq!(fake.handoffs(), ["storage"]);

    // A bridge that offers no Storage handoff refuses it.
    fake.set_features(Some(&[]));
    assert!(p.get_features().await.unwrap().is_empty());
    let e = BridgeError::from(p.open_storage_settings().await.unwrap_err());
    assert_eq!(e.netvfs_name(), Some("Unsupported"));

    // A bridge without XB-19 knows neither method.
    fake.set_features(None);
    let e = BridgeError::from(p.get_features().await.unwrap_err());
    assert!(matches!(e, BridgeError::UnknownMethod(_)), "{e:?}");
    let e = BridgeError::from(p.open_storage_settings().await.unwrap_err());
    assert!(matches!(e, BridgeError::UnknownMethod(_)), "{e:?}");
    assert_eq!(fake.handoffs(), ["storage"]);
}

#[tokio::test]
async fn drives_are_listed_mounted_and_unmounted_without_consent() {
    // netvfs XB-20.
    use lautta_bridge_proto::fake::FakeDrive;
    use lautta_bridge_proto::{drive_info, feature, value_i64, value_str};
    let fake = fake_with_account().await;
    fake.set_consent(lautta_bridge_proto::fake::Consent::Denied).await;
    fake.set_features(Some(&[feature::STORAGE_SETTINGS, feature::DRIVES]));
    let conn = fake.connect().await.unwrap();
    hello(&conn).await;
    let mut signals = conn.signals();
    let p = conn.proxy();
    fake.set_drives(vec![
        FakeDrive::new("mmcblk1p1", "CARD", "sd", 64),
        FakeDrive::new("sda1", "", "usb", 8).with_uuid("1234-ABCD"),
    ])
    .await;
    assert!(drives_changed(&mut signals).await);
    let drives = p.list_drives().await.unwrap();
    let ids: Vec<&str> = drives.iter().map(|d| d.id.as_str()).collect();
    assert_eq!(ids, ["mmcblk1p1", "sda1"]);
    assert_eq!(drives[0].label, "CARD");
    assert!(drives[0].mount_point.is_empty());
    assert_eq!(
        value_str(&drives[0].info[drive_info::KIND]).as_deref(),
        Some("sd")
    );
    assert_eq!(value_i64(&drives[0].info[drive_info::SIZE]), Some(64));
    assert!(!drives[0].info.contains_key(drive_info::UUID));
    assert_eq!(
        value_str(&drives[1].info[drive_info::UUID]).as_deref(),
        Some("1234-ABCD")
    );

    assert_eq!(p.mount("mmcblk1p1").await.unwrap(), b"/run/media/user/CARD");
    assert!(drives_changed(&mut signals).await);
    // Mounted already: the same place.
    assert_eq!(p.mount("mmcblk1p1").await.unwrap(), b"/run/media/user/CARD");
    // Without a label, by UUID.
    assert_eq!(p.mount("sda1").await.unwrap(), b"/run/media/user/1234-ABCD");
    p.unmount("mmcblk1p1").await.unwrap();
    p.unmount("mmcblk1p1").await.unwrap();
    assert!(fake.drives()[0].mount_point.is_empty());

    let e = BridgeError::from(p.mount("sdb1").await.unwrap_err());
    assert_eq!(e.netvfs_name(), Some("NotFound"));
    let e = BridgeError::from(p.unmount("../sda1").await.unwrap_err());
    assert!(matches!(e, BridgeError::InvalidArgs(_)), "{e:?}");

    // With a mount root the folder comes and goes.
    let media = tempfile::tempdir().unwrap();
    fake.set_mount_root(Some(media.path()));
    let point = p.mount("mmcblk1p1").await.unwrap();
    assert_eq!(point, media.path().join("CARD").as_os_str().as_encoded_bytes());
    assert!(media.path().join("CARD").is_dir());
    p.unmount("mmcblk1p1").await.unwrap();
    assert!(!media.path().join("CARD").exists());

    // A bridge without XB-20 has none of them.
    fake.set_features(Some(&[feature::STORAGE_SETTINGS]));
    let e = BridgeError::from(p.list_drives().await.unwrap_err());
    assert!(matches!(e, BridgeError::UnknownMethod(_)), "{e:?}");
    let e = BridgeError::from(p.mount("sda1").await.unwrap_err());
    assert!(matches!(e, BridgeError::UnknownMethod(_)), "{e:?}");
}

/// Whether a `DrivesChanged` arrives within a second.
async fn drives_changed(signals: &mut SignalStream) -> bool {
    tokio::time::timeout(Duration::from_secs(1), async {
        while let Some(signal) = signals.next().await {
            if matches!(signal, Ok(Signal::DrivesChanged)) {
                return true;
            }
        }
        false
    })
    .await
    .unwrap_or(false)
}
