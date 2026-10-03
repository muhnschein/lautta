// SPDX-License-Identifier: LGPL-2.1-or-later
//! The one provider trait (SPEC ARC-4, Appendix B). The UI and the planner see
//! only [`Provider`] + [`Capabilities`]; there are no provider-specific
//! branches above this layer.

pub mod archive;
pub mod local;
pub mod netvfs;

use crate::entry::{Capabilities, Entry};
use crate::error::Result;
use crate::vpath::VPath;
use async_trait::async_trait;
use std::os::fd::OwnedFd;
use std::sync::Arc;
use std::time::SystemTime;
use tokio::sync::mpsc;

/// Request lane hints (SPEC NVB-7, netvfs XB-10).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Lane {
    Interactive,
    Bulk,
    Stream,
}

impl Lane {
    pub fn wire(self) -> &'static str {
        match self {
            Lane::Interactive => "interactive",
            Lane::Bulk => "bulk",
            Lane::Stream => "stream",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RenameMode {
    /// Fail with `AlreadyExists` if the destination exists (the default, OPS table).
    NoReplace,
    Replace,
}

/// Attribute changes for `set_attributes`.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct AttributeChanges {
    pub mode: Option<u32>,
    pub modified: Option<SystemTime>,
}

/// Disposition for writes (`upload_from`).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Disposition {
    /// Create; fail if it exists.
    #[default]
    Create,
    /// Create or truncate.
    Truncate,
    /// Append at `offset` (resume, XFR-12).
    Resume,
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct WriteOptions {
    pub disposition: Disposition,
    /// Resume offset in the source fd and destination (XFR-12).
    pub offset: u64,
    /// Total size, required for FIFOs on backends that need a length (XB-11).
    pub size: Option<u64>,
    pub modified: Option<SystemTime>,
    pub mode: Option<u32>,
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct ReadOptions {
    /// Start offset (resume of downloads, XFR-12).
    pub offset: u64,
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct CopyOptions {
    pub replace: bool,
    pub preserve_mtime: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct SpaceInfo {
    pub free: u64,
    pub total: u64,
    pub used: u64,
}

/// Progress callback: bytes done, total bytes (if known).
pub type ProgressSink = Arc<dyn Fn(u64, Option<u64>) + Send + Sync>;

pub fn no_progress() -> ProgressSink {
    Arc::new(|_, _| {})
}

/// Random-access reader (thumbnails, archives, media streaming, hex view).
#[async_trait]
pub trait ReadHandle: Send + Sync {
    fn size(&self) -> Option<u64>;
    /// Reads up to `max` bytes at `offset`; an empty result means end of file.
    async fn read_at(&self, offset: u64, max: usize) -> Result<Vec<u8>>;
    /// Hint that `bytes` from `offset` will be read soon (PRV-8 read-ahead).
    async fn read_ahead(&self, _offset: u64, _bytes: u64) -> Result<()> {
        Ok(())
    }
}

/// Reads the whole handle up to `limit` bytes.
pub async fn read_all(handle: &dyn ReadHandle, limit: u64) -> Result<Vec<u8>> {
    let mut out = Vec::new();
    loop {
        let remaining = limit.saturating_sub(out.len() as u64);
        if remaining == 0 {
            return Ok(out);
        }
        let chunk = handle
            .read_at(
                out.len() as u64,
                usize::try_from(remaining.min(1 << 20)).unwrap_or(1 << 20),
            )
            .await?;
        if chunk.is_empty() {
            return Ok(out);
        }
        out.extend_from_slice(&chunk);
    }
}

#[async_trait]
pub trait Provider: Send + Sync {
    fn capabilities(&self) -> Capabilities;

    /// Streams the listing in batches into `out`. Returns when complete.
    async fn list(&self, dir: &VPath, lane: Lane, out: mpsc::Sender<Vec<Entry>>) -> Result<()>;
    async fn stat(&self, path: &VPath, follow: bool, lane: Lane) -> Result<Entry>;
    async fn read_link(&self, path: &VPath) -> Result<Vec<u8>>;
    async fn make_dir(&self, path: &VPath, exclusive: bool) -> Result<()>;
    /// Creates an empty file (exclusive).
    async fn make_file(&self, path: &VPath) -> Result<()>;
    async fn remove_file(&self, path: &VPath) -> Result<()>;
    async fn remove_dir(&self, path: &VPath) -> Result<()>;
    async fn rename(&self, from: &VPath, to: &VPath, mode: RenameMode) -> Result<()>;
    async fn set_attributes(&self, path: &VPath, changes: AttributeChanges) -> Result<()>;
    async fn make_symlink(&self, target: &[u8], link: &VPath) -> Result<()>;
    async fn make_hardlink(&self, existing: &VPath, new_path: &VPath) -> Result<()>;
    async fn open_read(&self, path: &VPath, lane: Lane) -> Result<Box<dyn ReadHandle>>;
    /// Writes `dst` from `src`, an fd the caller opened (NVB-9).
    async fn upload_from(
        &self,
        src: OwnedFd,
        dst: &VPath,
        opts: WriteOptions,
        progress: ProgressSink,
    ) -> Result<()>;
    /// Reads `src` into `dst`, an fd the caller opened (NVB-9).
    async fn download_into(
        &self,
        src: &VPath,
        dst: OwnedFd,
        opts: ReadOptions,
        progress: ProgressSink,
    ) -> Result<()>;
    async fn server_copy(&self, from: &VPath, to: &VPath, opts: CopyOptions) -> Result<()>;
    async fn checksum(&self, path: &VPath, algorithm: &str) -> Result<Vec<u8>>;
    async fn space(&self, dir: &VPath) -> Result<SpaceInfo>;
}

/// Collects a full listing (tests and small folders).
pub async fn list_all(provider: &dyn Provider, dir: &VPath, lane: Lane) -> Result<Vec<Entry>> {
    let (tx, mut rx) = mpsc::channel(16);
    let collect = async {
        let mut all = Vec::new();
        while let Some(batch) = rx.recv().await {
            all.extend(batch);
        }
        all
    };
    let (res, all) = tokio::join!(provider.list(dir, lane, tx), collect);
    res.map(|()| all)
}

/// Resolves a location id to its provider. Implemented by the location
/// registry; tests use [`StaticResolver`].
pub trait ProviderResolver: Send + Sync {
    fn provider(&self, location: &str) -> Result<Arc<dyn Provider>>;
}

/// A fixed map of providers (tests, and the archive provider's nesting).
#[derive(Default, Clone)]
pub struct StaticResolver {
    pub providers: std::collections::HashMap<String, Arc<dyn Provider>>,
}

impl StaticResolver {
    pub fn with(mut self, location: &str, provider: Arc<dyn Provider>) -> StaticResolver {
        self.providers.insert(location.to_owned(), provider);
        self
    }
}

impl ProviderResolver for StaticResolver {
    fn provider(&self, location: &str) -> Result<Arc<dyn Provider>> {
        self.providers.get(location).cloned().ok_or_else(|| {
            crate::error::Error::new(
                crate::error::ErrorKind::NotFound,
                format!("unknown location {location}"),
            )
        })
    }
}
