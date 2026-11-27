// SPDX-License-Identifier: LGPL-2.1-or-later

use crate::error::{Error, ErrorKind, Result};
use crate::password::Password;
use aes::cipher::{KeyIvInit, StreamCipher};
use hmac::{KeyInit, Mac};
use std::io::{self, Read, Write};
use zeroize::Zeroizing;

pub const EXTRA_ID: u16 = 0x9901;
pub const METHOD: u16 = 99;
const CHECK_LEN: usize = 2;
const TAG_LEN: usize = 10;
const ROUNDS: u32 = 1000;
const VENDOR: &[u8; 2] = b"AE";
const AE1: u16 = 1;
const AE2: u16 = 2;

type HmacSha1 = hmac::Hmac<sha1::Sha1>;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Strength {
    Aes128,
    Aes192,
    Aes256,
}

impl Strength {
    fn code(self) -> u8 {
        match self {
            Strength::Aes128 => 1,
            Strength::Aes192 => 2,
            Strength::Aes256 => 3,
        }
    }

    fn from_code(code: u8) -> Option<Strength> {
        match code {
            1 => Some(Strength::Aes128),
            2 => Some(Strength::Aes192),
            3 => Some(Strength::Aes256),
            _ => None,
        }
    }

    fn key_len(self) -> usize {
        match self {
            Strength::Aes128 => 16,
            Strength::Aes192 => 24,
            Strength::Aes256 => 32,
        }
    }

    fn salt_len(self) -> usize {
        self.key_len() / 2
    }

    pub fn overhead(self) -> u64 {
        (self.salt_len() + CHECK_LEN + TAG_LEN) as u64
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct AesField {
    pub version: u16,
    pub strength: Strength,
    pub method: u16,
}

impl AesField {
    pub fn written(method: u16) -> AesField {
        AesField {
            version: AE2,
            strength: Strength::Aes256,
            method,
        }
    }

    pub fn parse(data: &[u8]) -> Option<AesField> {
        let version = u16::from_le_bytes([*data.first()?, *data.get(1)?]);
        if (version != AE1 && version != AE2) || data.get(2..4)? != VENDOR {
            return None;
        }
        Some(AesField {
            version,
            strength: Strength::from_code(*data.get(4)?)?,
            method: u16::from_le_bytes([*data.get(5)?, *data.get(6)?]),
        })
    }

    pub fn record(&self) -> [u8; 11] {
        let mut r = [0u8; 11];
        r[0..2].copy_from_slice(&EXTRA_ID.to_le_bytes());
        r[2..4].copy_from_slice(&7u16.to_le_bytes());
        r[4..6].copy_from_slice(&self.version.to_le_bytes());
        r[6..8].copy_from_slice(VENDOR);
        r[8] = self.strength.code();
        r[9..11].copy_from_slice(&self.method.to_le_bytes());
        r
    }

    pub fn has_crc(&self) -> bool {
        self.version == AE1
    }
}

enum Ctr {
    A128(ctr::Ctr128LE<aes::Aes128>),
    A192(ctr::Ctr128LE<aes::Aes192>),
    A256(ctr::Ctr128LE<aes::Aes256>),
}

impl Ctr {
    fn new(strength: Strength, key: &[u8]) -> Ctr {
        let iv = 1u128.to_le_bytes();
        const FITS: &str = "key length follows strength";
        match strength {
            Strength::Aes128 => Ctr::A128(ctr::Ctr128LE::new_from_slices(key, &iv).expect(FITS)),
            Strength::Aes192 => Ctr::A192(ctr::Ctr128LE::new_from_slices(key, &iv).expect(FITS)),
            Strength::Aes256 => Ctr::A256(ctr::Ctr128LE::new_from_slices(key, &iv).expect(FITS)),
        }
    }

    fn apply(&mut self, buf: &mut [u8]) {
        match self {
            Ctr::A128(c) => c.apply_keystream(buf),
            Ctr::A192(c) => c.apply_keystream(buf),
            Ctr::A256(c) => c.apply_keystream(buf),
        }
    }
}

struct Keys {
    cipher: Ctr,
    mac: HmacSha1,
    check: [u8; CHECK_LEN],
}

fn derive(password: &Password, salt: &[u8], strength: Strength) -> Keys {
    let n = strength.key_len();
    let mut out = Zeroizing::new(vec![0u8; 2 * n + CHECK_LEN]);
    pbkdf2::pbkdf2_hmac::<sha1::Sha1>(password.as_bytes(), salt, ROUNDS, &mut out);
    Keys {
        cipher: Ctr::new(strength, &out[..n]),
        mac: HmacSha1::new_from_slice(&out[n..2 * n]).expect("HMAC takes any key length"),
        check: [out[2 * n], out[2 * n + 1]],
    }
}

pub struct Encryptor<W: Write> {
    out: W,
    keys: Keys,
    buf: Vec<u8>,
}

impl<W: Write> Encryptor<W> {
    pub fn new(mut out: W, password: &Password) -> io::Result<Encryptor<W>> {
        let strength = Strength::Aes256;
        let mut salt = vec![0u8; strength.salt_len()];
        getrandom::fill(&mut salt).map_err(|e| io::Error::other(format!("no random salt: {e}")))?;
        let keys = derive(password, &salt, strength);
        out.write_all(&salt)?;
        out.write_all(&keys.check)?;
        Ok(Encryptor {
            out,
            keys,
            buf: Vec::new(),
        })
    }

    pub fn finish(self) -> io::Result<W> {
        let Encryptor { mut out, keys, .. } = self;
        let tag = keys.mac.finalize().into_bytes();
        out.write_all(&tag[..TAG_LEN])?;
        Ok(out)
    }
}

impl<W: Write> Write for Encryptor<W> {
    fn write(&mut self, data: &[u8]) -> io::Result<usize> {
        self.buf.clear();
        self.buf.extend_from_slice(data);
        self.keys.cipher.apply(&mut self.buf);
        self.keys.mac.update(&self.buf);
        self.out.write_all(&self.buf)?;
        Ok(data.len())
    }

    fn flush(&mut self) -> io::Result<()> {
        self.out.flush()
    }
}

fn auth_failed() -> io::Error {
    io::Error::new(
        io::ErrorKind::InvalidData,
        "zip entry failed its authentication check",
    )
}

pub struct Decryptor<R: Read> {
    inner: R,
    cipher: Ctr,
    mac: Option<HmacSha1>,
    left: u64,
}

impl<R: Read> Decryptor<R> {
    pub fn new(mut inner: R, stored: u64, password: &Password, strength: Strength) -> Result<Decryptor<R>> {
        let left = stored
            .checked_sub(strength.overhead())
            .ok_or_else(|| Error::new(ErrorKind::ProtocolError, "corrupt zip: encrypted entry too short"))?;
        let mut head = vec![0u8; strength.salt_len() + CHECK_LEN];
        inner.read_exact(&mut head)?;
        let (salt, check) = head.split_at(strength.salt_len());
        let keys = derive(password, salt, strength);
        if keys.check[..] != *check {
            return Err(Error::kind(ErrorKind::WrongPassword));
        }
        Ok(Decryptor {
            inner,
            cipher: keys.cipher,
            mac: Some(keys.mac),
            left,
        })
    }

    pub fn finish(&mut self) -> io::Result<()> {
        let mut scratch = [0u8; 8192];
        while self.left > 0 {
            let want = usize::try_from(self.left)
                .unwrap_or(usize::MAX)
                .min(scratch.len());
            self.read_cipher(&mut scratch[..want])?;
        }
        let Some(mac) = self.mac.take() else {
            return Ok(());
        };
        let mut tag = [0u8; TAG_LEN];
        self.inner.read_exact(&mut tag)?;
        mac.verify_truncated_left(&tag).map_err(|_| auth_failed())
    }

    fn read_cipher(&mut self, out: &mut [u8]) -> io::Result<usize> {
        let n = self.inner.read(out)?;
        if n == 0 {
            return Err(io::Error::new(
                io::ErrorKind::UnexpectedEof,
                "zip entry is truncated",
            ));
        }
        if let Some(mac) = self.mac.as_mut() {
            mac.update(&out[..n]);
        }
        self.cipher.apply(&mut out[..n]);
        self.left -= n as u64;
        Ok(n)
    }
}

impl<R: Read> Read for Decryptor<R> {
    fn read(&mut self, out: &mut [u8]) -> io::Result<usize> {
        let want = usize::try_from(self.left).unwrap_or(usize::MAX).min(out.len());
        if want == 0 {
            self.finish()?;
            return Ok(0);
        }
        let n = self.read_cipher(&mut out[..want])?;
        if self.left == 0 {
            self.finish()?;
        }
        Ok(n)
    }
}

#[cfg(test)]
mod tests;
