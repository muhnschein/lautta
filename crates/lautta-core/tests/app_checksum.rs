// SPDX-License-Identifier: LGPL-2.1-or-later

use lautta_core::app::Core;
use lautta_core::app_checksum::*;
use lautta_core::checksum::{Algorithm, Cancel};
use lautta_core::entry::{cap, Capabilities};
use lautta_core::locations::{Location, LocationKind, LocationRegistry};
use lautta_core::paths::AppPaths;
use lautta_core::provider::memory::MemoryProvider;
use lautta_core::provider::no_progress;
use lautta_core::{Error, ErrorKind, Uri};
use std::path::Path;
use std::sync::{Arc, Mutex};

const ABC256: &str = "ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad";
const ABC_MD5: &str = "900150983cd24fb0d6963f7d28e17f72";
const ZERO256: &str = "0000000000000000000000000000000000000000000000000000000000000000";

struct Home {
    _dir: tempfile::TempDir,
    root: std::path::PathBuf,
    core: Arc<Core>,
}

async fn home() -> Home {
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path().to_path_buf();
    std::fs::create_dir_all(root.join("Documents")).unwrap();
    let paths = AppPaths::new(&root);
    let registry = LocationRegistry::with_media_root(paths.clone(), root.join("media"));
    let core = Core::open_with(paths, registry).await.unwrap();
    Home {
        _dir: dir,
        root,
        core,
    }
}

fn uri(s: &str) -> Uri {
    Uri::parse(s).unwrap()
}

fn write(path: &Path, data: &[u8]) {
    std::fs::create_dir_all(path.parent().unwrap()).unwrap();
    std::fs::write(path, data).unwrap();
}

fn server(h: &Home, mem: MemoryProvider) -> Arc<MemoryProvider> {
    let mem = Arc::new(mem);
    h.core.locations.register(
        Location::remote(
            "nas",
            LocationKind::Server {
                provider: "sftp".into(),
            },
            "NAS",
            None,
        ),
        mem.clone(),
    );
    mem
}

async fn sum(h: &Home, u: &str, a: Algorithm) -> Result<String, Error> {
    h.core
        .checksum(&uri(u), a, no_progress(), &Cancel::default())
        .await
        .map(hex::encode)
}

#[tokio::test]
async fn local_file_with_progress_to_the_end() {
    let h = home().await;
    write(&h.root.join("Documents/a.txt"), b"abc");
    let seen = Arc::new(Mutex::new(Vec::new()));
    let s = seen.clone();
    let d = h
        .core
        .checksum(
            &uri("lautta://user-documents/a.txt"),
            Algorithm::Sha256,
            Arc::new(move |done, total| s.lock().unwrap().push((done, total))),
            &Cancel::default(),
        )
        .await
        .unwrap();
    assert_eq!(hex::encode(d), ABC256);
    assert_eq!(seen.lock().unwrap().last(), Some(&(3, Some(3))));
    assert_eq!(
        sum(&h, "lautta://user-documents/a.txt", Algorithm::Md5)
            .await
            .unwrap(),
        ABC_MD5
    );
}

#[tokio::test]
async fn folders_fifos_and_missing_files_are_refused() {
    let h = home().await;
    std::fs::create_dir_all(h.root.join("Documents/dir")).unwrap();
    let fifo = h.root.join("Documents/pipe");
    rustix::fs::mknodat(
        rustix::fs::CWD,
        &fifo,
        rustix::fs::FileType::Fifo,
        rustix::fs::Mode::RUSR | rustix::fs::Mode::WUSR,
        0,
    )
    .unwrap();
    let dir = sum(&h, "lautta://user-documents/dir", Algorithm::Sha256)
        .await
        .unwrap_err();
    assert_eq!(dir.kind, ErrorKind::IsADirectory);
    let pipe = sum(&h, "lautta://user-documents/pipe", Algorithm::Sha256)
        .await
        .unwrap_err();
    assert_eq!(pipe.kind, ErrorKind::Unsupported);
    let gone = sum(&h, "lautta://user-documents/none", Algorithm::Sha256)
        .await
        .unwrap_err();
    assert_eq!(gone.kind, ErrorKind::NotFound);
}

#[tokio::test]
async fn local_link_to_a_file_is_hashed() {
    let h = home().await;
    write(&h.root.join("Documents/a.txt"), b"abc");
    std::os::unix::fs::symlink("a.txt", h.root.join("Documents/link")).unwrap();
    assert_eq!(
        sum(&h, "lautta://user-documents/link", Algorithm::Sha256)
            .await
            .unwrap(),
        ABC256
    );
}

#[tokio::test]
async fn server_file_is_read_through_when_the_server_cannot_hash() {
    let h = home().await;
    let mem = server(&h, MemoryProvider::new(Capabilities::with(&[cap::CHECKSUMS])));
    mem.add_file("f", b"abc", 0);
    assert_eq!(
        sum(&h, "lautta://nas/f", Algorithm::Sha256).await.unwrap(),
        ABC256
    );
    let calls = mem.calls();
    assert!(!calls.iter().any(|c| c.starts_with("checksum")), "{calls:?}");
    assert!(calls.iter().any(|c| c.starts_with("open_read")), "{calls:?}");
}

#[tokio::test]
async fn server_hashes_what_it_lists() {
    let h = home().await;
    let mut caps = Capabilities::with(&[cap::CHECKSUMS]);
    caps.checksum_algorithms = vec!["sha256".into(), "md5".into()];
    let mem = server(&h, MemoryProvider::new(caps));
    mem.add_file("f", b"abc", 0);
    assert_eq!(
        sum(&h, "lautta://nas/f", Algorithm::Sha256).await.unwrap(),
        ABC256
    );
    assert!(mem.calls().iter().any(|c| c.starts_with("checksum")));
    assert!(!mem.calls().iter().any(|c| c.starts_with("open_read")));
    assert_eq!(sum(&h, "lautta://nas/f", Algorithm::Md5).await.unwrap(), ABC_MD5);
    assert!(mem.calls().iter().any(|c| c.starts_with("open_read")));
}

#[tokio::test]
async fn server_errors_are_not_hidden() {
    let h = home().await;
    let mut caps = Capabilities::with(&[cap::CHECKSUMS]);
    caps.checksum_algorithms = vec!["sha256".into()];
    let mem = server(&h, MemoryProvider::new(caps));
    mem.add_file("f", b"abc", 0);
    mem.fail_next("checksum", "f", Error::kind(ErrorKind::TimedOut));
    assert_eq!(
        sum(&h, "lautta://nas/f", Algorithm::Sha256)
            .await
            .unwrap_err()
            .kind,
        ErrorKind::TimedOut
    );
    mem.fail_next("open_read", "f", Error::kind(ErrorKind::ConnectionLost));
    assert_eq!(
        sum(&h, "lautta://nas/f", Algorithm::Md5).await.unwrap_err().kind,
        ErrorKind::ConnectionLost
    );
}

#[tokio::test]
async fn server_digest_of_wrong_length_is_refused() {
    let h = home().await;
    let mut caps = Capabilities::with(&[cap::CHECKSUMS]);
    caps.checksum_algorithms = vec!["sha256".into()];
    let mem = server(&h, MemoryProvider::new(caps));
    mem.add_file("f", b"abc", 0);
    mem.answer_checksum("f", &[0; 20]);
    assert_eq!(
        sum(&h, "lautta://nas/f", Algorithm::Sha256)
            .await
            .unwrap_err()
            .kind,
        ErrorKind::ProtocolError
    );
    mem.answer_checksum("f", &[7; 32]);
    assert_eq!(
        sum(&h, "lautta://nas/f", Algorithm::Sha256).await.unwrap(),
        "07".repeat(32)
    );
}

#[tokio::test]
async fn canceled_checksum_ends() {
    let h = home().await;
    write(&h.root.join("Documents/a.txt"), b"abc");
    let cancel = Cancel::default();
    cancel.cancel();
    let err = h
        .core
        .checksum(
            &uri("lautta://user-documents/a.txt"),
            Algorithm::Sha256,
            no_progress(),
            &cancel,
        )
        .await
        .unwrap_err();
    assert_eq!(err.kind, ErrorKind::Canceled);
}

#[test]
fn verdicts() {
    let listed = |name: &str, a, d: &[u8]| Listed {
        list_name: name.as_bytes().to_vec(),
        algorithm: a,
        digest: d.to_vec(),
    };
    let lists = [
        listed("MD5SUMS", Algorithm::Md5, b"m"),
        listed("old.sha256", Algorithm::Sha256, b"x"),
        listed("SHA256SUMS", Algorithm::Sha256, b"d"),
    ];
    assert_eq!(
        verdict(&lists, Algorithm::Sha256, b"d"),
        Some(Verdict::Matches(b"SHA256SUMS".to_vec()))
    );
    assert_eq!(
        verdict(&lists, Algorithm::Sha256, b"z"),
        Some(Verdict::Differs(b"old.sha256".to_vec()))
    );
    assert_eq!(
        verdict(&lists, Algorithm::Md5, b"m"),
        Some(Verdict::Matches(b"MD5SUMS".to_vec()))
    );
    assert_eq!(verdict(&lists, Algorithm::Sha1, b"d"), None);
}

#[test]
fn names_stay_inside_the_folder() {
    let dir = uri("lautta://user-documents/iso");
    assert_eq!(
        resolve(&dir, b"a.iso"),
        Some(uri("lautta://user-documents/iso/a.iso"))
    );
    assert_eq!(
        resolve(&dir, b"./sub/b"),
        Some(uri("lautta://user-documents/iso/sub/b"))
    );
    for bad in [&b"../x"[..], b"sub/../../x", b"/etc/passwd", b"a\0b", b".", b""] {
        assert_eq!(resolve(&dir, bad), None, "{}", String::from_utf8_lossy(bad));
    }
}

#[tokio::test]
async fn lists_next_to_a_file_that_name_it() {
    let h = home().await;
    let d = h.root.join("Documents");
    write(&d.join("a.iso"), b"abc");
    write(
        &d.join("SHA256SUMS"),
        format!("{ABC256}  a.iso\n{ZERO256}  b.iso\n").as_bytes(),
    );
    write(&d.join("a.iso.md5"), format!("{ABC_MD5}\n").as_bytes());
    write(&d.join("other.md5"), format!("{ABC_MD5}  other\n").as_bytes());
    write(
        &d.join("sub/SHA256SUMS"),
        format!("{ZERO256}  ../a.iso\n").as_bytes(),
    );
    write(&d.join("CHECKSUMS"), vec![b'#'; 2 << 20].as_slice());
    let mut got = h
        .core
        .lists_naming(&uri("lautta://user-documents/a.iso"), &Cancel::default())
        .await
        .unwrap();
    got.sort_by(|a, b| a.list_name.cmp(&b.list_name));
    let names: Vec<_> = got
        .iter()
        .map(|l| (String::from_utf8_lossy(&l.list_name).into_owned(), l.algorithm))
        .collect();
    assert_eq!(
        names,
        [
            ("SHA256SUMS".into(), Algorithm::Sha256),
            ("a.iso.md5".into(), Algorithm::Md5)
        ]
    );
    assert_eq!(hex::encode(&got[0].digest), ABC256);
}

#[tokio::test]
async fn lists_named_after_the_file_come_first() {
    let h = home().await;
    let d = h.root.join("Documents");
    write(&d.join("a.iso"), b"abc");
    write(&d.join("SHA256SUMS"), format!("{ZERO256}  a.iso\n").as_bytes());
    write(&d.join("a.iso.sha256"), format!("{ZERO256}\n").as_bytes());
    let got = h
        .core
        .lists_naming(&uri("lautta://user-documents/a.iso"), &Cancel::default())
        .await
        .unwrap();
    let names: Vec<_> = got.iter().map(|l| l.list_name.as_slice()).collect();
    assert_eq!(names, [b"a.iso.sha256".as_slice(), b"SHA256SUMS"]);
}

#[tokio::test]
async fn folders_and_big_files_use_up_none_of_the_lists_read() {
    let h = home().await;
    let d = h.root.join("Documents");
    write(&d.join("a.iso"), b"abc");
    for i in 0..16 {
        std::fs::create_dir_all(d.join(format!("0d{i:02}.sha256"))).unwrap();
        write(
            &d.join(format!("0f{i:02}.sha256")),
            vec![b'#'; (1 << 20) + 1].as_slice(),
        );
    }
    write(&d.join("SHA256SUMS"), format!("{ABC256}  a.iso\n").as_bytes());
    let got = h
        .core
        .lists_naming(&uri("lautta://user-documents/a.iso"), &Cancel::default())
        .await
        .unwrap();
    assert_eq!(got.len(), 1);
    assert_eq!(got[0].list_name, b"SHA256SUMS");
}

#[tokio::test]
async fn no_lists_no_verdict() {
    let h = home().await;
    write(&h.root.join("Documents/a.iso"), b"abc");
    let got = h
        .core
        .lists_naming(&uri("lautta://user-documents/a.iso"), &Cancel::default())
        .await
        .unwrap();
    assert!(got.is_empty());
}

type Sink = Arc<dyn Fn(VerifyEvent) + Send + Sync>;

fn collect() -> (Sink, Arc<Mutex<Vec<VerifyEvent>>>) {
    let seen = Arc::new(Mutex::new(Vec::new()));
    let s = seen.clone();
    (Arc::new(move |e| s.lock().unwrap().push(e)), seen)
}

#[tokio::test]
async fn verify_reports_every_state() {
    let h = home().await;
    let d = h.root.join("Documents/rel");
    write(&d.join("good.iso"), b"abc");
    write(&d.join("bad.iso"), b"abd");
    write(&d.join("sub/deep.iso"), b"abc");
    std::fs::create_dir_all(d.join("dir")).unwrap();
    std::os::unix::fs::symlink("good.iso", d.join("link.iso")).unwrap();
    write(&h.root.join("Documents/outside.iso"), b"abc");
    let list = [
        "good.iso",
        "bad.iso",
        "gone.iso",
        "../outside.iso",
        "/etc/passwd",
        "link.iso",
        "dir",
        "sub/deep.iso",
    ]
    .iter()
    .map(|n| format!("{ABC256}  {n}\n"))
    .collect::<String>();
    write(&d.join("SHA256SUMS"), list.as_bytes());
    let (sink, seen) = collect();
    h.core
        .verify_list(
            &uri("lautta://user-documents/rel/SHA256SUMS"),
            sink,
            &Cancel::default(),
        )
        .await
        .unwrap();
    let seen = seen.lock().unwrap();
    let VerifyEvent::Listed(names) = &seen[0] else {
        panic!("{:?}", seen[0])
    };
    assert_eq!(names.len(), 8);
    let states: Vec<_> = seen
        .iter()
        .filter_map(|e| match e {
            VerifyEvent::Checked { index, state } => Some((*index, *state)),
            _ => None,
        })
        .collect();
    use ItemState::*;
    assert_eq!(
        states,
        [
            (0, Matches),
            (1, Differs),
            (2, Missing),
            (3, NotChecked),
            (4, NotChecked),
            (5, NotChecked),
            (6, NotChecked),
            (7, Matches)
        ]
    );
    assert!(seen.contains(&VerifyEvent::Progress {
        index: 0,
        done: 3,
        total: Some(3)
    }));
}

#[tokio::test]
async fn verify_on_a_server_marks_read_errors_per_file() {
    let h = home().await;
    let mem = server(&h, MemoryProvider::new(Capabilities::with(&[cap::WRITE])));
    mem.add_file("r/a", b"abc", 0);
    mem.add_file("r/b", b"abc", 0);
    mem.add_file("r/c", b"abc", 0);
    mem.add_file(
        "r/MD5SUMS",
        format!("{ABC_MD5}  a\n{ABC_MD5}  b\n{ABC_MD5}  c\n").as_bytes(),
        0,
    );
    mem.fail_next("open_read", "r/a", Error::kind(ErrorKind::PermissionDenied));
    mem.fail_next("stat", "r/c", Error::kind(ErrorKind::ConnectionLost));
    let (sink, seen) = collect();
    h.core
        .verify_list(&uri("lautta://nas/r/MD5SUMS"), sink, &Cancel::default())
        .await
        .unwrap();
    let seen = seen.lock().unwrap();
    assert!(seen.contains(&VerifyEvent::Checked {
        index: 0,
        state: ItemState::Failed(ErrorKind::PermissionDenied)
    }));
    assert!(seen.contains(&VerifyEvent::Checked {
        index: 1,
        state: ItemState::Matches
    }));
    assert!(seen.contains(&VerifyEvent::Checked {
        index: 2,
        state: ItemState::Failed(ErrorKind::ConnectionLost)
    }));
}

#[tokio::test]
async fn verify_reads_a_list_of_exactly_the_largest_size() {
    let h = home().await;
    let d = h.root.join("Documents");
    write(&d.join("a"), b"abc");
    let last = format!("{ABC256}  a");
    let mut data = vec![b'#'; (1 << 20) - last.len() - 1];
    data.push(b'\n');
    data.extend_from_slice(last.as_bytes());
    write(&d.join("SHA256SUMS"), &data);
    let (sink, seen) = collect();
    h.core
        .verify_list(
            &uri("lautta://user-documents/SHA256SUMS"),
            sink,
            &Cancel::default(),
        )
        .await
        .unwrap();
    assert!(seen.lock().unwrap().contains(&VerifyEvent::Checked {
        index: 0,
        state: ItemState::Matches
    }));
}

#[tokio::test]
async fn verify_refuses_what_is_not_a_list() {
    let h = home().await;
    write(
        &h.root.join("Documents/SHA256SUMS"),
        vec![b'#'; 2 << 20].as_slice(),
    );
    let (sink, seen) = collect();
    let err = h
        .core
        .verify_list(
            &uri("lautta://user-documents/SHA256SUMS"),
            sink,
            &Cancel::default(),
        )
        .await
        .unwrap_err();
    assert_eq!(err.kind, ErrorKind::Unsupported);
    assert!(seen.lock().unwrap().is_empty());
    write(
        &h.root.join("Documents/notes.txt"),
        format!("{}  a\n", "0".repeat(64)).as_bytes(),
    );
    let (sink, seen) = collect();
    let err = h
        .core
        .verify_list(
            &uri("lautta://user-documents/notes.txt"),
            sink,
            &Cancel::default(),
        )
        .await
        .unwrap_err();
    assert_eq!(err.kind, ErrorKind::Unsupported, "a name no list has");
    assert!(seen.lock().unwrap().is_empty());
    std::fs::create_dir_all(h.root.join("Documents/MD5SUMS")).unwrap();
    let (sink, _) = collect();
    let err = h
        .core
        .verify_list(&uri("lautta://user-documents/MD5SUMS"), sink, &Cancel::default())
        .await
        .unwrap_err();
    assert_eq!(err.kind, ErrorKind::Unsupported, "a folder");
    let (sink, _) = collect();
    let err = h
        .core
        .verify_list(&uri("lautta://user-documents/"), sink, &Cancel::default())
        .await
        .unwrap_err();
    assert_eq!(err.kind, ErrorKind::InvalidArgument);
}

#[tokio::test]
async fn canceled_verify_stops_before_the_next_file() {
    let h = home().await;
    let d = h.root.join("Documents");
    write(&d.join("a"), b"abc");
    write(&d.join("b"), b"abc");
    write(
        &d.join("SHA256SUMS"),
        format!("{ABC256}  a\n{ABC256}  b\n").as_bytes(),
    );
    let cancel = Cancel::default();
    let c = cancel.clone();
    let seen = Arc::new(Mutex::new(Vec::new()));
    let s = seen.clone();
    let sink: Arc<dyn Fn(VerifyEvent) + Send + Sync> = Arc::new(move |e| {
        if matches!(e, VerifyEvent::Checked { index: 0, .. }) {
            c.cancel();
        }
        s.lock().unwrap().push(e);
    });
    let err = h
        .core
        .verify_list(&uri("lautta://user-documents/SHA256SUMS"), sink, &cancel)
        .await
        .unwrap_err();
    assert_eq!(err.kind, ErrorKind::Canceled);
    assert!(!seen
        .lock()
        .unwrap()
        .iter()
        .any(|e| matches!(e, VerifyEvent::Checked { index: 1, .. })));
}
