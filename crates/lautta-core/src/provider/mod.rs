// SPDX-License-Identifier: LGPL-2.1-or-later

pub mod archive;
pub mod local;
pub mod memory;
pub mod netvfs;

use crate::entry::{Capabilities, Entry};
use crate::error::Result;
use crate::vpath::VPath;
use async_trait::async_trait;
use std::os::fd::OwnedFd;
use std::sync::Arc;
use std::time::SystemTime;
use tokio::sync::mpsc;

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
    NoReplace,
    Replace,
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct AttributeChanges {
    pub mode: Option<u32>,
    pub modified: Option<SystemTime>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Disposition {
    #[default]
    Create,
    Truncate,
    Resume,
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct WriteOptions {
    pub disposition: Disposition,
    pub offset: u64,
    pub size: Option<u64>,
    pub modified: Option<SystemTime>,
    pub mode: Option<u32>,
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct ReadOptions {
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

pub type ProgressSink = Arc<dyn Fn(u64, Option<u64>) + Send + Sync>;

pub fn no_progress() -> ProgressSink {
    Arc::new(|_, _| {})
}

#[async_trait]
pub trait ReadHandle: Send + Sync {
    fn size(&self) -> Option<u64>;
    async fn read_at(&self, offset: u64, max: usize) -> Result<Vec<u8>>;
    async fn read_ahead(&self, _offset: u64, _bytes: u64) -> Result<()> {
        Ok(())
    }
}

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

    async fn known_capabilities(&self) -> Capabilities {
        self.capabilities()
    }

    async fn list(&self, dir: &VPath, lane: Lane, out: mpsc::Sender<Vec<Entry>>) -> Result<()>;
    async fn stat(&self, path: &VPath, follow: bool, lane: Lane) -> Result<Entry>;
    async fn read_link(&self, path: &VPath) -> Result<Vec<u8>>;
    async fn make_dir(&self, path: &VPath, exclusive: bool) -> Result<()>;
    async fn make_file(&self, path: &VPath) -> Result<()>;
    async fn remove_file(&self, path: &VPath) -> Result<()>;
    async fn remove_dir(&self, path: &VPath) -> Result<()>;
    async fn rename(&self, from: &VPath, to: &VPath, mode: RenameMode) -> Result<()>;
    async fn set_attributes(&self, path: &VPath, changes: AttributeChanges) -> Result<()>;
    async fn make_symlink(&self, target: &[u8], link: &VPath) -> Result<()>;
    async fn make_hardlink(&self, existing: &VPath, new_path: &VPath) -> Result<()>;
    async fn open_read(&self, path: &VPath, lane: Lane) -> Result<Box<dyn ReadHandle>>;
    async fn upload_from(
        &self,
        src: OwnedFd,
        dst: &VPath,
        opts: WriteOptions,
        progress: ProgressSink,
    ) -> Result<()>;
    async fn download_into(
        &self,
        src: &VPath,
        dst: OwnedFd,
        opts: ReadOptions,
        progress: ProgressSink,
    ) -> Result<()>;
    async fn server_copy(
        &self,
        from: &VPath,
        to: &VPath,
        opts: CopyOptions,
        progress: ProgressSink,
    ) -> Result<()>;
    async fn open_local(&self, _path: &VPath) -> Result<Option<OwnedFd>> {
        Ok(None)
    }
    async fn checksum(&self, path: &VPath, algorithm: &str) -> Result<Vec<u8>>;
    async fn space(&self, dir: &VPath) -> Result<SpaceInfo>;
}

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

pub trait ProviderResolver: Send + Sync {
    fn provider(&self, location: &str) -> Result<Arc<dyn Provider>>;
}

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
