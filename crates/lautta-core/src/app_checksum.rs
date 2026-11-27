// SPDX-License-Identifier: LGPL-2.1-or-later

use crate::app::Core;
use crate::checksum::list::{self, ListEntry};
use crate::checksum::{digest_handle, digest_reader, Algorithm, Cancel};
use crate::entry::{cap, Kind};
use crate::error::{Error, ErrorKind, Result};
use crate::provider::{list_all, read_all, Lane, ProgressSink, Provider};
use crate::uri::Uri;
use crate::vpath::VPath;
use std::sync::Arc;

const MAX_LISTS: usize = 16;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Listed {
    pub list_name: Vec<u8>,
    pub algorithm: Algorithm,
    pub digest: Vec<u8>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Verdict {
    Matches(Vec<u8>),
    Differs(Vec<u8>),
}

pub fn verdict(listed: &[Listed], algorithm: Algorithm, digest: &[u8]) -> Option<Verdict> {
    let same = || listed.iter().filter(|l| l.algorithm == algorithm);
    if let Some(l) = same().find(|l| l.digest == digest) {
        return Some(Verdict::Matches(l.list_name.clone()));
    }
    same().next().map(|l| Verdict::Differs(l.list_name.clone()))
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ItemState {
    Matches,
    Differs,
    Missing,
    NotChecked,
    Failed(ErrorKind),
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum VerifyEvent {
    Listed(Vec<Vec<u8>>),
    Progress {
        index: usize,
        done: u64,
        total: Option<u64>,
    },
    Checked {
        index: usize,
        state: ItemState,
    },
}

pub fn resolve(dir: &Uri, name: &[u8]) -> Option<Uri> {
    if name.first() == Some(&b'/') {
        return None;
    }
    let rel = VPath::parse(name).ok().filter(|p| !p.is_root())?;
    Some(Uri::new(dir.location.clone(), dir.path.join_path(&rel)))
}

impl Core {
    pub async fn checksum(
        &self,
        uri: &Uri,
        algorithm: Algorithm,
        progress: ProgressSink,
        cancel: &Cancel,
    ) -> Result<Vec<u8>> {
        let provider = self.provider(&uri.location)?;
        cancel
            .run(file_digest(provider, &uri.path, algorithm, progress, cancel))
            .await
    }

    pub async fn lists_naming(&self, file: &Uri, cancel: &Cancel) -> Result<Vec<Listed>> {
        let (Some(dir), Some(name)) = (file.parent(), file.name()) else {
            return Ok(Vec::new());
        };
        let provider = self.provider(&file.location)?;
        let work = async {
            let mut lists: Vec<_> = list_all(provider.as_ref(), &dir.path, Lane::Interactive)
                .await?
                .into_iter()
                .filter(|e| {
                    e.kind == Kind::File
                        && e.name != name
                        && e.size.is_some_and(|s| s <= list::MAX_BYTES)
                        && list::list_kind(&e.name).is_some()
                })
                .collect();
            lists.sort_by_key(|e| (!e.name.starts_with(name), e.name.clone()));
            let mut out = Vec::new();
            for entry in lists.into_iter().take(MAX_LISTS) {
                let Ok(path) = dir.path.join(&entry.name) else {
                    continue;
                };
                let Ok(entries) = read_list(provider.as_ref(), &path, &entry.name).await else {
                    continue;
                };
                out.extend(
                    entries
                        .into_iter()
                        .filter(|e| resolve(&dir, &e.name).as_ref() == Some(file))
                        .map(|e| Listed {
                            list_name: entry.name.clone(),
                            algorithm: e.algorithm,
                            digest: e.digest,
                        }),
                );
            }
            Ok(out)
        };
        cancel.run(work).await
    }

    pub async fn verify_list(
        &self,
        list: &Uri,
        events: Arc<dyn Fn(VerifyEvent) + Send + Sync>,
        cancel: &Cancel,
    ) -> Result<()> {
        let (Some(dir), Some(list_name)) = (list.parent(), list.name()) else {
            return Err(Error::kind(ErrorKind::InvalidArgument));
        };
        let provider = self.provider(&list.location)?;
        let work = async {
            let entries = read_list(provider.as_ref(), &list.path, list_name).await?;
            events(VerifyEvent::Listed(
                entries.iter().map(|e| e.name.clone()).collect(),
            ));
            for (index, entry) in entries.iter().enumerate() {
                let item_events = events.clone();
                let progress: ProgressSink = Arc::new(move |done, total| {
                    item_events(VerifyEvent::Progress { index, done, total });
                });
                let state = check_entry(provider.clone(), &dir, entry, progress, cancel).await?;
                events(VerifyEvent::Checked { index, state });
            }
            Ok(())
        };
        cancel.run(work).await
    }
}

async fn read_list(provider: &dyn Provider, path: &VPath, name: &[u8]) -> Result<Vec<ListEntry>> {
    let entry = provider.stat(path, false, Lane::Interactive).await?;
    let listed = list::list_kind(name).is_some();
    if !listed || entry.kind != Kind::File || entry.size.is_some_and(|s| s > list::MAX_BYTES) {
        return Err(Error::new(ErrorKind::Unsupported, "not a checksum list"));
    }
    let handle = provider.open_read(path, Lane::Interactive).await?;
    let data = read_all(handle.as_ref(), list::MAX_BYTES + 1).await?;
    list::parse(name, &data)
}

async fn check_entry(
    provider: Arc<dyn Provider>,
    dir: &Uri,
    entry: &ListEntry,
    progress: ProgressSink,
    cancel: &Cancel,
) -> Result<ItemState> {
    let Some(target) = resolve(dir, &entry.name) else {
        return Ok(ItemState::NotChecked);
    };
    let found = match provider.stat(&target.path, false, Lane::Interactive).await {
        Ok(e) => e,
        Err(e) if e.kind == ErrorKind::NotFound => return Ok(ItemState::Missing),
        Err(e) => return Ok(ItemState::Failed(e.kind)),
    };
    if found.kind != Kind::File {
        return Ok(ItemState::NotChecked);
    }
    match file_digest(provider, &target.path, entry.algorithm, progress, cancel).await {
        Ok(d) if d == entry.digest => Ok(ItemState::Matches),
        Ok(_) => Ok(ItemState::Differs),
        Err(e) if e.kind == ErrorKind::Canceled => Err(e),
        Err(e) if e.kind == ErrorKind::NotFound => Ok(ItemState::Missing),
        Err(e) => Ok(ItemState::Failed(e.kind)),
    }
}

async fn file_digest(
    provider: Arc<dyn Provider>,
    path: &VPath,
    algorithm: Algorithm,
    progress: ProgressSink,
    cancel: &Cancel,
) -> Result<Vec<u8>> {
    let entry = provider.stat(path, true, Lane::Interactive).await?;
    if entry.kind != Kind::File && entry.target_kind != Kind::File {
        return Err(Error::kind(if entry.kind == Kind::Dir {
            ErrorKind::IsADirectory
        } else {
            ErrorKind::Unsupported
        }));
    }
    if let Some(fd) = provider.open_local(path).await? {
        let file = std::fs::File::from(fd);
        let total = entry.size;
        let cancel = cancel.clone();
        return tokio::task::spawn_blocking(move || digest_reader(file, algorithm, total, progress, &cancel))
            .await
            .map_err(|e| Error::new(ErrorKind::Internal, format!("checksum thread: {e}")))?;
    }
    let caps = provider.known_capabilities().await;
    if caps.has(cap::CHECKSUMS) && caps.checksum_algorithms.iter().any(|a| a == algorithm.id()) {
        match provider.checksum(path, algorithm.id()).await {
            Ok(d) if d.len() == algorithm.digest_len() => return Ok(d),
            Ok(_) => return Err(Error::new(ErrorKind::ProtocolError, "checksum of wrong length")),
            Err(e) if e.kind == ErrorKind::Unsupported => {}
            Err(e) => return Err(e),
        }
    }
    let handle = provider.open_read(path, Lane::Bulk).await?;
    digest_handle(handle, algorithm, progress, cancel).await
}
