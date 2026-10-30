// SPDX-License-Identifier: LGPL-2.1-or-later

mod bridge_support;

use bridge_support::{eventually, quick_backoff, Rig, ACCOUNT};
use lautta_bridge_proto::fake::{Consent, FakeBridge, FakeDrive};
use lautta_bridge_proto::WireNearby;
use lautta_core::bridge::{
    feature, AdHocOptions, Attention, BridgeConfig, BridgeStatus, DriveKind, RemoteKind,
};
use lautta_core::error::ErrorKind;
use lautta_core::provider::{Lane, Provider};
use lautta_core::vpath::VPath;
use std::path::Path;
use std::sync::Arc;
use std::time::Duration;
use zeroize::Zeroizing;

fn secret(bytes: &[u8]) -> Zeroizing<Vec<u8>> {
    Zeroizing::new(bytes.to_vec())
}

#[tokio::test]
async fn standalone_until_the_socket_appears() {
    let fake = FakeBridge::new();
    fake.add_account(1, "fake", "Fake 1", "fake.example").await;
    let mut rig = Rig::with_fake(fake, false, |c| c).await;
    rig.client.wait_status(|s| s == BridgeStatus::Absent).await;
    assert!(rig.client.locations().is_empty());
    let e = rig
        .provider()
        .stat(&VPath::root(), true, Lane::Interactive)
        .await
        .unwrap_err();
    assert_eq!(e.kind, ErrorKind::BridgeUnavailable);
    assert!(
        !rig.paths.bridge_socket().exists(),
        "the app never creates the socket"
    );

    rig.listen();
    rig.client.poke();
    rig.client.wait_status(|s| s == BridgeStatus::Ready).await;
    assert_eq!(rig.client.locations().len(), 1);
}

#[tokio::test]
async fn hello_names_the_app_and_reports_the_bridge() {
    let rig = Rig::ready().await;
    let hello = rig.fake.calls_of("Hello");
    assert_eq!(hello.len(), 1);
    assert!(hello[0].target.starts_with("lautta "), "{:?}", hello[0]);
    assert_eq!(rig.client.bridge_version().as_deref(), Some("0.2.0-fake"));
    assert_eq!(rig.client.consent(), Some(lautta_core::bridge::Consent::Granted));
}

#[tokio::test]
async fn any_bridge_version_is_taken_as_it_is() {
    let fake = FakeBridge::new();
    fake.add_account(1, "fake", "Fake 1", "fake.example").await;
    fake.set_hello("0.0.1");
    let rig = Rig::with_fake(fake, true, |c| c).await;
    rig.client.wait_status(|s| s == BridgeStatus::Ready).await;
    assert_eq!(rig.client.bridge_version().as_deref(), Some("0.0.1"));
    assert_eq!(rig.client.locations().len(), 1);
}

#[tokio::test]
async fn a_refused_hello_keeps_locations_hidden_until_a_poke() {
    let fake = FakeBridge::new();
    fake.add_account(1, "fake", "Fake 1", "fake.example").await;
    fake.fail_next("Hello", "ProtocolError", "not now");
    let rig = Rig::with_fake(fake, true, |c| c).await;
    eventually("the refused Hello", || rig.fake.calls_of("Hello").len() == 1).await;
    rig.client.wait_status(|s| s == BridgeStatus::Absent).await;
    assert!(rig.client.locations().is_empty());
    assert_eq!(rig.client.bridge_version(), None);
    let e = rig
        .provider()
        .stat(&VPath::root(), true, Lane::Interactive)
        .await
        .unwrap_err();
    assert_eq!(e.kind, ErrorKind::BridgeUnavailable);
    rig.client.poke();
    rig.client.wait_status(|s| s == BridgeStatus::Ready).await;
    assert_eq!(rig.client.locations().len(), 1);
}

#[tokio::test]
async fn nothing_is_shown_until_the_user_allows() {
    let fake = FakeBridge::new();
    fake.set_consent(Consent::Unknown).await;
    fake.add_account(1, "fake", "Fake 1", "fake.example").await;
    let rig = Rig::with_fake(fake, true, |c| c).await;
    rig.client
        .wait_status(|s| s == BridgeStatus::ConsentUnknown)
        .await;
    assert_eq!(rig.fake.consent_requests(), 1);
    assert!(rig.client.locations().is_empty());
    let e = rig
        .provider()
        .stat(&VPath::root(), true, Lane::Interactive)
        .await
        .unwrap_err();
    assert_eq!(e.kind, ErrorKind::PermissionDenied);

    rig.client.request_consent().await.unwrap();
    assert_eq!(rig.fake.consent_requests(), 2);

    rig.fake.set_consent(Consent::Granted).await;
    rig.client.wait_status(|s| s == BridgeStatus::Ready).await;
    eventually("the locations", || rig.client.locations().len() == 1).await;
}

#[tokio::test]
async fn denial_hides_the_servers_and_revocation_takes_them_back() {
    let fake = FakeBridge::new();
    fake.set_consent(Consent::Denied).await;
    fake.add_account(1, "fake", "Fake 1", "fake.example").await;
    let rig = Rig::with_fake(fake, true, |c| c).await;
    rig.client.wait_status(|s| s == BridgeStatus::ConsentDenied).await;
    assert!(rig.client.locations().is_empty());
    assert_eq!(
        rig.fake.consent_requests(),
        0,
        "a denial is not asked again unprompted"
    );

    rig.fake.set_consent(Consent::Granted).await;
    rig.client.wait_status(|s| s == BridgeStatus::Ready).await;
    eventually("the locations", || rig.client.locations().len() == 1).await;

    rig.fake.set_consent(Consent::Denied).await;
    rig.client.wait_status(|s| s == BridgeStatus::ConsentDenied).await;
    assert!(rig.client.locations().is_empty());
    assert_eq!(rig.client.consent(), Some(lautta_core::bridge::Consent::Denied));
}

#[tokio::test]
async fn the_bridge_can_answer_the_notification_by_itself() {
    let fake = FakeBridge::new();
    fake.set_consent(Consent::Unknown).await;
    fake.auto_consent(Some(Consent::Granted));
    let rig = Rig::with_fake(fake, true, |c| c).await;
    rig.client.wait_status(|s| s == BridgeStatus::Ready).await;
    assert_eq!(rig.fake.consent(), Consent::Granted);
}

#[tokio::test]
async fn locations_use_nv_ids_and_carry_the_info() {
    let rig = Rig::ready().await;
    let locations = rig.client.locations();
    assert_eq!(locations.len(), 1);
    let l = &locations[0];
    assert_eq!(l.id, ACCOUNT);
    assert_eq!(l.bridge_id, "account:1");
    assert_eq!((l.provider.as_str(), l.name.as_str()), ("fake", "Fake 1"));
    assert_eq!(l.kind, RemoteKind::Account);
    assert_eq!(l.host, "fake.example");
    assert_eq!(l.account_id, Some(1));
    assert_eq!(l.attention, Attention::None);
    assert_eq!(rig.client.location(ACCOUNT).unwrap().name, "Fake 1");
    assert_eq!(rig.client.location("account:1").unwrap().name, "Fake 1");
    assert!(rig.client.location("nv-account:9").is_none());
}

#[tokio::test]
async fn locations_and_attention_follow_the_bridge() {
    let rig = Rig::ready().await;
    let mut watch = rig.client.watch_locations();
    watch.borrow_and_update();

    rig.fake.add_account(2, "sftp", "NAS", "nas.local").await;
    eventually("the second account", || rig.client.locations().len() == 2).await;

    rig.fake.set_attention("account:2", Some("auth-failed")).await;
    eventually("the badge", || {
        rig.client
            .location("nv-account:2")
            .is_some_and(|l| l.attention == Attention::AuthFailed)
    })
    .await;
    rig.fake
        .set_attention("account:2", Some("server-identity-changed"))
        .await;
    eventually("the identity badge", || {
        rig.client
            .location("nv-account:2")
            .is_some_and(|l| l.attention == Attention::ServerIdentityChanged)
    })
    .await;
    rig.fake.set_attention("account:2", None).await;
    eventually("the badge going away", || {
        rig.client
            .location("nv-account:2")
            .is_some_and(|l| l.attention == Attention::None)
    })
    .await;

    rig.fake.remove_location("account:2").await;
    eventually("the removal", || rig.client.locations().len() == 1).await;
    assert!(watch.has_changed().unwrap(), "watchers are told");
}

#[tokio::test]
async fn nearby_servers_come_from_discovery() {
    let rig = Rig::ready().await;
    let server = WireNearby {
        name: "NAS".into(),
        provider: "smb".into(),
        host: "nas.local".into(),
        port: 445,
    };
    rig.fake.set_nearby(vec![server.clone()]).await;
    tokio::time::sleep(Duration::from_millis(50)).await;
    assert!(rig.client.nearby().is_empty());

    rig.client.discover(true).await.unwrap();
    eventually("the server", || rig.client.nearby().len() == 1).await;
    let n = rig.client.nearby();
    assert_eq!(
        (n[0].name.as_str(), n[0].host.as_str(), n[0].port),
        ("NAS", "nas.local", 445)
    );
    assert_eq!(n[0].provider, "smb");

    rig.fake.set_nearby(vec![]).await;
    eventually("the list to empty", || rig.client.nearby().is_empty()).await;

    rig.client.discover(false).await.unwrap();
    rig.fake.set_nearby(vec![server]).await;
    tokio::time::sleep(Duration::from_millis(50)).await;
    assert!(rig.client.nearby().is_empty(), "stopped discovery stays quiet");
}

fn nearby_server(name: &str, host: &str) -> WireNearby {
    WireNearby {
        name: name.into(),
        provider: "smb".into(),
        host: host.into(),
        port: 445,
    }
}

fn record_nearby_lengths(rig: &Rig) -> Arc<std::sync::Mutex<Vec<usize>>> {
    let seen = Arc::new(std::sync::Mutex::new(Vec::new()));
    let mut rx = rig.client.watch_nearby();
    let log = seen.clone();
    tokio::spawn(async move {
        while rx.changed().await.is_ok() {
            let len = rx.borrow_and_update().len();
            log.lock().unwrap().push(len);
        }
    });
    seen
}

#[tokio::test]
async fn nearby_servers_stay_while_a_restarted_discovery_settles() {
    let rig = Rig::ready_with(|c| BridgeConfig {
        nearby_settle: Duration::from_secs(30),
        ..c
    })
    .await;
    rig.client.discover(true).await.unwrap();
    rig.fake.set_nearby(vec![nearby_server("NAS", "nas.local")]).await;
    eventually("the server", || rig.client.nearby().len() == 1).await;

    let seen = record_nearby_lengths(&rig);
    rig.client.discover(false).await.unwrap();
    rig.client.discover(true).await.unwrap();
    tokio::time::sleep(Duration::from_millis(100)).await;
    rig.fake.set_nearby(vec![nearby_server("Mac", "mac.local")]).await;
    eventually("the second server", || rig.client.nearby().len() == 2).await;
    let names: Vec<String> = rig.client.nearby().iter().map(|n| n.name.clone()).collect();
    assert_eq!(names, ["NAS", "Mac"], "rows keep their places");
    assert!(
        !seen.lock().unwrap().contains(&0),
        "the Nearby section never empties: {:?}",
        seen.lock().unwrap()
    );
}

#[tokio::test]
async fn a_settled_discovery_drops_servers_that_did_not_answer() {
    let rig = Rig::ready().await;
    rig.client.discover(true).await.unwrap();
    rig.fake
        .set_nearby(vec![
            nearby_server("NAS", "nas.local"),
            nearby_server("Mac", "mac.local"),
        ])
        .await;
    eventually("the servers", || rig.client.nearby().len() == 2).await;

    rig.client.discover(false).await.unwrap();
    rig.client.discover(true).await.unwrap();
    rig.fake.set_nearby(vec![nearby_server("Mac", "mac.local")]).await;
    eventually("the list of the settled discovery", || {
        rig.client.nearby().iter().map(|n| n.name.as_str()).eq(["Mac"])
    })
    .await;

    rig.client.discover(false).await.unwrap();
    rig.client.discover(true).await.unwrap();
    eventually("the empty list", || rig.client.nearby().is_empty()).await;
}

#[tokio::test]
async fn account_management_is_handed_off() {
    let rig = Rig::ready().await;
    rig.client.add_account("sftp").await.unwrap();
    rig.client.open_account_settings(ACCOUNT).await.unwrap();
    assert_eq!(
        rig.fake.handoffs(),
        vec!["add:sftp".to_owned(), "settings:account:1".to_owned()]
    );

    let e = rig
        .client
        .open_account_settings("nv-account:9")
        .await
        .unwrap_err();
    assert_eq!(e.kind, ErrorKind::NotFound);
    let e = rig.client.add_account("Not A Token").await.unwrap_err();
    assert_eq!(e.kind, ErrorKind::InvalidArgument);
}

#[tokio::test]
async fn the_bridge_opens_storage_settings_whatever_the_consent() {
    let fake = FakeBridge::new();
    fake.set_consent(Consent::Denied).await;
    let rig = Rig::with_fake(fake, true, |c| c).await;
    rig.client.wait_status(|s| s == BridgeStatus::ConsentDenied).await;
    let order: Vec<String> = rig.fake.calls().into_iter().map(|c| c.method).collect();
    assert_eq!(order, ["Hello", "GetFeatures", "GetConsent"]);
    assert!(rig.client.offers(feature::STORAGE_SETTINGS));
    rig.client.open_storage_settings().await.unwrap();
    assert_eq!(rig.fake.handoffs(), ["storage"]);
}

#[tokio::test]
async fn a_bridge_without_features_works_as_before() {
    let fake = FakeBridge::new();
    fake.add_account(1, "fake", "Fake 1", "fake.example").await;
    fake.set_features(None);
    let rig = Rig::with_fake(fake, true, |c| c).await;
    rig.client.wait_status(|s| s == BridgeStatus::Ready).await;
    assert_eq!(rig.client.locations().len(), 1);
    assert!(!rig.client.offers(feature::STORAGE_SETTINGS));
    let e = rig.client.open_storage_settings().await.unwrap_err();
    assert_eq!(e.kind, ErrorKind::Unsupported);
    assert!(rig.fake.handoffs().is_empty());
}

#[tokio::test]
async fn features_that_fail_are_none_and_unknown_ones_are_ignored() {
    let fake = FakeBridge::new();
    fake.add_account(1, "fake", "Fake 1", "fake.example").await;
    fake.fail_next("GetFeatures", "Internal", "no");
    let rig = Rig::with_fake(fake, true, |c| c).await;
    rig.client.wait_status(|s| s == BridgeStatus::Ready).await;
    assert!(!rig.client.offers(feature::STORAGE_SETTINGS));

    let fake = FakeBridge::new();
    fake.set_features(Some(&["teleport", feature::STORAGE_SETTINGS]));
    let rig = Rig::with_fake(fake, true, |c| c).await;
    rig.client.wait_status(|s| s == BridgeStatus::Ready).await;
    assert!(rig.client.offers(feature::STORAGE_SETTINGS));
    assert!(!rig.client.offers("drives"));
}

#[tokio::test]
async fn features_are_asked_again_on_every_connection() {
    let rig = Rig::ready_with(quick_backoff).await;
    assert!(rig.client.offers(feature::STORAGE_SETTINGS));
    rig.fake.set_accepting(false);
    rig.fake.disconnect_clients().await;
    rig.client.wait_status(|s| s == BridgeStatus::Reconnecting).await;
    assert!(!rig.client.offers(feature::STORAGE_SETTINGS));

    rig.fake.set_features(Some(&[]));
    rig.fake.set_accepting(true);
    eventually("the second GetFeatures", || {
        rig.fake.calls_of("GetFeatures").len() == 2
    })
    .await;
    rig.client.wait_status(|s| s == BridgeStatus::Ready).await;
    assert!(!rig.client.offers(feature::STORAGE_SETTINGS));
}

async fn drives_fake() -> FakeBridge {
    let fake = FakeBridge::new();
    fake.set_features(Some(&[feature::STORAGE_SETTINGS, feature::DRIVES]));
    fake.set_drives(vec![FakeDrive::new("mmcblk1p1", "CARD", "sd", 64)])
        .await;
    fake
}

#[tokio::test]
async fn drives_need_no_consent_and_follow_the_bridge() {
    let fake = drives_fake().await;
    fake.set_consent(Consent::Denied).await;
    let rig = Rig::with_fake(fake, true, |c| c).await;
    rig.client.wait_status(|s| s == BridgeStatus::ConsentDenied).await;
    eventually("the card", || rig.client.drives().len() == 1).await;
    let card = rig.client.drives()[0].clone();
    assert_eq!(
        (card.id.as_str(), card.label.as_str(), card.kind, card.size),
        ("mmcblk1p1", "CARD", DriveKind::Card, 64)
    );
    assert_eq!(card.mount_point, None);
    let mut watch = rig.client.watch_drives();
    watch.borrow_and_update();

    rig.fake
        .set_drives(vec![
            FakeDrive::new("mmcblk1p1", "CARD", "sd", 64),
            FakeDrive::new("sda1", "STICK", "usb", 8),
        ])
        .await;
    eventually("the stick", || rig.client.drives().len() == 2).await;
    assert_eq!(rig.client.drives()[1].kind, DriveKind::Usb);
    assert!(watch.has_changed().unwrap(), "watchers are told");

    let point = rig.client.mount_drive("mmcblk1p1").await.unwrap();
    assert_eq!(point, Path::new("/run/media/user/CARD"));
    eventually("the mount", || {
        rig.client.drives()[0].mount_point.as_deref() == Some(point.as_path())
    })
    .await;
    rig.client.unmount_drive("mmcblk1p1").await.unwrap();
    eventually("the unmount", || rig.client.drives()[0].mount_point.is_none()).await;
    let e = rig.client.mount_drive("sdb1").await.unwrap_err();
    assert_eq!(e.kind, ErrorKind::NotFound);
}

#[tokio::test]
async fn drives_go_with_the_connection() {
    let rig = Rig::with_fake(drives_fake().await, true, quick_backoff).await;
    rig.client.wait_status(|s| s == BridgeStatus::Ready).await;
    eventually("the card", || rig.client.drives().len() == 1).await;
    rig.fake.set_accepting(false);
    rig.fake.disconnect_clients().await;
    rig.client.wait_status(|s| s == BridgeStatus::Reconnecting).await;
    assert!(rig.client.drives().is_empty(), "nothing mounts them now");
    let e = rig.client.mount_drive("mmcblk1p1").await.unwrap_err();
    assert_eq!(e.kind, ErrorKind::ConnectionLost);

    rig.fake.set_accepting(true);
    eventually("the card again", || rig.client.drives().len() == 1).await;
}

#[tokio::test]
async fn an_older_bridge_is_not_asked_for_drives() {
    for features in [Some(&[feature::STORAGE_SETTINGS][..]), None] {
        let fake = FakeBridge::new();
        fake.add_account(1, "fake", "Fake 1", "fake.example").await;
        fake.set_features(features);
        let rig = Rig::with_fake(fake, true, |c| c).await;
        rig.client.wait_status(|s| s == BridgeStatus::Ready).await;
        eventually("the account", || rig.client.locations().len() == 1).await;
        rig.fake
            .set_drives(vec![FakeDrive::new("sda1", "STICK", "usb", 8)])
            .await;
        tokio::time::sleep(Duration::from_millis(50)).await;
        assert!(rig.fake.calls_of("ListDrives").is_empty(), "{features:?}");
        assert!(rig.client.drives().is_empty());
        let e = rig.client.mount_drive("sda1").await.unwrap_err();
        assert_eq!(e.kind, ErrorKind::Unsupported, "{features:?}");
    }
}

#[tokio::test]
async fn adhoc_servers_are_connected_and_forgotten() {
    let rig = Rig::ready().await;
    let opts = AdHocOptions {
        user: Some("me".into()),
        ..AdHocOptions::default()
    };
    let id = rig
        .client
        .connect_adhoc("sftp://host.example:2222/docs", secret(b"pw"), opts)
        .await
        .unwrap();
    assert_eq!(id, "nv-adhoc:1");
    assert_eq!(rig.fake.received_secrets(), vec![b"pw".to_vec()]);
    eventually("the ad-hoc server in the list", || {
        rig.client.location(&id).is_some()
    })
    .await;
    let l = rig.client.location(&id).unwrap();
    assert_eq!(l.kind, RemoteKind::AdHoc);
    assert_eq!(
        (l.host.as_str(), l.port, l.user.as_deref()),
        ("host.example", Some(2222), Some("me"))
    );
    assert_eq!(l.provider, "sftp");
    assert_eq!(l.start_path.as_bytes(), b"docs");

    rig.client.forget_adhoc(&id).await.unwrap();
    eventually("the removal", || rig.client.location(&id).is_none()).await;
    let e = rig.client.forget_adhoc(&id).await.unwrap_err();
    assert_eq!(e.kind, ErrorKind::NotFound);
    let e = rig.client.forget_adhoc(ACCOUNT).await.unwrap_err();
    assert_eq!(e.kind, ErrorKind::NotFound);
}

#[tokio::test]
async fn adhoc_refusals_keep_their_names() {
    let rig = Rig::ready().await;
    let none = AdHocOptions::default();
    let e = rig
        .client
        .connect_adhoc("sftp://u:pw@host/", secret(b""), none.clone())
        .await
        .unwrap_err();
    assert_eq!(
        e.kind,
        ErrorKind::SecurityPolicy,
        "a password in the URL is refused"
    );
    let e = rig
        .client
        .connect_adhoc("file:///home/user", secret(b""), none.clone())
        .await
        .unwrap_err();
    assert_eq!(
        e.kind,
        ErrorKind::PermissionDenied,
        "the bridge never touches local files"
    );
    let e = rig
        .client
        .connect_adhoc("gopher://host/", secret(b""), none.clone())
        .await
        .unwrap_err();
    assert_eq!(e.kind, ErrorKind::Unsupported);
    let profile = AdHocOptions {
        security_profile: Some("legacy".into()),
        ..AdHocOptions::default()
    };
    let e = rig
        .client
        .connect_adhoc("sftp://host/", secret(b""), profile)
        .await
        .unwrap_err();
    assert_eq!(e.kind, ErrorKind::InvalidArgument, "a profile is for SMB only");
    assert!(rig.client.locations().len() == 1);
}

#[tokio::test]
async fn only_sftp_and_smb_urls_connect() {
    let rig = Rig::ready().await;
    for url in ["dav://h/", "davs://h/", "ftp://h/", "ftps://h/", "https://h/"] {
        let e = rig
            .client
            .connect_adhoc(url, secret(b"pw"), AdHocOptions::default())
            .await
            .unwrap_err();
        assert_eq!(e.kind, ErrorKind::Unsupported, "{url}");
    }
    let e = rig
        .client
        .connect_adhoc("davs://h/", secret(b""), AdHocOptions::default())
        .await
        .unwrap_err();
    assert_eq!(e.message, "davs URLs are no longer supported");
    assert_eq!(rig.client.locations().len(), 1, "nothing was added");
    for url in ["sftp://h/", "smb://h/share"] {
        let id = rig
            .client
            .connect_adhoc(url, secret(b""), AdHocOptions::default())
            .await
            .unwrap();
        eventually("the ad-hoc server in the list", || {
            rig.client.location(&id).is_some()
        })
        .await;
    }
}

#[tokio::test]
async fn foreground_changes_do_not_disturb_a_working_connection() {
    let rig = Rig::ready().await;
    rig.client.set_foreground(false);
    rig.client.set_foreground(true);
    rig.client.poke();
    tokio::time::sleep(Duration::from_millis(50)).await;
    assert_eq!(rig.client.status(), BridgeStatus::Ready);
    assert_eq!(rig.fake.calls_of("Hello").len(), 1, "no second connection");
}

#[tokio::test]
async fn shutdown_ends_the_connection() {
    let rig = Rig::ready().await;
    rig.client.shutdown();
    assert_eq!(rig.client.status(), BridgeStatus::Absent);
    let e = rig
        .provider()
        .stat(&VPath::root(), true, Lane::Interactive)
        .await
        .unwrap_err();
    assert_eq!(e.kind, ErrorKind::BridgeUnavailable);
}
