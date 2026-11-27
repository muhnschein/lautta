// SPDX-License-Identifier: LGPL-2.1-or-later

use super::list::{list_kind, parse, ListEntry, MAX_BYTES, MAX_ENTRIES};
use super::*;
use crate::entry::Capabilities;
use crate::provider::memory::MemoryProvider;
use crate::provider::{no_progress, Provider};
use crate::vpath::VPath;
use std::sync::Mutex;

const ABC: [(Algorithm, &str); 13] = [
    (Algorithm::Sha256, "ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad"),
    (Algorithm::Md5, "900150983cd24fb0d6963f7d28e17f72"),
    (Algorithm::Sha1, "a9993e364706816aba3e25717850c26c9cd0d89d"),
    (Algorithm::Sha512, "ddaf35a193617abacc417349ae20413112e6fa4e89a97ea20a9eeee64b55d39a2192992a274fc1a836ba3c23a3feebbd454d4423643ce80e2a9ac94fa54ca49f"),
    (Algorithm::Sha224, "23097d223405d8228642a477bda255b32aadbce4bda0b3f7e36c9da7"),
    (Algorithm::Sha384, "cb00753f45a35e8bb5a03d699ac65007272c32ab0eded1631a8b605a43ff5bed8086072ba1e7cc2358baeca134c825a7"),
    (Algorithm::Sha3_256, "3a985da74fe225b2045c172d6bd390bd855f086e3e9d525b46bfe24511431532"),
    (Algorithm::Sha3_512, "b751850b1a57168a5693cd924b6b096e08f621827444f70d884f5d0240d2712e10e116e9192af3c91a7ec57647e3934057340b4cf408d5a56592f8274eec53f0"),
    (Algorithm::Blake2b, "ba80a53f981c4d0d6a2797b69f12f6e94c212f14685ac4b74b12bb6fdbffa2d17d87c5392aab792dc252d5de4533cc9518d38aa8dbf1925ab92386edd4009923"),
    (Algorithm::Blake3, "6437b3ac38465133ffb63b75273a8db548c558465d79db03fd359c6cd5bd9d85"),
    (Algorithm::Crc32, "352441c2"),
    (Algorithm::Xxh64, "44bc2cf5ad770999"),
    (Algorithm::Xxh128, "06b05ab6733a618578af5f94892f3950"),
];

fn digest_of(algorithm: Algorithm, data: &[u8]) -> String {
    let mut h = algorithm.hasher();
    h.update(data);
    hex::encode(h.finish())
}

#[test]
fn every_algorithm_matches_its_reference_tool() {
    for (algorithm, want) in ABC {
        assert_eq!(digest_of(algorithm, b"abc"), want, "{}", algorithm.label());
        assert_eq!(want.len(), algorithm.digest_len() * 2, "{}", algorithm.label());
    }
    assert_eq!(Algorithm::all().count(), ABC.len());
}

#[test]
fn split_updates_give_the_one_shot_digest() {
    for (algorithm, want) in ABC {
        let mut h = algorithm.hasher();
        h.update(b"a");
        h.update(b"");
        h.update(b"bc");
        assert_eq!(hex::encode(h.finish()), want, "{}", algorithm.label());
    }
}

#[test]
fn hex_is_lowercase() {
    assert_eq!(to_hex(&[0xab, 0x01, 0xff]), "ab01ff");
}

#[test]
fn ids_and_labels_parse_back() {
    for a in Algorithm::all() {
        assert_eq!(Algorithm::parse(a.id()), Some(a));
        assert_eq!(Algorithm::parse(a.label()), Some(a));
    }
    assert_eq!(Algorithm::parse("SHA3_512"), Some(Algorithm::Sha3_512));
    assert_eq!(Algorithm::parse("sha-2"), None);
    assert_eq!(Algorithm::parse(""), None);
    let common: Vec<_> = Algorithm::COMMON.iter().map(|a| a.label()).collect();
    assert_eq!(common, ["SHA-256", "MD5", "SHA-1", "SHA-512"]);
}

#[test]
fn throttle_reports_at_most_every_quarter_second() {
    let mut t = Throttle::new(no_progress(), None);
    let start = Instant::now();
    assert!(t.due(start), "first tick reports");
    assert!(!t.due(start + Duration::from_millis(249)));
    assert!(t.due(start + Duration::from_millis(250)));
    assert!(!t.due(start + Duration::from_millis(300)));
    assert!(t.due(start + Duration::from_millis(600)));
}

type Seen = Arc<Mutex<Vec<(u64, Option<u64>)>>>;

fn recorder() -> (ProgressSink, Seen) {
    let seen = Arc::new(Mutex::new(Vec::new()));
    let s = seen.clone();
    (Arc::new(move |d, t| s.lock().unwrap().push((d, t))), seen)
}

#[test]
fn reader_digest_reports_the_end() {
    let data = vec![7u8; CHUNK * 2 + 5];
    let (sink, seen) = recorder();
    let d = digest_reader(
        &data[..],
        Algorithm::Sha256,
        Some(data.len() as u64),
        sink,
        &Cancel::default(),
    )
    .unwrap();
    assert_eq!(hex::encode(d), digest_of(Algorithm::Sha256, &data));
    let seen = seen.lock().unwrap();
    assert_eq!(seen.first(), Some(&(CHUNK as u64, Some(data.len() as u64))));
    assert_eq!(seen.last(), Some(&(data.len() as u64, Some(data.len() as u64))));
}

#[test]
fn reader_digest_of_unknown_size_ends_with_its_size() {
    let (sink, seen) = recorder();
    digest_reader(&b"abc"[..], Algorithm::Md5, None, sink, &Cancel::default()).unwrap();
    assert_eq!(seen.lock().unwrap().last(), Some(&(3, Some(3))));
}

#[test]
fn canceled_reader_digest_stops() {
    let cancel = Cancel::default();
    cancel.cancel();
    let err = digest_reader(&b"abc"[..], Algorithm::Md5, None, no_progress(), &cancel).unwrap_err();
    assert_eq!(err.kind, ErrorKind::Canceled);
}

struct Flaky<'a>(&'a [u8], bool);
impl Read for Flaky<'_> {
    fn read(&mut self, buf: &mut [u8]) -> std::io::Result<usize> {
        if !self.1 {
            self.1 = true;
            return Err(std::io::ErrorKind::Interrupted.into());
        }
        self.0.read(buf)
    }
}

#[test]
fn interrupted_reads_are_retried_other_errors_end() {
    let d = digest_reader(
        Flaky(b"abc", false),
        Algorithm::Sha1,
        None,
        no_progress(),
        &Cancel::default(),
    )
    .unwrap();
    assert_eq!(hex::encode(d), "a9993e364706816aba3e25717850c26c9cd0d89d");
    struct Broken;
    impl Read for Broken {
        fn read(&mut self, _: &mut [u8]) -> std::io::Result<usize> {
            Err(std::io::ErrorKind::PermissionDenied.into())
        }
    }
    let err = digest_reader(Broken, Algorithm::Sha1, None, no_progress(), &Cancel::default()).unwrap_err();
    assert_eq!(err.kind, ErrorKind::PermissionDenied);
}

async fn mem_handle(data: &[u8]) -> Box<dyn ReadHandle> {
    let mem = MemoryProvider::new(Capabilities::default());
    mem.add_file("f", data, 0);
    mem.open_read(&VPath::parse(b"f").unwrap(), crate::provider::Lane::Bulk)
        .await
        .unwrap()
}

#[tokio::test]
async fn handle_digest_reads_every_chunk() {
    let data: Vec<u8> = (0..CHUNK * 3 + 17).map(|i| (i % 251) as u8).collect();
    let (sink, seen) = recorder();
    let d = digest_handle(
        mem_handle(&data).await,
        Algorithm::Blake3,
        sink,
        &Cancel::default(),
    )
    .await
    .unwrap();
    assert_eq!(hex::encode(d), digest_of(Algorithm::Blake3, &data));
    assert_eq!(
        seen.lock().unwrap().last(),
        Some(&(data.len() as u64, Some(data.len() as u64)))
    );
}

#[tokio::test]
async fn canceled_handle_digest_stops() {
    let cancel = Cancel::default();
    cancel.cancel();
    let err = digest_handle(mem_handle(b"abc").await, Algorithm::Md5, no_progress(), &cancel)
        .await
        .unwrap_err();
    assert_eq!(err.kind, ErrorKind::Canceled);
}

#[tokio::test]
async fn cancel_wakes_a_waiting_run() {
    let cancel = Cancel::default();
    let c = cancel.clone();
    let waiting = tokio::spawn(async move { c.run(std::future::pending::<Result<()>>()).await });
    tokio::task::yield_now().await;
    cancel.cancel();
    let res = tokio::time::timeout(Duration::from_secs(10), waiting)
        .await
        .expect("hang guard")
        .unwrap();
    assert_eq!(res.unwrap_err().kind, ErrorKind::Canceled);
    let res = cancel.run(std::future::pending::<Result<()>>()).await;
    assert_eq!(res.unwrap_err().kind, ErrorKind::Canceled);
    assert_eq!(Cancel::default().run(async { Ok(5) }).await.unwrap(), 5);
}

const SUM: &str = "ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad";

fn entry(name: &str, algorithm: Algorithm, hexd: &str) -> ListEntry {
    ListEntry {
        name: name.as_bytes().to_vec(),
        algorithm,
        digest: hex::decode(hexd).unwrap(),
    }
}

#[test]
fn list_names() {
    for (name, want) in [
        ("SHA256SUMS", Some(Some(Algorithm::Sha256))),
        ("SHA256SUMS.asc", Some(Some(Algorithm::Sha256))),
        ("sha512sums.txt", Some(Some(Algorithm::Sha512))),
        ("MD5SUMS", Some(Some(Algorithm::Md5))),
        ("x.iso.sha256", Some(Some(Algorithm::Sha256))),
        ("x.iso.SHA1", Some(Some(Algorithm::Sha1))),
        ("x.iso.md5sum", Some(Some(Algorithm::Md5))),
        ("x.b3", Some(Some(Algorithm::Blake3))),
        ("B2SUMS", Some(Some(Algorithm::Blake2b))),
        ("rel.sfv", Some(Some(Algorithm::Crc32))),
        ("x.xxh128", Some(Some(Algorithm::Xxh128))),
        ("CHECKSUMS", Some(None)),
        ("checksums.txt", Some(None)),
        ("Fedora-Workstation-43-x86_64-CHECKSUM", Some(None)),
        ("x.iso", None),
        ("sha256", None),
        ("notes.txt", None),
        ("md5.png", None),
        (".md5", Some(Some(Algorithm::Md5))),
        ("app-1.2-checksums.txt", Some(None)),
        ("sums.tar", None),
    ] {
        assert_eq!(list_kind(name.as_bytes()), want, "{name}");
    }
}

#[test]
fn gnu_lines_text_and_binary_mode() {
    let data = format!("{SUM}  a.iso\n{SUM} *b dir/c d.iso\r\n\nnot a line\n{SUM}  \n");
    assert_eq!(
        parse(b"SHA256SUMS", data.as_bytes()).unwrap(),
        [
            entry("a.iso", Algorithm::Sha256, SUM),
            entry("b dir/c d.iso", Algorithm::Sha256, SUM)
        ]
    );
}

#[test]
fn gnu_escaped_names() {
    let data = format!("\\{SUM}  new\\nline\\\\back\\r\n\\{SUM}  bad\\q\n");
    let got = parse(b"SHA256SUMS", data.as_bytes()).unwrap();
    assert_eq!(got.len(), 1);
    assert_eq!(got[0].name, b"new\nline\\back\r");
}

#[test]
fn length_picks_algorithm_unless_the_name_says() {
    let md5 = "900150983cd24fb0d6963f7d28e17f72";
    let data = format!("{md5}  a\n{SUM}  b\n");
    let got = parse(b"CHECKSUMS", data.as_bytes()).unwrap();
    assert_eq!(got[0].algorithm, Algorithm::Md5);
    assert_eq!(got[1].algorithm, Algorithm::Sha256);
    let got = parse(b"B3SUMS", data.as_bytes()).unwrap();
    assert_eq!(got[0].algorithm, Algorithm::Md5, "hint of other length ignored");
    assert_eq!(got[1].algorithm, Algorithm::Blake3);
    let got = parse(b"x.xxh128", format!("{md5}  a\n").as_bytes()).unwrap();
    assert_eq!(got[0].algorithm, Algorithm::Xxh128);
    assert!(parse(b"CHECKSUMS", b"abcde  odd\n").unwrap().is_empty());
}

#[test]
fn bsd_tags() {
    let b2 = "ba80a53f981c4d0d6a2797b69f12f6e94c212f14685ac4b74b12bb6fdbffa2d17d87c5392aab792dc252d5de4533cc9518d38aa8dbf1925ab92386edd4009923";
    let data = format!(
        "SHA256 (a (1).iso) = {SUM}\nBLAKE2b (b) = {b2}\nMD5 (c) = {SUM}\nXXH64 (d) = 44bc2cf5ad770999\nWHIRL (e) = {SUM}\nSHA256(f) = {SUM}\n"
    );
    assert_eq!(
        parse(b"CHECKSUMS", data.as_bytes()).unwrap(),
        [
            entry("a (1).iso", Algorithm::Sha256, SUM),
            entry("b", Algorithm::Blake2b, b2),
            entry("d", Algorithm::Xxh64, "44bc2cf5ad770999"),
        ]
    );
}

#[test]
fn pgp_clear_signed_list_reads_signed_text_only() {
    let data = format!(
        "-----BEGIN PGP SIGNED MESSAGE-----\nHash: SHA256\n{SUM}  in-header\n\n{SUM}  a\n- {SUM}  b\n-----BEGIN PGP SIGNATURE-----\n{SUM}  forged\n-----END PGP SIGNATURE-----\n"
    );
    let got = parse(b"SHA256SUMS.asc", data.as_bytes()).unwrap();
    assert_eq!(
        got,
        [
            entry("a", Algorithm::Sha256, SUM),
            entry("b", Algorithm::Sha256, SUM)
        ]
    );
}

#[test]
fn crc32_lists_not_named_sfv_are_gnu_lines() {
    assert_eq!(
        parse(b"files.crc32", b"352441c2  a.bin\n").unwrap(),
        [entry("a.bin", Algorithm::Crc32, "352441c2")]
    );
}

#[test]
fn tag_without_a_name_is_skipped() {
    assert!(parse(
        b"CHECKSUMS",
        format!("SHA256 () = {SUM}\nSHA256 ) = x (\n").as_bytes()
    )
    .unwrap()
    .is_empty());
}

#[test]
fn sfv() {
    let data = b"; made by hand\nCD1.bin 352441C2\nwith space.bin 352441c2  \nshort.bin 1234\n";
    assert_eq!(
        parse(b"rel.SFV", data).unwrap(),
        [
            entry("CD1.bin", Algorithm::Crc32, "352441c2"),
            entry("with space.bin", Algorithm::Crc32, "352441c2")
        ]
    );
}

#[test]
fn one_hash_file_names_its_stem() {
    let got = parse(b"x.iso.sha256", format!("{SUM}\n").as_bytes()).unwrap();
    assert_eq!(got, [entry("x.iso", Algorithm::Sha256, SUM)]);
    assert!(
        parse(b"SHA256SUMS", SUM.as_bytes()).unwrap().is_empty(),
        "no stem to name"
    );
    assert!(parse(b".sha256", SUM.as_bytes()).unwrap().is_empty());
    assert!(parse(b"x.sha256", b"\n  \n").unwrap().is_empty());
}

#[test]
fn caps() {
    let big = vec![b' '; MAX_BYTES as usize + 1];
    assert_eq!(
        parse(b"SHA256SUMS", &big).unwrap_err().kind,
        ErrorKind::Unsupported
    );
    assert!(parse(b"SHA256SUMS", &big[1..]).is_ok());
    let line = format!("{SUM}  a\n");
    let at_cap = line.repeat(MAX_ENTRIES);
    assert_eq!(
        parse(b"SHA256SUMS", at_cap.as_bytes()).unwrap().len(),
        MAX_ENTRIES
    );
    let over = line.repeat(MAX_ENTRIES + 1);
    assert_eq!(
        parse(b"SHA256SUMS", over.as_bytes()).unwrap_err().kind,
        ErrorKind::Unsupported
    );
}
