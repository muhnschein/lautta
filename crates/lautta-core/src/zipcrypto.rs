// SPDX-License-Identifier: LGPL-2.1-or-later

use crate::error::{Error, ErrorKind, Result};
use crate::password::Password;
use std::io::{self, Read};
use zeroize::Zeroize;

pub const HEADER_LEN: u64 = 12;

const CRC_TABLE: [u32; 256] = crc_table();

const fn crc_table() -> [u32; 256] {
    let mut table = [0u32; 256];
    let mut i = 0;
    while i < 256 {
        let mut c = i as u32;
        let mut bit = 0;
        while bit < 8 {
            c = if c & 1 != 0 {
                0xEDB8_8320 ^ (c >> 1)
            } else {
                c >> 1
            };
            bit += 1;
        }
        table[i] = c;
        i += 1;
    }
    table
}

fn crc_step(crc: u32, byte: u8) -> u32 {
    CRC_TABLE[((crc ^ u32::from(byte)) & 0xFF) as usize] ^ (crc >> 8)
}

struct Keys([u32; 3]);

impl Keys {
    fn new(password: &[u8]) -> Keys {
        let mut keys = Keys([0x1234_5678, 0x2345_6789, 0x3456_7890]);
        for &b in password {
            keys.update(b);
        }
        keys
    }

    fn update(&mut self, plain: u8) {
        let k = &mut self.0;
        k[0] = crc_step(k[0], plain);
        k[1] = k[1]
            .wrapping_add(k[0] & 0xFF)
            .wrapping_mul(134_775_813)
            .wrapping_add(1);
        k[2] = crc_step(k[2], (k[1] >> 24) as u8);
    }

    fn decrypt(&mut self, byte: u8) -> u8 {
        let t = (self.0[2] | 2) & 0xFFFF;
        let plain = byte ^ ((t.wrapping_mul(t ^ 1) >> 8) & 0xFF) as u8;
        self.update(plain);
        plain
    }
}

impl Drop for Keys {
    fn drop(&mut self) {
        self.0.zeroize();
    }
}

pub struct Decryptor<R: Read> {
    inner: R,
    keys: Keys,
}

impl<R: Read> Decryptor<R> {
    pub fn new(mut inner: R, password: &Password, check: u8) -> Result<Decryptor<R>> {
        let mut head = [0u8; HEADER_LEN as usize];
        inner.read_exact(&mut head)?;
        let mut keys = Keys::new(password.as_bytes());
        for b in &mut head {
            *b = keys.decrypt(*b);
        }
        let last = head[head.len() - 1];
        head.zeroize();
        if last != check {
            return Err(Error::kind(ErrorKind::WrongPassword));
        }
        Ok(Decryptor { inner, keys })
    }
}

impl<R: Read> Read for Decryptor<R> {
    fn read(&mut self, out: &mut [u8]) -> io::Result<usize> {
        let n = self.inner.read(out)?;
        for b in &mut out[..n] {
            *b = self.keys.decrypt(*b);
        }
        Ok(n)
    }
}

#[cfg(test)]
mod tests;
