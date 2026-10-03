// SPDX-License-Identifier: LGPL-2.1-or-later
//! Uploads, downloads and jobs through file descriptors (SPEC NVB-9..11,
//! XFR-12): the app opens the local side, the bridge only gets the fd.

mod bridge_support;

use bridge_support::{eventually, Rig};
use lautta_bridge_proto::fake::JobFailure;
use lautta_core::error::ErrorKind;
use lautta_core::provider::{
    no_progress, Disposition, Lane, ProgressSink, Provider, ReadOptions, WriteOptions,
};
use lautta_core::vpath::VPath;
use std::fs::{File, OpenOptions};
use std::io::{Read, Write};
use std::os::fd::OwnedFd;
use std::path::Path;
use std::sync::{Arc, Mutex};
use std::time::{Duration, UNIX_EPOCH};

const DATA: &[u8] = b"0123456789";

fn p(s: &str) -> VPath {
    VPath::parse(s.as_bytes()).unwrap()
}

fn read_only(path: &Path) -> OwnedFd {
    File::open(path).unwrap().into()
}

fn create_rw(path: &Path) -> OwnedFd {
    OpenOptions::new()
        .read(true)
        .write(true)
        .create_new(true)
        .open(path)
        .unwrap()
        .into()
}

type Seen = Arc<Mutex<Vec<(u64, Option<u64>)>>>;

fn recorder() -> (ProgressSink, Seen) {
    let seen: Seen = Arc::default();
    let sink = seen.clone();
    (
        Arc::new(move |done, total| sink.lock().unwrap().push((done, total))),
        seen,
    )
}

async fn local_source(rig: &Rig, name: &str, data: &[u8]) -> std::path::PathBuf {
    let path = rig.home.path().join(name);
    std::fs::write(&path, data).unwrap();
    path
}

#[tokio::test]
async fn an_upload_sends_the_file_through_its_descriptor() {
    let rig = Rig::ready().await;
    rig.fake.set_chunk_size(4);
    let src = local_source(&rig, "src.bin", DATA).await;
    let (sink, seen) = recorder();
    rig.provider()
        .upload_from(read_only(&src), &p("dst.bin"), WriteOptions::default(), sink)
        .await
        .unwrap();
    assert_eq!(rig.fake.file("account:1", b"dst.bin").unwrap(), DATA);
    let seen = seen.lock().unwrap().clone();
    assert!(seen.len() >= 3, "progress after every step: {seen:?}");
    assert!(
        seen.windows(2).all(|w| w[0].0 <= w[1].0),
        "never goes back: {seen:?}"
    );
    assert_eq!(seen.last(), Some(&(10, Some(10))));
    assert_eq!(rig.fake.calls_of("Upload")[0].lane, "bulk");
}

#[tokio::test]
async fn dispositions_decide_what_happens_to_an_existing_file() {
    let rig = Rig::ready().await;
    rig.fake.put_file("account:1", b"dst.bin", b"old old old old");
    let src = local_source(&rig, "src.bin", DATA).await;
    let provider = rig.provider();

    let e = provider
        .upload_from(
            read_only(&src),
            &p("dst.bin"),
            WriteOptions::default(),
            no_progress(),
        )
        .await
        .unwrap_err();
    assert_eq!(e.kind, ErrorKind::AlreadyExists);
    assert_eq!(
        rig.fake.file("account:1", b"dst.bin").unwrap(),
        b"old old old old",
        "nothing was written"
    );

    let truncate = WriteOptions {
        disposition: Disposition::Truncate,
        modified: Some(UNIX_EPOCH + Duration::from_millis(7_000)),
        ..WriteOptions::default()
    };
    provider
        .upload_from(read_only(&src), &p("dst.bin"), truncate, no_progress())
        .await
        .unwrap();
    assert_eq!(rig.fake.file("account:1", b"dst.bin").unwrap(), DATA);
    let e = provider
        .stat(&p("dst.bin"), true, Lane::Interactive)
        .await
        .unwrap();
    assert_eq!(e.modified, Some(UNIX_EPOCH + Duration::from_millis(7_000)));
}

#[tokio::test]
async fn an_interrupted_upload_resumes_at_the_offset() {
    let rig = Rig::ready().await;
    rig.fake.set_chunk_size(2);
    rig.fake.fail_job(
        JobFailure::new("Upload", 4, "ConnectionLost", "link dropped").detail("reset by peer", Some(1500)),
    );
    let src = local_source(&rig, "src.bin", DATA).await;
    let provider = rig.provider();

    let e = provider
        .upload_from(
            read_only(&src),
            &p("part.bin"),
            WriteOptions::default(),
            no_progress(),
        )
        .await
        .unwrap_err();
    assert_eq!(e.kind, ErrorKind::ConnectionLost);
    assert_eq!(e.message, "link dropped");
    assert_eq!(e.detail.as_deref(), Some("reset by peer"));
    assert_eq!(e.retry_after_ms, Some(1500));
    assert!(e.kind.is_transient());
    assert_eq!(
        rig.fake.file("account:1", b"part.bin").unwrap(),
        b"0123",
        "the partial file stays for the resume"
    );

    let resume = WriteOptions {
        disposition: Disposition::Resume,
        offset: 4,
        size: Some(10),
        ..WriteOptions::default()
    };
    let (sink, seen) = recorder();
    provider
        .upload_from(read_only(&src), &p("part.bin"), resume, sink)
        .await
        .unwrap();
    assert_eq!(rig.fake.file("account:1", b"part.bin").unwrap(), DATA);
    // The bridge counts from the offset: 6 bytes were left to send.
    assert_eq!(seen.lock().unwrap().last(), Some(&(6, Some(6))));
}

#[tokio::test]
async fn a_resume_must_match_the_destination() {
    let rig = Rig::ready().await;
    rig.fake.put_file("account:1", b"part.bin", b"0123");
    let src = local_source(&rig, "src.bin", DATA).await;
    let provider = rig.provider();
    let at = |offset| WriteOptions {
        disposition: Disposition::Resume,
        offset,
        ..WriteOptions::default()
    };
    let e = provider
        .upload_from(read_only(&src), &p("part.bin"), at(3), no_progress())
        .await
        .unwrap_err();
    assert_eq!(e.kind, ErrorKind::InvalidArgument);
    let e = provider
        .upload_from(read_only(&src), &p("missing.bin"), at(4), no_progress())
        .await
        .unwrap_err();
    assert_eq!(e.kind, ErrorKind::InvalidArgument);
    assert_eq!(rig.fake.file("account:1", b"part.bin").unwrap(), b"0123");
}

#[tokio::test]
async fn resuming_at_offset_zero_is_a_plain_write() {
    let rig = Rig::ready().await;
    let src = local_source(&rig, "src.bin", DATA).await;
    let resume = WriteOptions {
        disposition: Disposition::Resume,
        ..WriteOptions::default()
    };
    rig.provider()
        .upload_from(read_only(&src), &p("new.bin"), resume, no_progress())
        .await
        .unwrap();
    assert_eq!(rig.fake.file("account:1", b"new.bin").unwrap(), DATA);
}

#[tokio::test]
async fn a_download_writes_into_the_descriptor() {
    let rig = Rig::ready().await;
    rig.fake.set_chunk_size(3);
    rig.fake.put_file("account:1", b"remote.bin", DATA);
    let dst = rig.home.path().join("dst.bin");
    let (sink, seen) = recorder();
    rig.provider()
        .download_into(&p("remote.bin"), create_rw(&dst), ReadOptions::default(), sink)
        .await
        .unwrap();
    assert_eq!(std::fs::read(&dst).unwrap(), DATA);
    assert_eq!(seen.lock().unwrap().last(), Some(&(10, Some(10))));
    assert_eq!(rig.fake.calls_of("Download")[0].lane, "bulk");
}

#[tokio::test]
async fn an_interrupted_download_resumes_at_the_offset() {
    let rig = Rig::ready().await;
    rig.fake.set_chunk_size(2);
    rig.fake.put_file("account:1", b"remote.bin", DATA);
    rig.fake.fail_job(JobFailure::new(
        "Download",
        4,
        "NetworkUnreachable",
        "wifi went away",
    ));
    let dst = rig.home.path().join("dst.bin");
    let provider = rig.provider();
    let e = provider
        .download_into(
            &p("remote.bin"),
            create_rw(&dst),
            ReadOptions::default(),
            no_progress(),
        )
        .await
        .unwrap_err();
    assert_eq!(e.kind, ErrorKind::NetworkUnreachable);
    assert_eq!(std::fs::read(&dst).unwrap(), b"0123");

    // Resume into the same file at the offset; the app keeps its own fd (NVB-9).
    let again: OwnedFd = OpenOptions::new().write(true).open(&dst).unwrap().into();
    provider
        .download_into(&p("remote.bin"), again, ReadOptions { offset: 4 }, no_progress())
        .await
        .unwrap();
    assert_eq!(std::fs::read(&dst).unwrap(), DATA);
}

#[tokio::test]
async fn download_failures() {
    let rig = Rig::ready().await;
    rig.fake.put_file("account:1", b"remote.bin", DATA);
    rig.fake.put_dir("account:1", b"dir");
    let provider = rig.provider();
    let dst = rig.home.path().join("dst.bin");
    let e = provider
        .download_into(
            &p("missing"),
            create_rw(&dst),
            ReadOptions::default(),
            no_progress(),
        )
        .await
        .unwrap_err();
    assert_eq!(e.kind, ErrorKind::NotFound);
    let e = provider
        .download_into(
            &p("dir"),
            create_rw(&dst.with_extension("2")),
            ReadOptions::default(),
            no_progress(),
        )
        .await
        .unwrap_err();
    assert_eq!(e.kind, ErrorKind::IsADirectory);
    // A descriptor opened read-only cannot be written: refused, not retried.
    let e = provider
        .download_into(
            &p("remote.bin"),
            read_only(&dst),
            ReadOptions::default(),
            no_progress(),
        )
        .await
        .unwrap_err();
    assert_eq!(e.kind, ErrorKind::PermissionDenied);
}

#[tokio::test]
async fn upload_refuses_descriptors_that_are_not_files() {
    let rig = Rig::ready().await;
    let dir: OwnedFd = File::open(rig.home.path()).unwrap().into();
    let e = rig
        .provider()
        .upload_from(dir, &p("x"), WriteOptions::default(), no_progress())
        .await
        .unwrap_err();
    assert_eq!(e.kind, ErrorKind::PermissionDenied);
    assert!(!rig.fake.exists("account:1", b"x"));
}

#[tokio::test]
async fn a_pipe_streams_into_an_upload_with_its_length() {
    let rig = Rig::ready().await;
    rig.fake.set_chunk_size(4);
    let (reader, writer) = rustix::pipe::pipe().unwrap();
    let feeder = std::thread::spawn(move || {
        let mut w = File::from(writer);
        for piece in DATA.chunks(3) {
            w.write_all(piece).unwrap();
        }
    });
    let opts = WriteOptions {
        size: Some(10),
        ..WriteOptions::default()
    };
    let (sink, seen) = recorder();
    rig.provider()
        .upload_from(reader, &p("streamed.bin"), opts, sink)
        .await
        .unwrap();
    feeder.join().unwrap();
    assert_eq!(rig.fake.file("account:1", b"streamed.bin").unwrap(), DATA);
    assert_eq!(seen.lock().unwrap().last(), Some(&(10, Some(10))));
}

#[tokio::test]
async fn a_pipe_without_a_length_reports_no_total() {
    let rig = Rig::ready().await;
    let (reader, writer) = rustix::pipe::pipe().unwrap();
    File::from(writer).write_all(DATA).unwrap();
    let (sink, seen) = recorder();
    rig.provider()
        .upload_from(reader, &p("s.bin"), WriteOptions::default(), sink)
        .await
        .unwrap();
    assert_eq!(rig.fake.file("account:1", b"s.bin").unwrap(), DATA);
    assert_eq!(seen.lock().unwrap().last(), Some(&(10, None)));
}

#[tokio::test]
async fn a_download_can_feed_a_pipe() {
    let rig = Rig::ready().await;
    rig.fake.put_file("account:1", b"remote.bin", DATA);
    let (reader, writer) = rustix::pipe::pipe().unwrap();
    let drain = std::thread::spawn(move || {
        let mut out = Vec::new();
        File::from(reader).read_to_end(&mut out).unwrap();
        out
    });
    rig.provider()
        .download_into(&p("remote.bin"), writer, ReadOptions::default(), no_progress())
        .await
        .unwrap();
    // Our copy of the write end is gone, so the reader sees the end of the stream.
    assert_eq!(drain.join().unwrap(), DATA);
}

#[tokio::test]
async fn make_file_creates_an_empty_file_exclusively() {
    let rig = Rig::ready().await;
    rig.fake.put_dir("account:1", b"d");
    let provider = rig.provider();
    provider.make_file(&p("d/new.txt")).await.unwrap();
    assert_eq!(rig.fake.file("account:1", b"d/new.txt").unwrap(), b"");
    assert_eq!(
        provider.make_file(&p("d/new.txt")).await.unwrap_err().kind,
        ErrorKind::AlreadyExists
    );
    assert_eq!(
        provider.make_file(&p("nodir/new.txt")).await.unwrap_err().kind,
        ErrorKind::NotFound
    );
    assert_eq!(
        rig.fake.file("account:1", b"d/new.txt").unwrap(),
        b"",
        "the existing file was not touched"
    );
}

#[tokio::test]
async fn dropping_a_transfer_cancels_the_job() {
    let rig = Rig::ready().await;
    rig.fake.set_chunk_size(1);
    rig.fake.set_latency(Duration::from_millis(40));
    let src = local_source(&rig, "src.bin", DATA).await;
    let provider = rig.provider();
    let target = p("slow.bin");
    let upload = provider.upload_from(read_only(&src), &target, WriteOptions::default(), no_progress());
    assert!(tokio::time::timeout(Duration::from_millis(250), upload)
        .await
        .is_err());
    eventually("the Cancel", || !rig.fake.calls_of("Cancel").is_empty()).await;
    tokio::time::sleep(Duration::from_millis(600)).await;
    let partial = rig.fake.file("account:1", b"slow.bin").map_or(0, |d| d.len());
    assert!(
        partial < DATA.len(),
        "the canceled job stopped early ({partial} bytes)"
    );
}

#[tokio::test]
async fn a_connection_lost_during_a_transfer_fails_it_as_connection_lost() {
    let rig = Rig::ready().await;
    rig.fake.set_chunk_size(1);
    rig.fake.set_latency(Duration::from_millis(30));
    let src = local_source(&rig, "src.bin", DATA).await;
    let provider = rig.provider();
    let target = p("x.bin");
    let upload = provider.upload_from(read_only(&src), &target, WriteOptions::default(), no_progress());
    let kill = async {
        tokio::time::sleep(Duration::from_millis(100)).await;
        rig.fake.disconnect_clients().await;
    };
    let (result, ()) = tokio::join!(upload, kill);
    assert_eq!(result.unwrap_err().kind, ErrorKind::ConnectionLost);
}

#[tokio::test]
async fn remove_tree_is_one_job_with_counts() {
    let rig = Rig::ready().await;
    rig.fake.put_file("account:1", b"tree/a", b"a");
    rig.fake.put_file("account:1", b"tree/sub/b", b"b");
    let provider = rig.provider();
    rig.fake
        .fail_job(JobFailure::new("RemoveTree", 0, "Locked", "in use"));
    let e = provider.remove_tree(&p("tree"), no_progress()).await.unwrap_err();
    assert_eq!(e.kind, ErrorKind::Locked);
    assert!(
        rig.fake.exists("account:1", b"tree/sub/b"),
        "a failed job removed nothing"
    );

    let outcome = provider.remove_tree(&p("tree"), no_progress()).await.unwrap();
    assert_eq!((outcome.get("files"), outcome.get("dirs")), (Some(2), Some(2)));
    assert!(!rig.fake.exists("account:1", b"tree"));
    assert_eq!(
        provider
            .remove_tree(&p("tree"), no_progress())
            .await
            .unwrap_err()
            .kind,
        ErrorKind::NotFound
    );
}
