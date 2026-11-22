// SPDX-License-Identifier: LGPL-2.1-or-later
//! Passwords of compressed files: memory only, wiped on drop. Never stored, logged (`Debug` hides
//! the text) or sent.

use std::fmt;
use zeroize::Zeroizing;

#[derive(Clone, PartialEq, Eq)]
pub struct Password(Zeroizing<String>);

impl Password {
    /// Empty text is no password.
    pub fn new(text: &str) -> Option<Password> {
        Password::from_text(text.to_owned())
    }

    /// Takes `text` without a copy left behind.
    pub fn from_text(text: String) -> Option<Password> {
        let text = Zeroizing::new(text);
        (!text.is_empty()).then(|| Password(text))
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }

    pub fn as_bytes(&self) -> &[u8] {
        self.0.as_bytes()
    }
}

impl fmt::Debug for Password {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("Password(..)")
    }
}

/// Longer text is cut for the estimate: zxcvbn's matching grows faster than linear.
const WEIGH_CHARS: usize = 64;

/// Guessable with little effort (zxcvbn score below 3 of 4). A hint only; nothing is refused.
pub fn is_weak(text: &str) -> bool {
    if text.is_empty() {
        return false;
    }
    let cut: String = text.chars().take(WEIGH_CHARS).collect();
    u8::from(zxcvbn::zxcvbn(&cut, &[]).score()) < 3
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn empty_text_is_no_password() {
        assert!(Password::new("").is_none());
        assert_eq!(
            Password::new("pw").map(|p| p.as_bytes().to_vec()),
            Some(b"pw".to_vec())
        );
    }

    #[test]
    fn debug_hides_the_text() {
        let p = Password::new("hunter2").expect("password");
        assert_eq!(format!("{p:?}"), "Password(..)");
    }

    #[test]
    fn weak_and_strong_passwords() {
        assert!(is_weak("password"));
        assert!(is_weak("1234"));
        assert!(is_weak("summer2024"));
        assert!(!is_weak("correct horse battery staple"));
        assert!(!is_weak("Vq7#pL2!xZ9@"));
        assert!(!is_weak(""), "no password is not a weak one");
    }

    #[test]
    fn long_text_is_weighed_by_its_start() {
        let long = "Vq7#pL2!xZ9@".repeat(100);
        assert!(!is_weak(&long));
        let weak_start = format!("{}{}", "a".repeat(WEIGH_CHARS), "Vq7#pL2!xZ9@");
        assert!(is_weak(&weak_start));
    }
}
