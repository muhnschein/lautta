// SPDX-License-Identifier: LGPL-2.1-or-later
//! Orderings that must not confuse the client: answers that arrive after the
//! situation has moved on (SPEC NVB-3, NVB-9).

mod bridge_support;

use bridge_support::{eventually, Rig};
use lautta_bridge_proto::fake::Consent;
use lautta_core::bridge::BridgeStatus;
use lautta_core::provider::{no_progress, Provider, WriteOptions};
use lautta_core::vpath::VPath;
use std::fs::File;
use std::io::Write;
use std::time::Duration;

#[tokio::test]
async fn a_late_location_list_does_not_resurrect_a_revoked_consent() {
    let rig = Rig::ready().await;
    rig.fake.set_consent(Consent::Denied).await;
    rig.client.wait_status(|s| s == BridgeStatus::ConsentDenied).await;

    // The user allows and takes it back at once, while ListLocations is slow.
    rig.fake.set_latency(Duration::from_millis(300));
    rig.fake.set_consent(Consent::Granted).await;
    rig.fake.set_consent(Consent::Denied).await;
    tokio::time::sleep(Duration::from_millis(700)).await;
    assert_eq!(rig.client.status(), BridgeStatus::ConsentDenied);
    assert!(
        rig.client.locations().is_empty(),
        "nothing is shown without consent"
    );
}

/// How many descriptors of this process refer to the pipe with `inode`.
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
        tokio::time::sleep(Duration::from_millis(150)).await;
        // Left are the writer's end and the bridge's reader (a message of the fake may hold
        // one more for a moment). A copy kept by the app would stay for the whole job and
        // would hide a stalled reader from the writer (NVB-9).
        eventually("the app's copy to be closed", || descriptors_on_pipe(inode) <= 2).await;
        File::from(writer).write_all(b"abc").unwrap();
    };
    let (result, ()) = tokio::join!(upload, check);
    result.unwrap();
    assert_eq!(rig.fake.file("account:1", b"held.bin").unwrap(), b"abc");
}
