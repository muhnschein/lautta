// SPDX-License-Identifier: LGPL-2.1-or-later

mod bridge_support;

use bridge_support::{eventually, Rig};
use lautta_bridge_proto::fake::Consent;
use lautta_core::bridge::BridgeStatus;
use lautta_core::provider::{no_progress, Lane, Provider, WriteOptions};
use lautta_core::vpath::VPath;
use std::fs::File;
use std::io::Write;
use std::time::Duration;

#[tokio::test]
async fn a_late_location_list_does_not_resurrect_a_revoked_consent() {
    let rig = Rig::ready().await;
    rig.fake.set_consent(Consent::Denied).await;
    rig.client.wait_status(|s| s == BridgeStatus::ConsentDenied).await;

    rig.fake.set_latency(Duration::from_millis(300));
    let replies = rig.fake.location_replies();
    let asked = rig.fake.calls_of("ListLocations").len();
    rig.fake.set_consent(Consent::Granted).await;
    // The bridge has made its list (with the account) and is slow to answer.
    eventually("the list asked for", || {
        rig.fake.calls_of("ListLocations").len() > asked
    })
    .await;
    rig.fake.set_consent(Consent::Denied).await;
    eventually("the late list", || rig.fake.location_replies() > replies).await;
    rig.fake.set_latency(Duration::ZERO);
    // One more round trip: the late answer came before it on the same connection.
    let _ = rig
        .provider()
        .stat(&VPath::root(), false, Lane::Interactive)
        .await;
    assert_eq!(rig.client.status(), BridgeStatus::ConsentDenied);
    assert!(
        rig.client.locations().is_empty(),
        "nothing is shown without consent"
    );
}

fn descriptors_on_pipe(inode: u64) -> usize {
    let want = format!("pipe:[{inode}]");
    std::fs::read_dir("/proc/self/fd")
        .unwrap()
        .filter_map(|e| std::fs::read_link(e.ok()?.path()).ok())
        .filter(|target| target.to_string_lossy() == want)
        .count()
}

#[tokio::test]
async fn the_apps_copy_of_the_descriptor_is_closed_once_the_bridge_has_it() {
    let rig = Rig::ready().await;
    let (reader, writer) = rustix::pipe::pipe().unwrap();
    let inode = rustix::fs::fstat(&reader).unwrap().st_ino;
    let provider = rig.provider();
    let opts = WriteOptions {
        size: Some(100),
        ..WriteOptions::default()
    };
    let target = VPath::parse(b"held.bin").unwrap();
    let upload = provider.upload_from(reader, &target, opts, no_progress());
    let check = async {
        eventually("the job to start", || !rig.fake.calls_of("Upload").is_empty()).await;
        // Left: writer's end and bridge's reader (a fake message may hold one more briefly). An app
        // copy would hide a stalled reader from the writer.
        eventually("the app's copy to be closed", || descriptors_on_pipe(inode) <= 2).await;
        File::from(writer).write_all(b"abc").unwrap();
    };
    let (result, ()) = tokio::join!(upload, check);
    result.unwrap();
    assert_eq!(rig.fake.file("account:1", b"held.bin").unwrap(), b"abc");
}
