// SPDX-License-Identifier: LGPL-2.1-or-later
//! File checksums: algorithms, one-pass hashing off the async workers, cancellation, progress.
//! Lists of checksums (`SHA256SUMS`, `x.sha256`, SFV, …): [`list`].

pub mod list;

use crate::error::{Error, ErrorKind, Result};
use crate::provider::{ProgressSink, ReadHandle};
use sha2::Digest;
use std::io::Read;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::time::{Duration, Instant};
use tokio::sync::Notify;

/// Read size; also the most a remote read asks for at once.
pub const CHUNK: usize = 1 << 20;
/// Progress at most this often (≤ 4 Hz, battery).
const PROGRESS_EVERY: Duration = Duration::from_millis(250);

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Algorithm {
    Sha256,
    Md5,
    Sha1,
    Sha512,
    Sha224,
    Sha384,
    Sha3_256,
    Sha3_512,
    Blake2b,
    Blake3,
    Crc32,
    Xxh64,
    Xxh128,
}

impl Algorithm {
    /// Offered first, most used first.
    pub const COMMON: [Algorithm; 4] = [
        Algorithm::Sha256,
        Algorithm::Md5,
        Algorithm::Sha1,
        Algorithm::Sha512,
    ];
    /// Behind "More algorithms".
    pub const MORE: [Algorithm; 9] = [
        Algorithm::Sha224,
        Algorithm::Sha384,
        Algorithm::Sha3_256,
        Algorithm::Sha3_512,
        Algorithm::Blake2b,
        Algorithm::Blake3,
        Algorithm::Crc32,
        Algorithm::Xxh64,
        Algorithm::Xxh128,
    ];

    pub fn all() -> impl Iterator<Item = Algorithm> {
        Self::COMMON.into_iter().chain(Self::MORE)
    }

    /// Stable id: QML ⇄ core, netvfs `checksum_algorithms`.
    pub fn id(self) -> &'static str {
        match self {
            Algorithm::Sha256 => "sha256",
            Algorithm::Md5 => "md5",
            Algorithm::Sha1 => "sha1",
            Algorithm::Sha512 => "sha512",
            Algorithm::Sha224 => "sha224",
            Algorithm::Sha384 => "sha384",
            Algorithm::Sha3_256 => "sha3-256",
            Algorithm::Sha3_512 => "sha3-512",
            Algorithm::Blake2b => "blake2b",
            Algorithm::Blake3 => "blake3",
            Algorithm::Crc32 => "crc32",
            Algorithm::Xxh64 => "xxh64",
            Algorithm::Xxh128 => "xxh128",
        }
    }

    /// Name on screen; not translated.
    pub fn label(self) -> &'static str {
        match self {
            Algorithm::Sha256 => "SHA-256",
            Algorithm::Md5 => "MD5",
            Algorithm::Sha1 => "SHA-1",
            Algorithm::Sha512 => "SHA-512",
            Algorithm::Sha224 => "SHA-224",
            Algorithm::Sha384 => "SHA-384",
            Algorithm::Sha3_256 => "SHA3-256",
            Algorithm::Sha3_512 => "SHA3-512",
            Algorithm::Blake2b => "BLAKE2b",
            Algorithm::Blake3 => "BLAKE3",
            Algorithm::Crc32 => "CRC32",
            Algorithm::Xxh64 => "XXH64",
            Algorithm::Xxh128 => "XXH128",
        }
    }

    /// Id or label, any case, `-` and `_` ignored (`SHA-256`, `sha256`, `SHA3_256`).
    pub fn parse(name: &str) -> Option<Algorithm> {
        let key: String = name
            .chars()
            .filter(|c| *c != '-' && *c != '_')
            .flat_map(char::to_lowercase)
            .collect();
        Algorithm::all().find(|a| a.id().replace('-', "") == key)
    }

    /// Digest length in bytes.
    pub fn digest_len(self) -> usize {
        match self {
            Algorithm::Crc32 => 4,
            Algorithm::Xxh64 => 8,
            Algorithm::Md5 | Algorithm::Xxh128 => 16,
            Algorithm::Sha1 => 20,
            Algorithm::Sha224 => 28,
            Algorithm::Sha256 | Algorithm::Sha3_256 | Algorithm::Blake3 => 32,
            Algorithm::Sha384 => 48,
            Algorithm::Sha512 | Algorithm::Sha3_512 | Algorithm::Blake2b => 64,
        }
    }

    pub fn hasher(self) -> Hasher {
        Hasher(match self {
            Algorithm::Sha256 => State::Sha256(sha2::Sha256::new()),
            Algorithm::Md5 => State::Md5(md5::Md5::new()), // NOSONAR: user-chosen file checksum
            Algorithm::Sha1 => State::Sha1(sha1::Sha1::new()), // NOSONAR: user-chosen file checksum
            Algorithm::Sha512 => State::Sha512(sha2::Sha512::new()),
            Algorithm::Sha224 => State::Sha224(sha2::Sha224::new()),
            Algorithm::Sha384 => State::Sha384(sha2::Sha384::new()),
            Algorithm::Sha3_256 => State::Sha3_256(sha3::Sha3_256::new()),
            Algorithm::Sha3_512 => State::Sha3_512(sha3::Sha3_512::new()),
            Algorithm::Blake2b => State::Blake2b(blake2::Blake2b512::new()),
            Algorithm::Blake3 => State::Blake3(Box::default()),
            Algorithm::Crc32 => State::Crc32(crc32fast::Hasher::new()),
            Algorithm::Xxh64 => State::Xxh64(xxhash_rust::xxh64::Xxh64::new(0)),
            Algorithm::Xxh128 => State::Xxh128(Box::default()),
        })
    }
}

enum State {
    Sha256(sha2::Sha256),
    Md5(md5::Md5),
    Sha1(sha1::Sha1),
    Sha512(sha2::Sha512),
    Sha224(sha2::Sha224),
    Sha384(sha2::Sha384),
    Sha3_256(sha3::Sha3_256),
    Sha3_512(sha3::Sha3_512),
    Blake2b(blake2::Blake2b512),
    Blake3(Box<blake3::Hasher>),
    Crc32(crc32fast::Hasher),
    Xxh64(xxhash_rust::xxh64::Xxh64),
    Xxh128(Box<xxhash_rust::xxh3::Xxh3>),
}

/// One running digest. Integer digests (CRC32, xxHash) come out big-endian, as `crc32`, `xxhsum`
/// print them.
pub struct Hasher(State);

impl Hasher {
    pub fn update(&mut self, data: &[u8]) {
        match &mut self.0 {
            State::Sha256(h) => h.update(data),
            State::Md5(h) => h.update(data),
            State::Sha1(h) => h.update(data),
            State::Sha512(h) => h.update(data),
            State::Sha224(h) => h.update(data),
            State::Sha384(h) => h.update(data),
            State::Sha3_256(h) => h.update(data),
            State::Sha3_512(h) => h.update(data),
            State::Blake2b(h) => h.update(data),
            State::Blake3(h) => {
                h.update(data);
            }
            State::Crc32(h) => h.update(data),
            State::Xxh64(h) => h.update(data),
            State::Xxh128(h) => h.update(data),
        }
    }

    pub fn finish(self) -> Vec<u8> {
        match self.0 {
            State::Sha256(h) => h.finalize().to_vec(),
            State::Md5(h) => h.finalize().to_vec(),
            State::Sha1(h) => h.finalize().to_vec(),
            State::Sha512(h) => h.finalize().to_vec(),
            State::Sha224(h) => h.finalize().to_vec(),
            State::Sha384(h) => h.finalize().to_vec(),
            State::Sha3_256(h) => h.finalize().to_vec(),
            State::Sha3_512(h) => h.finalize().to_vec(),
            State::Blake2b(h) => h.finalize().to_vec(),
            State::Blake3(h) => h.finalize().as_bytes().to_vec(),
            State::Crc32(h) => h.finalize().to_be_bytes().to_vec(),
            State::Xxh64(h) => h.digest().to_be_bytes().to_vec(),
            State::Xxh128(h) => h.digest128().to_be_bytes().to_vec(),
        }
    }
}

/// Lowercase hex, as checksum tools print it.
pub fn to_hex(digest: &[u8]) -> String {
    hex::encode(digest)
}

/// Stops a running checksum: the read ends at the next chunk, the call returns `Canceled`.
#[derive(Clone, Default)]
pub struct Cancel(Arc<CancelInner>);

#[derive(Default)]
struct CancelInner {
    set: AtomicBool,
    notify: Notify,
}

impl Cancel {
    pub fn cancel(&self) {
        self.0.set.store(true, Ordering::Release);
        self.0.notify.notify_waiters();
    }

    pub fn is_canceled(&self) -> bool {
        self.0.set.load(Ordering::Acquire)
    }

    pub fn check(&self) -> Result<()> {
        if self.is_canceled() {
            Err(Error::kind(ErrorKind::Canceled))
        } else {
            Ok(())
        }
    }

    /// Resolves once [`Cancel::cancel`] ran.
    pub async fn canceled(&self) {
        loop {
            let woken = self.0.notify.notified();
            if self.is_canceled() {
                return;
            }
            woken.await;
        }
    }

    /// Runs `work` until it ends or this is canceled.
    pub async fn run<T>(&self, work: impl std::future::Future<Output = Result<T>>) -> Result<T> {
        tokio::select! {
            res = work => res,
            () = self.canceled() => Err(Error::kind(ErrorKind::Canceled)),
        }
    }
}

/// Reports progress at most every [`PROGRESS_EVERY`]; the end always.
struct Throttle {
    sink: ProgressSink,
    total: Option<u64>,
    last: Option<Instant>,
}

impl Throttle {
    fn new(sink: ProgressSink, total: Option<u64>) -> Throttle {
        Throttle {
            sink,
            total,
            last: None,
        }
    }

    fn due(&mut self, now: Instant) -> bool {
        match self.last {
            Some(last) if now.duration_since(last) < PROGRESS_EVERY => false,
            _ => {
                self.last = Some(now);
                true
            }
        }
    }

    fn tick(&mut self, done: u64) {
        if self.due(Instant::now()) {
            (self.sink)(done, self.total);
        }
    }

    fn end(&self, done: u64) {
        (self.sink)(done, self.total.or(Some(done)));
    }
}

/// Blocking: hashes `reader` to its end. Call on a blocking thread only.
pub fn digest_reader(
    mut reader: impl Read,
    algorithm: Algorithm,
    total: Option<u64>,
    progress: ProgressSink,
    cancel: &Cancel,
) -> Result<Vec<u8>> {
    let mut hasher = algorithm.hasher();
    let mut throttle = Throttle::new(progress, total);
    let mut buf = vec![0u8; CHUNK];
    let mut done = 0u64;
    loop {
        cancel.check()?;
        let n = match reader.read(&mut buf) {
            Ok(0) => break,
            Ok(n) => n,
            Err(e) if e.kind() == std::io::ErrorKind::Interrupted => continue,
            Err(e) => return Err(e.into()),
        };
        hasher.update(&buf[..n]);
        done += n as u64;
        throttle.tick(done);
    }
    throttle.end(done);
    Ok(hasher.finish())
}

/// Reads `handle` on the caller's task while a blocking thread hashes the chunk before.
pub async fn digest_handle(
    handle: Box<dyn ReadHandle>,
    algorithm: Algorithm,
    progress: ProgressSink,
    cancel: &Cancel,
) -> Result<Vec<u8>> {
    let (tx, mut rx) = tokio::sync::mpsc::channel::<Vec<u8>>(2);
    let hashing = tokio::task::spawn_blocking(move || {
        let mut hasher = algorithm.hasher();
        while let Some(chunk) = rx.blocking_recv() {
            hasher.update(&chunk);
        }
        hasher.finish()
    });
    let mut throttle = Throttle::new(progress, handle.size());
    let mut done = 0u64;
    loop {
        cancel.check()?;
        let chunk = handle.read_at(done, CHUNK).await?;
        if chunk.is_empty() {
            break;
        }
        done += chunk.len() as u64;
        tx.send(chunk)
            .await
            .map_err(|_| Error::new(ErrorKind::Internal, "checksum thread ended"))?;
        throttle.tick(done);
    }
    drop(tx);
    let digest = hashing
        .await
        .map_err(|e| Error::new(ErrorKind::Internal, format!("checksum thread: {e}")))?;
    throttle.end(done);
    Ok(digest)
}

#[cfg(test)]
mod tests;
