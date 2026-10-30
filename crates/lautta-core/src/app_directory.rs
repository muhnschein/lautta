// SPDX-License-Identifier: LGPL-2.1-or-later
//! Directory area actions on `Core`: streaming listings, path menu, folder picker locations.

use crate::app::Core;
use crate::entry::Entry;
use crate::error::Result;
use crate::filter::fold_text;
use crate::foldercount::{count_entries, FolderCount, COUNT_CAP};
use crate::locations::Location;
use crate::provider::Lane;
use crate::uri::Uri;
use crate::vpath::{display_name, VPath};
use percent_encoding::percent_decode_str;
use std::ffi::OsStr;
use std::os::unix::ffi::OsStrExt;
use std::path::Path;
use tokio::sync::mpsc::{self, UnboundedSender};

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

    pub fn edit_address(&self, uri: &Uri) -> String {
        match self.locations.to_local_path(uri) {
            Some(path) => home_relative(&self.paths.home, &path),
            None if uri.path.is_root() => "/".to_owned(),
            None => format!("/{}", uri.path.display()),
        }
    }

    /// Local absolute paths, `~/…`, `file://` and `lautta://` work everywhere;
    /// `/…` is a path inside `base`'s location when that is remote.
    pub fn resolve_address(&self, text: &str, base: &Uri) -> Option<Uri> {
        let text = text.trim();
        if text.starts_with("lautta://") {
            return Uri::parse(text).ok();
        }
        if let Some(rest) = text.strip_prefix("file://") {
            let bytes: Vec<u8> = percent_decode_str(rest).collect();
            return self.local_uri(Path::new(OsStr::from_bytes(&bytes)));
        }
        if text == "~" || text.starts_with("~/") {
            let rest = text.trim_start_matches('~').trim_start_matches('/');
            return self.local_uri(&self.paths.home.join(rest));
        }
        if !text.starts_with('/') {
            return None;
        }
        if self
            .locations
            .to_local_path(&Uri::root(base.location.clone()))
            .is_some()
        {
            return self.local_uri(Path::new(text));
        }
        let path = VPath::parse(text.as_bytes()).ok()?;
        Some(Uri::new(base.location.clone(), path))
    }

    fn local_uri(&self, path: &Path) -> Option<Uri> {
        self.locations.uri_for_local_path(path)
    }

    pub fn complete_address(&self, prefix: &str, base: &Uri) -> Vec<Uri> {
        let Some(cut) = prefix.rfind('/') else {
            return Vec::new();
        };
        let (parent_text, partial) = (&prefix[..=cut], &prefix[cut + 1..]);
        let Some(parent) = self.resolve_address(parent_text, base) else {
            return Vec::new();
        };
        let Ok(Some(hit)) = self.dircache.get(&parent) else {
            return Vec::new();
        };
        let needle = fold_text(partial);
        let mut names: Vec<&Entry> = hit
            .entries
            .iter()
            .filter(|e| e.is_dir() && fold_text(&e.display_name()).starts_with(&needle))
            .collect();
        names.sort_by_key(|e| fold_text(&e.display_name()));
        names
            .into_iter()
            .filter_map(|e| parent.join(&e.name).ok())
            .collect()
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

fn home_relative(home: &Path, path: &Path) -> String {
    match path.strip_prefix(home) {
        Ok(rest) if rest.as_os_str().is_empty() => "~".to_owned(),
        Ok(rest) => format!("~/{}", String::from_utf8_lossy(rest.as_os_str().as_bytes())),
        Err(_) => String::from_utf8_lossy(path.as_os_str().as_bytes()).into_owned(),
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
