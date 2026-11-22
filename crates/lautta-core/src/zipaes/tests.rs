// SPDX-License-Identifier: LGPL-2.1-or-later
use super::*;
use std::io::Cursor;

fn pw(text: &str) -> Password {
    Password::new(text).expect("password")
}

fn seal(plain: &[u8], password: &str) -> Vec<u8> {
    let mut enc = Encryptor::new(Vec::new(), &pw(password)).expect("encryptor");
    enc.write_all(plain).expect("write");
    enc.finish().expect("finish")
}

fn open(stored: &[u8], password: &str) -> Result<Decryptor<Cursor<Vec<u8>>>> {
    Decryptor::new(
        Cursor::new(stored.to_vec()),
        stored.len() as u64,
        &pw(password),
        Strength::Aes256,
    )
}

fn read_all(stored: &[u8], password: &str) -> io::Result<Vec<u8>> {
    let mut out = Vec::new();
    open(stored, password)
        .map_err(io::Error::other)?
        .read_to_end(&mut out)?;
    Ok(out)
}

#[test]
fn sealed_data_opens_with_its_password() {
    let plain: Vec<u8> = (0..70_000u32).map(|i| (i % 251) as u8).collect();
    let stored = seal(&plain, "secret");
    assert_eq!(
        stored.len() as u64,
        plain.len() as u64 + Strength::Aes256.overhead()
    );
    assert_eq!(Strength::Aes256.overhead(), 16 + 2 + 10);
    assert_ne!(
        &stored[18..18 + 64],
        &plain[..64],
        "data is not stored in the clear"
    );
    assert_eq!(read_all(&stored, "secret").expect("read"), plain);
}

#[test]
fn same_data_seals_differently_each_time() {
    assert_ne!(seal(b"same", "pw"), seal(b"same", "pw"), "fresh salt every time");
}

#[test]
fn wrong_password_fails_before_any_data() {
    let stored = seal(b"hello", "right");
    let err = open(&stored, "wrong").err().expect("refused");
    assert_eq!(err.kind, ErrorKind::WrongPassword);
}

#[test]
fn a_changed_byte_fails_the_check() {
    let mut stored = seal(&[7u8; 1000], "pw");
    stored[100] ^= 1;
    let err = read_all(&stored, "pw").expect_err("changed data");
    assert_eq!(err.kind(), io::ErrorKind::InvalidData);
}

#[test]
fn a_changed_tag_fails_the_check() {
    let mut stored = seal(b"some data", "pw");
    let last = stored.len() - 1;
    stored[last] ^= 0x80;
    let err = read_all(&stored, "pw").expect_err("changed tag");
    assert_eq!(err.kind(), io::ErrorKind::InvalidData);
}

#[test]
fn a_cut_entry_fails() {
    let stored = seal(&[1u8; 300], "pw");
    let cut = &stored[..stored.len() - 4];
    let mut d = Decryptor::new(
        Cursor::new(cut.to_vec()),
        stored.len() as u64,
        &pw("pw"),
        Strength::Aes256,
    )
    .expect("header fits");
    let err = d.read_to_end(&mut Vec::new()).expect_err("truncated");
    assert_eq!(err.kind(), io::ErrorKind::UnexpectedEof);
    let short = Decryptor::new(Cursor::new(vec![0u8; 20]), 20, &pw("pw"), Strength::Aes256).err();
    assert_eq!(
        short.map(|e| e.kind),
        Some(ErrorKind::ProtocolError),
        "shorter than salt, check and tag"
    );
}

#[test]
fn a_reader_that_stops_early_still_gets_the_check() {
    let mut stored = seal(&[9u8; 5000], "pw");
    stored[4000] ^= 1;
    let mut d = open(&stored, "pw").expect("open");
    let mut head = [0u8; 10];
    d.read_exact(&mut head).expect("first bytes");
    assert_eq!(head, [9u8; 10]);
    assert_eq!(
        d.finish().expect_err("rest checked").kind(),
        io::ErrorKind::InvalidData
    );

    let good = seal(&[9u8; 5000], "pw");
    let mut d = open(&good, "pw").expect("open");
    d.read_exact(&mut head).expect("first bytes");
    d.finish().expect("untouched data passes");
    d.finish().expect("second finish is a no-op");
}

#[test]
fn empty_data_still_carries_a_tag() {
    let stored = seal(b"", "pw");
    assert_eq!(stored.len() as u64, Strength::Aes256.overhead());
    assert_eq!(read_all(&stored, "pw").expect("read"), Vec::<u8>::new());
    let mut bad = stored.clone();
    bad[20] ^= 1;
    assert!(read_all(&bad, "pw").is_err(), "tag of empty data is checked too");
}

#[test]
fn extra_field_round_trip() {
    let f = AesField::written(8);
    let r = f.record();
    assert_eq!(u16::from_le_bytes([r[0], r[1]]), EXTRA_ID);
    assert_eq!(u16::from_le_bytes([r[2], r[3]]), 7);
    assert_eq!(AesField::parse(&r[4..]), Some(f));
    assert_eq!(f.version, 2);
    assert_eq!(f.strength, Strength::Aes256);
    assert!(!f.has_crc(), "AE-2 keeps no CRC");
    let ae1 = AesField::parse(&[1, 0, b'A', b'E', 1, 0, 0]).expect("AE-1, AES-128, stored");
    assert!(ae1.has_crc());
    assert_eq!((ae1.strength, ae1.method), (Strength::Aes128, 0));
    assert_eq!(
        AesField::parse(&[2, 0, b'A', b'E', 2, 8, 0]).map(|f| f.strength),
        Some(Strength::Aes192)
    );
}

#[test]
fn foreign_extra_fields_are_not_aes() {
    assert_eq!(AesField::parse(&[2, 0, b'X', b'E', 3, 8, 0]), None, "vendor");
    assert_eq!(AesField::parse(&[3, 0, b'A', b'E', 3, 8, 0]), None, "version");
    assert_eq!(AesField::parse(&[0, 0, b'A', b'E', 3, 8, 0]), None, "version 0");
    assert_eq!(AesField::parse(&[2, 0, b'A', b'E', 4, 8, 0]), None, "strength");
    assert_eq!(AesField::parse(&[2, 0, b'A', b'E', 3, 8]), None, "short");
}

#[test]
fn every_strength_has_its_sizes() {
    for (s, key) in [
        (Strength::Aes128, 16),
        (Strength::Aes192, 24),
        (Strength::Aes256, 32),
    ] {
        assert_eq!(s.key_len(), key);
        assert_eq!(s.salt_len(), key / 2);
        assert_eq!(Strength::from_code(s.code()), Some(s));
    }
    assert_eq!(Strength::from_code(0), None);
}
