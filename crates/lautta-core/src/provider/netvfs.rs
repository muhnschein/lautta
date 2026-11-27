// SPDX-License-Identifier: LGPL-2.1-or-later

use super::{
    check_one_by_one, no_progress, AttributeChanges, CopyOptions, Disposition, FolderCheck, KnownFolder,
    Lane, ProgressSink, Provider, ReadHandle, ReadOptions, RenameMode, SpaceInfo, WriteOptions,
};
use crate::bridge::convert::{capabilities_from_wire, entry_from_wire, finished_error};
use crate::bridge::feature;
use crate::bridge::link::{rpc, JobOutcome, Link};
use crate::bridge::{bridge_id, location_id, BridgeClient, Level};
use crate::entry::{cap, system_time_to_ms, Capabilities, Entry};
use crate::error::{Error, ErrorKind, Result};
use crate::uri::LocationId;
use crate::vpath::VPath;
use async_trait::async_trait;
use lautta_bridge_proto::zvariant::Fd;
use lautta_bridge_proto::{
    Opts, OptsBuilder, WireCheckItem, WireCheckResult, WireWalkItem, MAX_CHECK_ITEMS, MAX_READ_BYTES,
};
use std::collections::HashMap;
use std::os::fd::OwnedFd;
use std::sync::{Arc, RwLock};
use tokio::sync::mpsc;

pub struct NetvfsProvider {
    client: BridgeClient,
    loc: String,
    caps: RwLock<Option<Capabilities>>,
}

impl NetvfsProvider {
    pub fn new(client: BridgeClient, location: &str) -> NetvfsProvider {
        NetvfsProvider {
            client,
            loc: bridge_id(location).to_owned(),
            caps: RwLock::new(None),
        }
    }

    pub async fn connect(client: BridgeClient, location: &str) -> Result<NetvfsProvider> {
        let provider = NetvfsProvider::new(client, location);
        provider.refresh_capabilities().await?;
        Ok(provider)
    }

    pub fn location(&self) -> LocationId {
        location_id(&self.loc)
    }

    pub async fn refresh_capabilities(&self) -> Result<()> {
        let wire = rpc(self.link()?.proxy().capabilities(&self.loc).await)?;
        *self
            .caps
            .write()
            .unwrap_or_else(std::sync::PoisonError::into_inner) = Some(capabilities_from_wire(&wire));
        Ok(())
    }

    pub fn has_capabilities(&self) -> bool {
        self.caps
            .read()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .is_some()
    }

    fn link(&self) -> Result<Arc<Link>> {
        self.client.files_link()
    }

    pub async fn remove_tree(&self, path: &VPath, progress: ProgressSink) -> Result<JobOutcome> {
        let link = self.link()?;
        link.run_job(link.proxy().remove_tree(&self.loc, path.as_bytes()), &progress)
            .await
    }
}

impl BridgeClient {
    pub fn provider(&self, location: &str) -> NetvfsProvider {
        NetvfsProvider::new(self.clone(), location)
    }
}

fn upload_opts(opts: &WriteOptions) -> Opts {
    let resume = opts.disposition == Disposition::Resume && opts.offset > 0;
    let disposition = match opts.disposition {
        Disposition::Create => "create",
        Disposition::Truncate => "truncate",
        Disposition::Resume if resume => "resume",
        Disposition::Resume => "truncate",
    };
    let mut b = OptsBuilder::new()
        .str("lane", Lane::Bulk.wire())
        .str("disposition", disposition);
    if resume {
        b = b.int("offset", to_i64(opts.offset));
    }
    if let Some(total) = opts.size {
        let length = if resume {
            total.saturating_sub(opts.offset)
        } else {
            total
        };
        b = b.int("size", to_i64(length));
    }
    if let Some(mode) = opts.mode {
        b = b.int("createMode", i64::from(mode & 0o7777));
    }
    if let Some(t) = opts.modified {
        b = b.int("mtimeMs", system_time_to_ms(t));
    }
    b.build()
}

fn download_opts(opts: &ReadOptions) -> Opts {
    let mut b = OptsBuilder::new().str("lane", Lane::Bulk.wire());
    if opts.offset > 0 {
        b = b.int("offset", to_i64(opts.offset));
    }
    b.build()
}

fn to_i64(v: u64) -> i64 {
    i64::try_from(v).unwrap_or(i64::MAX)
}

pub(crate) fn folder_checks(count: usize, results: Vec<WireCheckResult>) -> Vec<FolderCheck> {
    let mut out: Vec<Option<FolderCheck>> = (0..count).map(|_| None).collect();
    for r in results {
        let Some(slot) = usize::try_from(r.index).ok().and_then(|i| out.get_mut(i)) else {
            continue;
        };
        if let Some(e) = finished_error(&r.error, "", &HashMap::new()) {
            *slot = Some(FolderCheck::Failed(e));
            continue;
        }
        let children = r.children.iter().map(entry_from_wire);
        match slot {
            Some(FolderCheck::Listed(_, all)) if r.listed => all.extend(children),
            _ if r.listed => *slot = Some(FolderCheck::Listed(entry_from_wire(&r.entry), children.collect())),
            _ => *slot = Some(FolderCheck::Unchanged(entry_from_wire(&r.entry))),
        }
    }
    out.into_iter()
        .map(|slot| {
            slot.unwrap_or_else(|| {
                FolderCheck::Failed(Error::new(
                    ErrorKind::ProtocolError,
                    "the bridge left a folder out",
                ))
            })
        })
        .collect()
}

pub(crate) fn found_below(root: &VPath, items: &[WireWalkItem]) -> Vec<(VPath, Entry)> {
    items
        .iter()
        .filter_map(|item| {
            let mut path = root.clone();
            for part in item.path.split(|b| *b == b'/') {
                path = path.join(part).ok()?;
            }
            (path.name() == Some(item.entry.name.as_slice())).then(|| (path, entry_from_wire(&item.entry)))
        })
        .collect()
}

fn with_access(mut caps: Capabilities, level: Option<Level>) -> Capabilities {
    let flags: &[&str] = match level {
        None | Some(Level::Full) => &[],
        Some(Level::ReadAdd) => &[cap::NO_CHANGE],
        Some(Level::Read) => &[cap::READ_ONLY],
        Some(Level::Add) => &[cap::NO_CHANGE, cap::NO_READ],
    };
    caps.raw.extend(flags.iter().map(|f| (*f).to_owned()));
    caps
}

#[async_trait]
impl Provider for NetvfsProvider {
    fn capabilities(&self) -> Capabilities {
        let caps = self
            .caps
            .read()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .clone()
            .unwrap_or_else(|| Capabilities::with(&[cap::WRITE]));
        with_access(caps, self.client.access(&self.loc))
    }

    async fn known_capabilities(&self) -> Capabilities {
        if !self.has_capabilities() {
            if let Err(e) = self.refresh_capabilities().await {
                log::debug!("capabilities of {}: {e}", self.loc);
            }
        }
        self.capabilities()
    }

    async fn check_folders(
        &self,
        folders: &[KnownFolder],
        now_ms: i64,
        lane: Lane,
    ) -> Result<Vec<FolderCheck>> {
        if !self.client.offers(feature::CHECK) {
            return Ok(check_one_by_one(self, folders, now_ms, lane).await);
        }
        let link = self.link()?;
        let mut out = Vec::with_capacity(folders.len());
        for chunk in folders.chunks(MAX_CHECK_ITEMS) {
            let items: Vec<WireCheckItem> = chunk
                .iter()
                .map(|f| WireCheckItem {
                    path: f.path.as_bytes().to_vec(),
                    etag: f.etag.clone().unwrap_or_default(),
                    mtime_ms: f.modified_ms.unwrap_or(-1),
                })
                .collect();
            let results = link.check(&self.loc, &items, lane).await?;
            out.extend(folder_checks(chunk.len(), results));
        }
        Ok(out)
    }

    async fn search_names(&self, root: &VPath, text: &str, limit: u32) -> Result<Vec<(VPath, Entry)>> {
        if !self.client.offers(feature::SEARCH) || !self.capabilities().raw.contains(cap::SEARCH) {
            return Err(Error::new(ErrorKind::Unsupported, "this server cannot search"));
        }
        let found = self
            .link()?
            .search(&self.loc, root.as_bytes(), text, limit)
            .await?;
        Ok(found_below(root, &found))
    }

    async fn list(&self, dir: &VPath, lane: Lane, out: mpsc::Sender<Vec<Entry>>) -> Result<()> {
        self.link()?.list(&self.loc, dir.as_bytes(), lane, &out).await
    }

    async fn stat(&self, path: &VPath, follow: bool, lane: Lane) -> Result<Entry> {
        let wire = rpc(self
            .link()?
            .proxy()
            .stat(&self.loc, path.as_bytes(), follow, lane.wire())
            .await)?;
        Ok(entry_from_wire(&wire))
    }

    async fn read_link(&self, path: &VPath) -> Result<Vec<u8>> {
        rpc(self.link()?.proxy().read_link(&self.loc, path.as_bytes()).await)
    }

    async fn make_dir(&self, path: &VPath, exclusive: bool) -> Result<()> {
        rpc(self
            .link()?
            .proxy()
            .make_dir(&self.loc, path.as_bytes(), exclusive)
            .await)
    }

    async fn make_file(&self, path: &VPath) -> Result<()> {
        let (reader, writer) = rustix::pipe::pipe_with(rustix::pipe::PipeFlags::CLOEXEC)?;
        drop(writer);
        let opts = WriteOptions {
            disposition: Disposition::Create,
            size: Some(0),
            ..WriteOptions::default()
        };
        self.upload_from(reader, path, opts, no_progress()).await
    }

    async fn remove_file(&self, path: &VPath) -> Result<()> {
        rpc(self.link()?.proxy().remove_file(&self.loc, path.as_bytes()).await)
    }

    async fn remove_dir(&self, path: &VPath) -> Result<()> {
        rpc(self.link()?.proxy().remove_dir(&self.loc, path.as_bytes()).await)
    }

    async fn rename(&self, from: &VPath, to: &VPath, mode: RenameMode) -> Result<()> {
        let replace = mode == RenameMode::Replace;
        rpc(self
            .link()?
            .proxy()
            .rename(&self.loc, from.as_bytes(), to.as_bytes(), replace)
            .await)
    }

    async fn set_attributes(&self, path: &VPath, changes: AttributeChanges) -> Result<()> {
        let mut b = OptsBuilder::new();
        if let Some(mode) = changes.mode {
            b = b.int("mode", i64::from(mode & 0o7777));
        }
        if let Some(t) = changes.modified {
            b = b.int("mtimeMs", system_time_to_ms(t));
        }
        let wire = b.build();
        if wire.is_empty() {
            return Ok(());
        }
        rpc(self
            .link()?
            .proxy()
            .set_attributes(&self.loc, path.as_bytes(), &wire)
            .await)
    }

    async fn make_symlink(&self, target: &[u8], link: &VPath) -> Result<()> {
        rpc(self
            .link()?
            .proxy()
            .make_symlink(&self.loc, target, link.as_bytes())
            .await)
    }

    async fn make_hardlink(&self, existing: &VPath, new_path: &VPath) -> Result<()> {
        rpc(self
            .link()?
            .proxy()
            .make_hardlink(&self.loc, existing.as_bytes(), new_path.as_bytes())
            .await)
    }

    async fn open_read(&self, path: &VPath, lane: Lane) -> Result<Box<dyn ReadHandle>> {
        let link = self.link()?;
        let (handle, size) = rpc(link
            .proxy()
            .open_read(&self.loc, path.as_bytes(), lane.wire())
            .await)?;
        Ok(Box::new(NetvfsReadHandle {
            link,
            handle,
            size: u64::try_from(size).ok(),
        }))
    }

    async fn upload_from(
        &self,
        src: OwnedFd,
        dst: &VPath,
        opts: WriteOptions,
        progress: ProgressSink,
    ) -> Result<()> {
        let link = self.link()?;
        let wire = upload_opts(&opts);
        let start = async {
            let started = link
                .proxy()
                .upload(&self.loc, dst.as_bytes(), Fd::from(&src), &wire)
                .await;
            drop(src);
            started
        };
        link.run_job(start, &progress).await.map(|_| ())
    }

    async fn download_into(
        &self,
        src: &VPath,
        dst: OwnedFd,
        opts: ReadOptions,
        progress: ProgressSink,
    ) -> Result<()> {
        let link = self.link()?;
        let wire = download_opts(&opts);
        let start = async {
            let started = link
                .proxy()
                .download(&self.loc, src.as_bytes(), Fd::from(&dst), &wire)
                .await;
            drop(dst);
            started
        };
        link.run_job(start, &progress).await.map(|_| ())
    }

    async fn server_copy(
        &self,
        from: &VPath,
        to: &VPath,
        opts: CopyOptions,
        _progress: ProgressSink,
    ) -> Result<()> {
        let wire = OptsBuilder::new().bool("replace", opts.replace).build();
        rpc(self
            .link()?
            .proxy()
            .server_copy(&self.loc, from.as_bytes(), to.as_bytes(), &wire)
            .await)
    }

    async fn checksum(&self, path: &VPath, algorithm: &str) -> Result<Vec<u8>> {
        rpc(self
            .link()?
            .proxy()
            .checksum(&self.loc, path.as_bytes(), algorithm)
            .await)
    }

    async fn space(&self, dir: &VPath) -> Result<SpaceInfo> {
        let (free, total, used) = rpc(self.link()?.proxy().space_info(&self.loc, dir.as_bytes()).await)?;
        let n = |v: i64| u64::try_from(v).unwrap_or(0);
        Ok(SpaceInfo {
            free: n(free),
            total: n(total),
            used: n(used),
        })
    }
}

struct NetvfsReadHandle {
    link: Arc<Link>,
    handle: u32,
    size: Option<u64>,
}

#[async_trait]
impl ReadHandle for NetvfsReadHandle {
    fn size(&self) -> Option<u64> {
        self.size
    }

    async fn read_at(&self, offset: u64, max: usize) -> Result<Vec<u8>> {
        if max == 0 {
            return Ok(Vec::new());
        }
        let offset =
            i64::try_from(offset).map_err(|_| Error::new(ErrorKind::InvalidArgument, "offset too large"))?;
        let want = u32::try_from(max).unwrap_or(u32::MAX).min(MAX_READ_BYTES);
        rpc(self
            .link
            .proxy()
            .read(self.handle, offset, want)
            .await
            .map(|b| b.0))
    }

    async fn read_ahead(&self, offset: u64, bytes: u64) -> Result<()> {
        let offset =
            i64::try_from(offset).map_err(|_| Error::new(ErrorKind::InvalidArgument, "offset too large"))?;
        rpc(self
            .link
            .proxy()
            .read_ahead(self.handle, offset, to_i64(bytes))
            .await)
    }
}

impl Drop for NetvfsReadHandle {
    fn drop(&mut self) {
        if let Ok(rt) = tokio::runtime::Handle::try_current() {
            let link = self.link.clone();
            let handle = self.handle;
            rt.spawn(async move {
                let _ = link.proxy().close(handle).await;
            });
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use lautta_bridge_proto::{value_bool, value_i64, value_str};
    use std::time::{Duration, UNIX_EPOCH};

    fn int(o: &Opts, k: &str) -> Option<i64> {
        o.get(k).and_then(value_i64)
    }

    #[test]
    fn levels_narrow_the_capabilities() {
        let server = Capabilities::with(&[cap::WRITE, cap::SERVER_COPY]);
        let at = |level| with_access(server.clone(), level);
        assert_eq!(at(None), server);
        assert_eq!(at(Some(Level::Full)), server);
        let read = at(Some(Level::Read));
        assert!(!read.writable() && read.readable() && read.has(cap::SERVER_COPY));
        let read_add = at(Some(Level::ReadAdd));
        assert!(read_add.writable() && !read_add.changeable() && read_add.readable());
        let add = at(Some(Level::Add));
        assert!(add.writable() && !add.changeable() && !add.readable());
    }

    #[test]
    fn upload_options_for_a_fresh_write() {
        let o = upload_opts(&WriteOptions {
            disposition: Disposition::Truncate,
            size: Some(10),
            mode: Some(0o100600),
            modified: Some(UNIX_EPOCH + Duration::from_millis(1234)),
            ..WriteOptions::default()
        });
        assert_eq!(
            o.get("disposition").and_then(value_str).as_deref(),
            Some("truncate")
        );
        assert_eq!(o.get("lane").and_then(value_str).as_deref(), Some("bulk"));
        assert_eq!(
            (int(&o, "size"), int(&o, "createMode"), int(&o, "mtimeMs")),
            (Some(10), Some(0o600), Some(1234))
        );
        assert!(!o.contains_key("offset"));
    }

    #[test]
    fn a_resume_counts_the_length_from_the_offset() {
        let o = upload_opts(&WriteOptions {
            disposition: Disposition::Resume,
            offset: 4,
            size: Some(10),
            ..WriteOptions::default()
        });
        assert_eq!(
            o.get("disposition").and_then(value_str).as_deref(),
            Some("resume")
        );
        assert_eq!((int(&o, "offset"), int(&o, "size")), (Some(4), Some(6)));
    }

    #[test]
    fn resuming_at_zero_is_a_fresh_write() {
        let o = upload_opts(&WriteOptions {
            disposition: Disposition::Resume,
            ..WriteOptions::default()
        });
        assert_eq!(
            o.get("disposition").and_then(value_str).as_deref(),
            Some("truncate")
        );
        assert!(!o.contains_key("offset"));
        let c = upload_opts(&WriteOptions::default());
        assert_eq!(
            c.get("disposition").and_then(value_str).as_deref(),
            Some("create")
        );
    }

    #[test]
    fn download_options_carry_the_offset_only_when_resuming() {
        assert!(!download_opts(&ReadOptions::default()).contains_key("offset"));
        let o = download_opts(&ReadOptions { offset: 7 });
        assert_eq!(int(&o, "offset"), Some(7));
        assert!(o.get("lane").and_then(value_str).is_some());
        assert!(o.get("nothing").and_then(value_bool).is_none());
    }
}
