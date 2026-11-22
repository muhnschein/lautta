// SPDX-License-Identifier: LGPL-2.1-or-later
//! Locked zip and 7z: written by us, read by the `zip` and `sevenz-rust2` crates and by us; zips
//! locked by the `zip` crate read by us.

use super::*;
use crate::password::Password;
use zip::write::SimpleFileOptions;

fn pw(text: &str) -> Password {
    Password::new(text).unwrap()
}

async fn run_locked(f: &Fixture, sources: &[Uri], kind: ArchiveKind, password: &str) -> Vec<u8> {
    let sink = SharedBuf::default();
    let opts = CompressOptions {
        password: Some(pw(password)),
        ..CompressOptions::new(kind)
    };
    compress(&f.resolver, sources, sink.clone(), &opts).await.unwrap();
    sink.bytes()
}

async fn open(name: &str, bytes: &[u8], cache: &Path, password: Option<&str>) -> Result<ArchiveProvider> {
    let mem = MemoryProvider::default();
    mem.add_file(name, bytes, 0);
    ArchiveProvider::open(Arc::new(mem), vp(name), cache.to_path_buf(), password.map(pw)).await
}

async fn read(p: &ArchiveProvider, path: &str) -> Result<Vec<u8>> {
    let h = p.open_read(&vp(path), Lane::Bulk).await?;
    read_all(h.as_ref(), 1 << 30).await
}

fn kind_of<T>(r: Result<T>) -> Option<ErrorKind> {
    r.err().map(|e| e.kind)
}

#[tokio::test]
async fn locked_zip_opens_in_the_zip_crate_with_its_password_only() {
    let f = fixture();
    let bytes = run_locked(&f, &[uri("mem", "proj")], ArchiveKind::Zip, "s3cret pass").await;
    let mut z = zip::ZipArchive::new(io::Cursor::new(bytes)).unwrap();
    assert!(
        z.file_names().any(|n| n.is_ok_and(|n| n == "proj/readme.txt")),
        "zip names stay readable"
    );
    let mut out = Vec::new();
    z.by_name_decrypt("proj/data/big.bin", b"s3cret pass")
        .unwrap()
        .read_to_end(&mut out)
        .unwrap();
    assert_eq!(out, f.big, "tag checked by the reader");
    out.clear();
    z.by_name_decrypt("proj/src/lib.rs", b"s3cret pass")
        .unwrap()
        .read_to_end(&mut out)
        .unwrap();
    assert!(out.is_empty());
    out.clear();
    z.by_name_decrypt("proj/link", b"s3cret pass")
        .unwrap()
        .read_to_end(&mut out)
        .unwrap();
    assert_eq!(out, b"src/main.rs", "a link's target is locked too");
    assert!(matches!(
        z.by_name("proj/readme.txt").err(),
        Some(zip::result::ZipError::UnsupportedArchive(
            zip::result::ZipError::PASSWORD_REQUIRED
        ))
    ));
    assert!(matches!(
        z.by_name_decrypt("proj/readme.txt", b"wrong").err(),
        Some(zip::result::ZipError::InvalidPassword)
    ));
    assert!(
        !z.by_name("proj/empty/").unwrap().encrypted(),
        "a folder has no data to lock"
    );
}

#[tokio::test]
async fn locked_zip_round_trips_through_the_archive_provider() {
    let f = fixture();
    let tmp = tempfile::tempdir().unwrap();
    let bytes = run_locked(&f, &[uri("mem", "proj")], ArchiveKind::Zip, "pass").await;
    assert_eq!(
        kind_of(open("l.zip", &bytes, tmp.path(), None).await),
        Some(ErrorKind::PasswordRequired)
    );
    assert_eq!(
        kind_of(open("l.zip", &bytes, tmp.path(), Some("Pass")).await),
        Some(ErrorKind::WrongPassword)
    );
    let p = open("l.zip", &bytes, tmp.path(), Some("pass")).await.unwrap();
    assert!(p.locked());
    assert_eq!(read(&p, "proj/data/big.bin").await.unwrap(), f.big);
    assert_eq!(read(&p, "proj/readme.txt").await.unwrap(), b"hello zip and tar");
    assert_eq!(read(&p, "proj/src/lib.rs").await.unwrap(), b"");
    assert_eq!(p.read_link(&vp("proj/link")).await.unwrap(), b"src/main.rs");
}

#[tokio::test]
async fn unlocked_zip_drops_the_password() {
    let f = fixture();
    let tmp = tempfile::tempdir().unwrap();
    let (bytes, _) = run(&f, &[uri("mem", "single.txt")], ArchiveKind::Zip).await;
    let p = open("u.zip", &bytes, tmp.path(), Some("unused")).await.unwrap();
    assert!(!p.locked(), "nothing kept for a file that needs no password");
    assert_eq!(read(&p, "single.txt").await.unwrap(), b"just one");
}

/// Bytes of `needle`'s first match from `from`.
fn find(hay: &[u8], needle: &[u8], from: usize) -> usize {
    from + hay[from..]
        .windows(needle.len())
        .position(|w| w == needle)
        .unwrap()
}

#[tokio::test]
async fn a_changed_byte_in_a_locked_zip_fails_its_read() {
    let f = fixture();
    let tmp = tempfile::tempdir().unwrap();
    let mut bytes = run_locked(&f, &[uri("mem", "proj/data")], ArchiveKind::Zip, "pass").await;
    // Name in big.bin's local header; 5000 bytes on is well inside its ciphertext.
    let name = find(&bytes, b"data/big.bin", 0);
    bytes[name + 5000] ^= 1;
    let p = open("c.zip", &bytes, tmp.path(), Some("pass")).await.unwrap();
    assert!(
        read(&p, "data/big.bin").await.is_err(),
        "changed data never reads as data"
    );
}

/// Central directory entry of `name`, found from the end record: no match inside ciphertext.
fn central_of(bytes: &[u8], name: &[u8]) -> usize {
    let end = bytes.len() - 22;
    assert_eq!(bytes[end..end + 4], [0x50, 0x4b, 0x05, 0x06]);
    let start = u32::from_le_bytes(bytes[end + 16..end + 20].try_into().unwrap()) as usize;
    let at = find(bytes, name, start) - 46;
    assert_eq!(bytes[at..at + 4], [0x50, 0x4b, 0x01, 0x02]);
    at
}

#[tokio::test]
async fn a_lowered_size_in_a_locked_zip_fails_its_read() {
    let f = fixture();
    let tmp = tempfile::tempdir().unwrap();
    let bytes = run_locked(&f, &[uri("mem", "proj")], ArchiveKind::Zip, "pass").await;
    // AE-2 tag covers neither size: readme.txt is deflated, link stored.
    let shorter = |name: &[u8]| {
        let mut b = bytes.clone();
        let at = central_of(&b, name) + 24;
        let size = u32::from_le_bytes(b[at..at + 4].try_into().unwrap());
        b[at..at + 4].copy_from_slice(&(size - 3).to_le_bytes());
        b
    };
    let p = open("d.zip", &shorter(b"proj/readme.txt"), tmp.path(), Some("pass"))
        .await
        .unwrap();
    assert!(
        read(&p, "proj/readme.txt").await.is_err(),
        "deflated entry cut short"
    );
    let p = open("s.zip", &shorter(b"proj/link"), tmp.path(), Some("pass"))
        .await
        .unwrap();
    assert!(
        p.read_link(&vp("proj/link")).await.is_err(),
        "stored entry cut short"
    );
    let p = open("l.zip", &bytes, tmp.path(), Some("pass")).await.unwrap();
    assert_eq!(read(&p, "proj/readme.txt").await.unwrap(), b"hello zip and tar");
}

fn zip_crate_locked(mode: zip::AesMode) -> Vec<u8> {
    let mut w = zip::ZipWriter::new(io::Cursor::new(Vec::new()));
    let deflated = SimpleFileOptions::default().with_aes_encryption(mode, "pw");
    let stored = deflated.compression_method(zip::CompressionMethod::Stored);
    w.start_file("a/deflated.txt", deflated).unwrap();
    w.write_all(&b"deflated text ".repeat(500)).unwrap();
    w.start_file("a/stored.bin", stored).unwrap();
    w.write_all(&noise(70_000, 9)).unwrap();
    w.add_directory("a/empty/", SimpleFileOptions::default()).unwrap();
    w.finish().unwrap().into_inner()
}

#[tokio::test]
async fn zips_locked_by_the_zip_crate_open_with_every_aes_strength() {
    let tmp = tempfile::tempdir().unwrap();
    for mode in [zip::AesMode::Aes128, zip::AesMode::Aes192, zip::AesMode::Aes256] {
        let bytes = zip_crate_locked(mode);
        assert_eq!(
            kind_of(open("z.zip", &bytes, tmp.path(), None).await),
            Some(ErrorKind::PasswordRequired),
            "{mode:?}"
        );
        assert_eq!(
            kind_of(open("z.zip", &bytes, tmp.path(), Some("pW")).await),
            Some(ErrorKind::WrongPassword),
            "{mode:?}"
        );
        let p = open("z.zip", &bytes, tmp.path(), Some("pw")).await.unwrap();
        assert_eq!(
            read(&p, "a/deflated.txt").await.unwrap(),
            b"deflated text ".repeat(500),
            "{mode:?}"
        );
        assert_eq!(
            read(&p, "a/stored.bin").await.unwrap(),
            noise(70_000, 9),
            "{mode:?}"
        );
        assert_eq!(
            p.stat(&vp("a/empty"), false, Lane::Bulk).await.unwrap().kind,
            Kind::Dir
        );
    }
}

/// One-entry zip with its AES version and CRC set in both headers.
fn with_ae(mut bytes: Vec<u8>, version: u8, crc: u32) -> Vec<u8> {
    let field = [0x01, 0x99, 0x07, 0x00];
    let local = find(&bytes, &[0x50, 0x4b, 0x03, 0x04], 0);
    let central = find(&bytes, &[0x50, 0x4b, 0x01, 0x02], 0);
    bytes[local + 14..local + 18].copy_from_slice(&crc.to_le_bytes());
    bytes[central + 16..central + 20].copy_from_slice(&crc.to_le_bytes());
    for from in [local, central] {
        let at = find(&bytes, &field, from);
        bytes[at + 4] = version;
    }
    bytes
}

#[tokio::test]
async fn ae1_entries_check_their_crc_and_ae2_ones_their_tag_only() {
    let tmp = tempfile::tempdir().unwrap();
    let data = b"checked twice".repeat(40);
    let mut w = zip::ZipWriter::new(io::Cursor::new(Vec::new()));
    w.start_file(
        "one.txt",
        SimpleFileOptions::default().with_aes_encryption(zip::AesMode::Aes256, "pw"),
    )
    .unwrap();
    w.write_all(&data).unwrap();
    let bytes = w.finish().unwrap().into_inner();
    let crc = {
        let mut c = flate2::Crc::new();
        c.update(&data);
        c.sum()
    };
    for (version, crc, ok) in [(1, crc, true), (1, crc ^ 1, false), (2, 0, true)] {
        let p = open(
            "one.zip",
            &with_ae(bytes.clone(), version, crc),
            tmp.path(),
            Some("pw"),
        )
        .await
        .unwrap();
        let got = read(&p, "one.txt").await;
        assert_eq!(
            got.as_ref().ok(),
            ok.then_some(&data),
            "AE-{version}, CRC {crc:08x}"
        );
    }
}

#[tokio::test]
async fn zipcrypto_entries_stay_unsupported() {
    let tmp = tempfile::tempdir().unwrap();
    let mut w = zip::ZipWriter::new(io::Cursor::new(Vec::new()));
    w.start_file("old.txt", SimpleFileOptions::default()).unwrap();
    w.write_all(b"legacy").unwrap();
    let mut bytes = w.finish().unwrap().into_inner();
    // Encrypted flag without the AES field: ZipCrypto.
    let local = find(&bytes, &[0x50, 0x4b, 0x03, 0x04], 0);
    let central = find(&bytes, &[0x50, 0x4b, 0x01, 0x02], 0);
    bytes[local + 6] |= 1;
    bytes[central + 8] |= 1;
    let p = open("old.zip", &bytes, tmp.path(), Some("pw")).await.unwrap();
    assert_eq!(kind_of(read(&p, "old.txt").await), Some(ErrorKind::Unsupported));
}

#[tokio::test]
async fn locked_7z_hides_its_names_and_needs_its_password() {
    let f = fixture();
    let tmp = tempfile::tempdir().unwrap();
    let bytes = run_locked(&f, &[uri("mem", "proj")], ArchiveKind::SevenZ, "pass").await;
    let utf16 = |s: &str| s.encode_utf16().flat_map(u16::to_le_bytes).collect::<Vec<u8>>();
    for name in ["readme.txt", "big.bin"] {
        assert!(!bytes.windows(name.len()).any(|w| w == name.as_bytes()));
        let wide = utf16(name);
        assert!(!bytes.windows(wide.len()).any(|w| w == wide), "{name} is hidden");
    }
    assert_eq!(
        kind_of(open("l.7z", &bytes, tmp.path(), None).await),
        Some(ErrorKind::PasswordRequired)
    );
    assert_eq!(
        kind_of(open("l.7z", &bytes, tmp.path(), Some("Pass")).await),
        Some(ErrorKind::WrongPassword)
    );
    let p = open("l.7z", &bytes, tmp.path(), Some("pass")).await.unwrap();
    assert!(p.locked());
    assert_eq!(read(&p, "proj/data/big.bin").await.unwrap(), f.big);
    assert_eq!(read(&p, "proj/readme.txt").await.unwrap(), b"hello zip and tar");
    assert_eq!(p.read_link(&vp("proj/link")).await.unwrap(), b"src/main.rs");

    let raw = sevenz_rust2::Archive::read(&mut io::Cursor::new(&bytes), &"pass".into()).unwrap();
    let aes_id = sevenz_rust2::EncoderMethod::AES256_SHA256.id();
    let mut ivs = Vec::new();
    for block in &raw.blocks {
        let aes = block
            .coders
            .iter()
            .find(|c| c.encoder_method_id() == aes_id)
            .expect("every block locked");
        let props = aes.properties();
        assert_eq!(props[0] & 0x3f, SEVENZ_KEY_ROUNDS_POWER, "key from 2^19 rounds");
        ivs.push(props.to_vec());
    }
    assert!(ivs.len() > 1);
    ivs.sort();
    ivs.dedup();
    assert_eq!(ivs.len(), raw.blocks.len(), "every block has its own IV");
}

/// As 7-Zip makes it by default: data locked, names readable.
fn sevenz_with_names_shown(password: &str) -> Vec<u8> {
    use sevenz_rust2::encoder_options::{AesEncoderOptions, Lzma2Options};
    let mut w = sevenz_rust2::ArchiveWriter::new(io::Cursor::new(Vec::new())).unwrap();
    w.set_encrypt_header(false);
    w.set_content_methods(vec![
        AesEncoderOptions::new(password.into()).into(),
        Lzma2Options::from_level(6).into(),
    ]);
    let entry = sevenz_rust2::ArchiveEntry::new_file("shown.txt");
    w.push_archive_entry(entry, Some(&b"names shown, data locked"[..]))
        .unwrap();
    w.finish().unwrap().into_inner()
}

#[tokio::test]
async fn locked_7z_with_readable_names_still_checks_its_password_at_open() {
    let tmp = tempfile::tempdir().unwrap();
    let bytes = sevenz_with_names_shown("pass");
    assert!(bytes.windows(18).any(|w| w
        == "shown.txt"
            .encode_utf16()
            .flat_map(u16::to_le_bytes)
            .collect::<Vec<u8>>()));
    assert_eq!(
        kind_of(open("s.7z", &bytes, tmp.path(), None).await),
        Some(ErrorKind::PasswordRequired)
    );
    assert_eq!(
        kind_of(open("s.7z", &bytes, tmp.path(), Some("Pass")).await),
        Some(ErrorKind::WrongPassword)
    );
    let p = open("s.7z", &bytes, tmp.path(), Some("pass")).await.unwrap();
    assert!(p.locked());
    assert_eq!(read(&p, "shown.txt").await.unwrap(), b"names shown, data locked");
}

#[tokio::test]
async fn tar_files_cannot_have_a_password() {
    let f = fixture();
    for kind in [ArchiveKind::TarGz, ArchiveKind::TarXz] {
        let opts = CompressOptions {
            password: Some(pw("pass")),
            ..CompressOptions::new(kind)
        };
        let done = compress(
            &f.resolver,
            &[uri("mem", "single.txt")],
            SharedBuf::default(),
            &opts,
        )
        .await;
        assert_eq!(kind_of(done), Some(ErrorKind::InvalidArgument));
    }
}
