// SPDX-License-Identifier: LGPL-2.1-or-later

use super::*;

fn pw(text: &str) -> Password {
    Password::new(text).unwrap()
}

#[test]
fn crc_table_matches_crc32() {
    let data = b"The quick brown fox jumps over the lazy dog";
    let crc = !data.iter().fold(!0u32, |c, &b| crc_step(c, b));
    assert_eq!(crc, 0x414F_A339);
}

#[test]
fn a_short_header_is_an_error_not_a_panic() {
    assert!(Decryptor::new(&[0u8; 11][..], &pw("pw"), 0).is_err());
}

#[test]
fn only_the_password_whose_check_byte_matches_opens() {
    let head = [0u8; 12];
    let mut keys = Keys::new(b"pw");
    let check = head.iter().fold(0, |_, &b| keys.decrypt(b));
    assert!(Decryptor::new(&head[..], &pw("pw"), check).is_ok());
    assert_eq!(
        Decryptor::new(&head[..], &pw("pw"), check ^ 1)
            .err()
            .map(|e| e.kind),
        Some(ErrorKind::WrongPassword)
    );
}
