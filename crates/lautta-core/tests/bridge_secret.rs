// SPDX-License-Identifier: LGPL-2.1-or-later
//! Ad-hoc secrets live in a zeroized buffer for the duration of one call
//! (SPEC NVB-6, SEC-1).

mod bridge_support;

use bridge_support::Rig;
use lautta_bridge_proto::fake::FakeBridge;
use lautta_core::bridge::{AdHocOptions, BridgeStatus};
use lautta_core::error::ErrorKind;
use std::time::Duration;
use zeroize::Zeroizing;

#[tokio::test]
async fn the_buffer_is_empty_after_a_successful_call() {
    let rig = Rig::ready().await;
    let mut secret = Zeroizing::new(b"hunter2".to_vec());
    let id = rig
        .client
        .connect_adhoc_wiping("sftp://host/", &mut secret, &AdHocOptions::default())
        .await
        .unwrap();
    assert_eq!(id, "nv-adhoc:1");
    assert!(secret.is_empty());
    assert_eq!(
        rig.fake.received_secrets(),
        vec![b"hunter2".to_vec()],
        "the bridge did get it"
    );
}

#[tokio::test]
async fn the_buffer_is_empty_after_a_refusal() {
    let rig = Rig::ready().await;
    let mut secret = Zeroizing::new(b"hunter3".to_vec());
    let e = rig
        .client
        .connect_adhoc_wiping("sftp://user:pw@host/", &mut secret, &AdHocOptions::default())
        .await
        .unwrap_err();
    assert_eq!(e.kind, ErrorKind::SecurityPolicy);
    assert!(secret.is_empty());
}

#[tokio::test]
async fn the_buffer_is_empty_when_the_bridge_is_not_there() {
    let fake = FakeBridge::new();
    let rig = Rig::with_fake(fake, false, |c| c).await;
    rig.client.wait_status(|s| s == BridgeStatus::Absent).await;
    let mut secret = Zeroizing::new(b"hunter4".to_vec());
    let e = rig
        .client
        .connect_adhoc_wiping("sftp://host/", &mut secret, &AdHocOptions::default())
        .await
        .unwrap_err();
    assert_eq!(e.kind, ErrorKind::BridgeUnavailable);
    assert!(secret.is_empty(), "never sent, still wiped");
    assert!(rig.fake.received_secrets().is_empty());
}

#[tokio::test]
async fn the_buffer_is_empty_when_the_caller_gives_up() {
    let rig = Rig::ready().await;
    rig.fake.set_latency(Duration::from_secs(5));
    let mut secret = Zeroizing::new(b"hunter5".to_vec());
    let opts = AdHocOptions::default();
    let call = rig
        .client
        .connect_adhoc_wiping("sftp://host/", &mut secret, &opts);
    assert!(tokio::time::timeout(Duration::from_millis(100), call)
        .await
        .is_err());
    assert!(secret.is_empty());
}

#[tokio::test]
async fn the_owned_form_takes_the_secret_and_works() {
    let rig = Rig::ready().await;
    let id = rig
        .client
        .connect_adhoc(
            "sftp://host/",
            Zeroizing::new(b"hunter6".to_vec()),
            AdHocOptions::default(),
        )
        .await
        .unwrap();
    assert_eq!(id, "nv-adhoc:1");
    assert_eq!(rig.fake.received_secrets(), vec![b"hunter6".to_vec()]);
}

#[tokio::test]
async fn the_app_never_keeps_a_secret_in_its_state() {
    // After the call the client holds nothing: the only copy is the bridge's.
    let rig = Rig::ready().await;
    let mut secret = Zeroizing::new(b"needle-in-a-haystack".to_vec());
    rig.client
        .connect_adhoc_wiping("sftp://host/", &mut secret, &AdHocOptions::default())
        .await
        .unwrap();
    let shown = format!("{:?}{:?}", rig.client.locations(), rig.questions.pending());
    assert!(!shown.contains("needle"));
}
