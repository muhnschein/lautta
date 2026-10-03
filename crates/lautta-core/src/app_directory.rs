// SPDX-License-Identifier: LGPL-2.1-or-later
//! User-level actions of the directory area on [`Core`](crate::app::Core):
//! streaming listings (BRW-5, BRW-8), the path menu (BRW-9) and the folder
//! picker's location list.

use crate::app::Core;
use crate::entry::Entry;
use crate::error::Result;
use crate::filter::fold_text;
use crate::locations::Location;
use crate::provider::Lane;
use crate::uri::Uri;
use crate::vpath::{display_name, VPath};
use percent_encoding::percent_decode_str;
use std::ffi::OsStr;
use std::os::unix::ffi::OsStrExt;
use std::path::Path;
use tokio::sync::mpsc::{self, UnboundedSender};

/// What a folder listing reports while it runs.
#[derive(Debug)]
pub enum ListEvent {
    /// The cached listing (BRW-5); only sent when asked for and present.
    Cached { entries: Vec<Entry>, fetched_ms: i64 },
    /// A batch of the fresh listing, in provider order (BRW-8).
    Batch(Vec<Entry>),
    /// The listing ended; always the last event.
    Done(Result<()>),
}

/// One step of the path from a location's root to a folder.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PathStep {
    pub uri: Uri,
    /// The location's name for the root step, otherwise the folder's name.
    pub name: String,
}

/// Batches buffered between a provider and the event sender.
const BATCH_QUEUE: usize = 8;

impl Core {
    /// Lists `dir` as a stream of events. With `use_cache` the cached listing
    /// goes first. The fresh listing refreshes the cache once complete.
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

    /// The capability flags of a location (local: per file system; remote:
    /// from the bridge), sorted, for the UI to filter actions (§10.1).
    pub fn capability_flags(&self, location: &str) -> Vec<String> {
        match self.provider(location) {
            Ok(p) => p.capabilities().raw.into_iter().collect(),
            Err(_) => Vec::new(),
        }
    }

    /// The path from the location's root to `uri` for the path menu (BRW-9):
    /// the root first (named like the location), `uri` last.
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

    /// The text the path editor starts with: local folders as `~/…` or an
    /// absolute path, remote ones as the path inside the location.
    pub fn edit_address(&self, uri: &Uri) -> String {
        match self.locations.to_local_path(uri) {
            Some(path) => home_relative(&self.paths.home, &path),
            None if uri.path.is_root() => "/".to_owned(),
            None => format!("/{}", uri.path.display()),
        }
    }

    /// Turns what the user typed in the path editor into a folder URI. Local
    /// absolute paths, `~/…`, `file://` and `lautta://` work everywhere;
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

    /// Folder completions for the path editor from cached listings (BRW-9):
    /// the cached sub-folders of the text before the last `/` whose names
    /// start with what follows it (case and accents ignored).
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

    /// The locations the folder picker starts from (OPS-10), in display
    /// order. Locations that need the user (sign-in, offline) are included
    /// so the picker can show why they are not selectable.
    pub fn picker_locations(&self) -> Vec<Location> {
        self.locations.locations()
    }
}

/// `~/…` for paths inside `home`, the absolute path otherwise.
fn home_relative(home: &Path, path: &Path) -> String {
    match path.strip_prefix(home) {
        Ok(rest) if rest.as_os_str().is_empty() => "~".to_owned(),
        Ok(rest) => format!("~/{}", String::from_utf8_lossy(rest.as_os_str().as_bytes())),
        Err(_) => String::from_utf8_lossy(path.as_os_str().as_bytes()).into_owned(),
    }
}

/// Bytes kept literally in a `file://` URL path.
const URL_SET: &percent_encoding::AsciiSet = &percent_encoding::NON_ALPHANUMERIC
    .remove(b'/')
    .remove(b'-')
    .remove(b'_')
    .remove(b'.')
    .remove(b'~');

/// A `file://` URL for a local path, safe for QML `url` properties even when
/// the name holds `%`, `#`, `?` or spaces (thumbnails, PRV-1).
pub fn file_url(path: &Path) -> String {
    format!(
        "file://{}",
        percent_encoding::percent_encode(path.as_os_str().as_bytes(), URL_SET)
    )
}
