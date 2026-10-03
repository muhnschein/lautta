// SPDX-License-Identifier: LGPL-2.1-or-later
//! Handshake, consent, locations, attention, discovery and handoff of the
//! bridge client against the fake bridge (SPEC NVB-1..6, LOC-6, LOC-7).

mod bridge_support;

use bridge_support::{eventually, Rig, ACCOUNT};
use lautta_bridge_proto::fake::{Consent, FakeBridge};
use lautta_bridge_proto::WireNearby;
use lautta_core::bridge::{AdHocOptions, Attention, BridgeStatus, RemoteKind};
use lautta_core::error::ErrorKind;
use lautta_core::provider::{Lane, Provider};
use lautta_core::vpath::VPath;
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
    // Without the socket every call says that the bridge is not there.
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
    assert!(rig.client.features().contains(&"error-details".to_owned()));
    assert_eq!(rig.client.consent(), Some(lautta_core::bridge::Consent::Granted));
}

#[tokio::test]
async fn a_bridge_with_an_older_protocol_keeps_locations_hidden() {
    let fake = FakeBridge::new();
    fake.add_account(1, "fake", "Fake 1", "fake.example").await;
    fake.set_hello(0, "0.0.1");
    let rig = Rig::with_fake(fake, true, |c| c).await;
    rig.client.wait_status(|s| s == BridgeStatus::TooOld).await;
    assert!(rig.client.locations().is_empty());
    let e = rig
        .provider()
        .stat(&VPath::root(), true, Lane::Interactive)
        .await
        .unwrap_err();
    assert_eq!(e.kind, ErrorKind::BridgeUnavailable);
    // netvfs gets updated; a poke finds the new protocol.
    rig.fake.set_hello(1, "0.2.0");
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
    // The first Hello made the bridge ask (netvfs XB-6).
    assert_eq!(rig.fake.consent_requests(), 1);
    assert!(rig.client.locations().is_empty());
    let e = rig
        .provider()
        .stat(&VPath::root(), true, Lane::Interactive)
        .await
        .unwrap_err();
    assert_eq!(e.kind, ErrorKind::PermissionDenied);

    // *Ask again* (NVB-3).
    rig.client.request_consent().await.unwrap();
    assert_eq!(rig.fake.consent_requests(), 2);

    // The user taps *Allow* in the notification.
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
        path: b"share".to_vec(),
    };
    // Nothing is delivered before the app asks (LOC-6).
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
    assert_eq!(n[0].path.as_bytes(), b"share");

    rig.fake.set_nearby(vec![]).await;
    eventually("the list to empty", || rig.client.nearby().is_empty()).await;

    rig.client.discover(false).await.unwrap();
    rig.fake.set_nearby(vec![server]).await;
    tokio::time::sleep(Duration::from_millis(50)).await;
    assert!(rig.client.nearby().is_empty(), "stopped discovery stays quiet");
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

    rig.client.disconnect(&id).await.unwrap();
    rig.client.forget_adhoc(&id).await.unwrap();
    eventually("the removal", || rig.client.location(&id).is_none()).await;
    let e = rig.client.forget_adhoc(&id).await.unwrap_err();
    assert_eq!(e.kind, ErrorKind::NotFound);
    // Accounts are not the app's to forget.
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
