// SPDX-License-Identifier: LGPL-2.1-or-later
//! Directory area actions on `Core`: streaming listings, path steps, folder picker locations.

use crate::app::Core;
use crate::entry::Entry;
use crate::error::Result;
use crate::foldercount::{count_entries, FolderCount, COUNT_CAP};
use crate::locations::Location;
use crate::provider::Lane;
use crate::uri::Uri;
use crate::vpath::display_name;
use std::os::unix::ffi::OsStrExt;
use std::path::Path;
use std::time::Duration;
use tokio::sync::mpsc::{self, UnboundedReceiver, UnboundedSender};
use tokio::time::Instant;

#[derive(Debug)]
pub enum ListEvent {
    Cached { entries: Vec<Entry>, fetched_ms: i64 },
    Batch(Vec<Entry>),
    Done(Result<()>),
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PathStep {
    pub uri: Uri,
    pub name: String,
}

const BATCH_QUEUE: usize = 8;

/// Hands `rx`'s events to `emit`: first batch at once, later batches joined so at most one
/// reaches `emit` per `every`. Each batch costs the GUI a merge and a row insert signal.
pub async fn forward_coalesced(
    mut rx: UnboundedReceiver<ListEvent>,
    every: Duration,
    mut emit: impl FnMut(ListEvent),
) {
    let mut next_flush = Instant::now();
    let mut held: Option<Vec<Entry>> = None;
    loop {
        let ev = match held {
            None => rx.recv().await,
            Some(_) => match tokio::time::timeout_at(next_flush, rx.recv()).await {
                Ok(ev) => ev,
                Err(_) => {
                    emit(ListEvent::Batch(held.take().unwrap_or_default()));
                    next_flush = Instant::now() + every;
                    continue;
                }
            },
        };
        match ev {
            Some(ListEvent::Batch(batch)) if held.is_none() && Instant::now() >= next_flush => {
                emit(ListEvent::Batch(batch));
                next_flush = Instant::now() + every;
            }
            Some(ListEvent::Batch(batch)) => held.get_or_insert_with(Vec::new).extend(batch),
            other => {
                if let Some(batch) = held.take() {
                    emit(ListEvent::Batch(batch));
                }
                match other {
                    Some(ev) => emit(ev),
                    None => return,
                }
            }
        }
    }
}

impl Core {
    pub async fn list_stream(&self, dir: &Uri, use_cache: bool, out: UnboundedSender<ListEvent>) {
        let result = self.stream_listing(dir, use_cache, &out).await;
        let _ = out.send(ListEvent::Done(result));
    }

    async fn stream_listing(
        &self,
        dir: &Uri,
        use_cache: bool,
        out: &UnboundedSender<ListEvent>,
    ) -> Result<()> {
        if use_cache {
            if let Ok(Some(hit)) = self.dircache.get(dir) {
                let _ = out.send(ListEvent::Cached {
                    entries: hit.entries,
                    fetched_ms: hit.fetched_ms,
                });
            }
        }
        let provider = self.provider(&dir.location)?;
        provider.known_capabilities().await;
        let (tx, mut rx) = mpsc::channel::<Vec<Entry>>(BATCH_QUEUE);
        let collect = async {
            let mut all = Vec::new();
            while let Some(batch) = rx.recv().await {
                let _ = out.send(ListEvent::Batch(batch.clone()));
                all.extend(batch);
            }
            all
        };
        let (res, all) = tokio::join!(provider.list(&dir.path, Lane::Interactive, tx), collect);
        res?;
        let _ = self.dircache.put(dir, &all);
        Ok(())
    }

    pub fn capability_flags(&self, location: &str) -> Vec<String> {
        match self.provider(location) {
            Ok(p) => p.capabilities().raw.into_iter().collect(),
            Err(_) => Vec::new(),
        }
    }

    /// Lautta may add files to folder `dir`: its place allows it and, on this device, so do the
    /// folder's permissions.
    pub fn may_add_to(&self, dir: &Uri) -> bool {
        self.provider(&dir.location)
            .is_ok_and(|p| p.capabilities().writable())
            && self
                .locations
                .to_local_path(dir)
                .is_none_or(|path| crate::sys::can_change(&path))
    }

    pub fn path_steps(&self, uri: &Uri) -> Vec<PathStep> {
        let root_name = self.location(&uri.location).map(|l| l.name).unwrap_or_default();
        let mut steps = vec![PathStep {
            uri: Uri::root(uri.location.clone()),
            name: root_name,
        }];
        let mut current = Uri::root(uri.location.clone());
        for comp in uri.path.components() {
            let Ok(next) = current.join(comp) else { break };
            steps.push(PathStep {
                uri: next.clone(),
                name: display_name(comp),
            });
            current = next;
        }
        steps
    }

    /// Local folders only: `None` for servers and archives (counting would list over the network)
    /// and unreadable folders.
    pub async fn count_folder(&self, uri: &Uri, show_hidden: bool) -> Option<FolderCount> {
        let path = self.locations.to_local_path(uri)?;
        let permit = self.locations.io_semaphore().acquire_owned().await.ok()?;
        tokio::task::spawn_blocking(move || {
            let _permit = permit;
            count_entries(&path, show_hidden, COUNT_CAP).ok()
        })
        .await
        .ok()
        .flatten()
    }

    /// Includes locations needing the user (sign-in, offline) so the picker can show why they are
    /// not selectable.
    pub fn picker_locations(&self) -> Vec<Location> {
        self.locations.locations()
    }
}

const URL_SET: &percent_encoding::AsciiSet = &percent_encoding::NON_ALPHANUMERIC
    .remove(b'/')
    .remove(b'-')
    .remove(b'_')
    .remove(b'.')
    .remove(b'~');

/// Safe for QML `url` properties when the name holds `%`, `#`, `?` or spaces.
pub fn file_url(path: &Path) -> String {
    format!(
        "file://{}",
        percent_encoding::percent_encode(path.as_os_str().as_bytes(), URL_SET)
    )
}
