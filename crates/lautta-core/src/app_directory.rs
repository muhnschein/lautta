// SPDX-License-Identifier: LGPL-2.1-or-later
//! Directory area actions on `Core`: streaming listings, path menu, folder picker locations.

use crate::app::Core;
use crate::entry::Entry;
use crate::error::Result;
use crate::filter::fold_text;
use crate::foldercount::{count_entries, FolderCount, COUNT_CAP};
use crate::locations::{Location, LocationKind};
use crate::org::RecentsFilter;
use crate::provider::Lane;
use crate::uri::Uri;
use crate::vpath::{display_name, VPath};
use percent_encoding::percent_decode_str;
use std::collections::HashSet;
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
    /// `AppIcon` name: the location's for its root, a folder's otherwise.
    pub icon: &'static str,
}

/// Where a Go to folder hit came from; also its rank among hits that match equally well.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum HitSource {
    Here,
    Recent,
    Favourite,
    Cached,
}

impl HitSource {
    pub fn as_str(self) -> &'static str {
        match self {
            HitSource::Here => "here",
            HitSource::Recent => "recent",
            HitSource::Favourite => "favourite",
            HitSource::Cached => "cached",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct GotoHit {
    pub uri: Uri,
    pub name: String,
    pub source: HitSource,
    /// Folder holding it, as Recents show it; empty for a location.
    pub place: String,
}

/// A typed path: the folder it names when known to exist, and folders to drill into: that
/// folder's, or the parent's whose names start with the text after the last slash.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct GotoPath {
    pub target: Option<Uri>,
    pub folders: Vec<Uri>,
}

const MAX_HITS: usize = 30;
const CACHED_FOLDERS: usize = 2000;
/// Archive inside archive inside …: path steps follow this many archives out.
const MAX_NESTING: usize = 8;
const FOLDER_ICON: &str = "file-folder";

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

    /// From the location's root; inside an archive, first the folders holding the archive.
    pub fn path_steps(&self, uri: &Uri) -> Vec<PathStep> {
        let mut steps = Vec::new();
        let mut at = Some(uri.clone());
        for _ in 0..MAX_NESTING {
            let Some(uri) = at.take() else { break };
            let location = self.location(&uri.location);
            let mut here = location_steps(&uri, location.as_ref());
            here.append(&mut steps);
            steps = here;
            at = location
                .filter(|l| l.kind == LocationKind::Archive)
                .and_then(|l| l.url)
                .and_then(|u| Uri::parse(&u).ok())
                .and_then(|archive| archive.parent());
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
            return self.location_address(text);
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

    /// `Documents/Uni`: a location's name, then a path inside it.
    fn location_address(&self, text: &str) -> Option<Uri> {
        let (head, rest) = text.split_once('/')?;
        let head = fold_text(head);
        let location = self
            .locations
            .locations()
            .into_iter()
            .find(|l| fold_text(&l.name) == head)?;
        match &location.local_root {
            Some(root) => self.local_uri(&root.join(rest)),
            None => Some(Uri::new(location.id, VPath::parse(rest.as_bytes()).ok()?)),
        }
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

    /// Folders whose name matches `text`: name starts with it, then a word in it does, then it
    /// is anywhere; equal matches by [`HitSource`]. Cache and database only, no listing.
    pub fn goto_hits(&self, text: &str, base: &Uri) -> Vec<GotoHit> {
        let needle = fold_text(text.trim());
        if needle.is_empty() {
            return Vec::new();
        }
        let mut seen = HashSet::from([base.clone()]);
        let mut hits: Vec<(Tier, GotoHit)> = Vec::new();
        for (source, uri) in self.goto_candidates(base) {
            if self.location(&uri.location).is_none() || !seen.insert(uri.clone()) {
                continue;
            }
            let name = self.folder_name(&uri);
            let Some(tier) = match_tier(&needle, &fold_text(&name)) else {
                continue;
            };
            let place = uri.parent().map(|p| self.place_of(&p)).unwrap_or_default();
            hits.push((
                tier,
                GotoHit {
                    uri,
                    name,
                    source,
                    place,
                },
            ));
        }
        hits.sort_by_key(|(tier, _)| *tier);
        hits.into_iter().take(MAX_HITS).map(|(_, h)| h).collect()
    }

    fn goto_candidates(&self, base: &Uri) -> Vec<(HitSource, Uri)> {
        let mut out: Vec<(HitSource, Uri)> = Vec::new();
        if let Ok(Some(hit)) = self.dircache.get(base) {
            out.extend(
                hit.entries
                    .iter()
                    .filter(|e| e.is_dir())
                    .filter_map(|e| base.join(&e.name).ok())
                    .map(|u| (HitSource::Here, u)),
            );
        }
        if self.recents.is_enabled() {
            let recents = self.recents.list(&RecentsFilter::default()).unwrap_or_default();
            out.extend(
                recents
                    .into_iter()
                    .filter_map(|r| r.uri.parent())
                    .map(|u| (HitSource::Recent, u)),
            );
        }
        let favourites = self.favourites.list().unwrap_or_default();
        out.extend(
            favourites
                .into_iter()
                .filter(|f| f.is_dir)
                .map(|f| (HitSource::Favourite, f.uri)),
        );
        let cached = self.dircache.folders(CACHED_FOLDERS).unwrap_or_default();
        out.extend(cached.into_iter().map(|u| (HitSource::Cached, u)));
        out
    }

    fn folder_name(&self, uri: &Uri) -> String {
        match uri.name() {
            Some(n) => display_name(n),
            None => self.location(&uri.location).map(|l| l.name).unwrap_or_default(),
        }
    }

    /// Blocking: may look at the local file system.
    pub fn goto_path(&self, text: &str, base: &Uri) -> GotoPath {
        let target = self
            .resolve_address(text, base)
            .filter(|u| self.is_known_folder(u));
        let folders = match &target {
            Some(_) => self.complete_address(&format!("{}/", text.trim_end()), base),
            None => self.complete_address(text.trim_end(), base),
        };
        GotoPath { target, folders }
    }

    fn is_known_folder(&self, uri: &Uri) -> bool {
        if self.location(&uri.location).is_none() {
            return false;
        }
        if uri.path.is_root() || matches!(self.dircache.get(uri), Ok(Some(_))) {
            return true;
        }
        if let Some(path) = self.locations.to_local_path(uri) {
            return path.is_dir();
        }
        let (Some(parent), Some(name)) = (uri.parent(), uri.name()) else {
            return false;
        };
        matches!(self.dircache.get(&parent), Ok(Some(hit))
            if hit.entries.iter().any(|e| e.is_dir() && e.name == name))
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

/// Text Go to folder treats as a path, not a folder name: `~…`, or anything with a slash.
pub fn looks_like_address(text: &str) -> bool {
    let text = text.trim_start();
    text.starts_with('~') || text.contains('/')
}

fn location_steps(uri: &Uri, location: Option<&Location>) -> Vec<PathStep> {
    let mut steps = vec![PathStep {
        uri: Uri::root(uri.location.clone()),
        name: location.map(|l| l.name.clone()).unwrap_or_default(),
        icon: location.map_or(FOLDER_ICON, |l| location_icon(&l.kind)),
    }];
    let mut current = Uri::root(uri.location.clone());
    for comp in uri.path.components() {
        let Ok(next) = current.join(comp) else { break };
        steps.push(PathStep {
            uri: next.clone(),
            name: display_name(comp),
            icon: FOLDER_ICON,
        });
        current = next;
    }
    steps
}

fn location_icon(kind: &LocationKind) -> &'static str {
    match kind {
        LocationKind::UserFolder | LocationKind::Android => FOLDER_ICON,
        LocationKind::Volume => "image://theme/icon-m-sd-card",
        LocationKind::Server { .. } | LocationKind::AdHoc => "server",
        LocationKind::Archive => "file-archive",
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
enum Tier {
    Prefix,
    WordPrefix,
    Inside,
}

/// Both folded. A word starts after any character that is not a letter or digit.
fn match_tier(needle: &str, name: &str) -> Option<Tier> {
    if name.starts_with(needle) {
        return Some(Tier::Prefix);
    }
    let mut word_start = false;
    for (i, c) in name.char_indices() {
        if word_start && name[i..].starts_with(needle) {
            return Some(Tier::WordPrefix);
        }
        word_start = !c.is_alphanumeric();
    }
    name.contains(needle).then_some(Tier::Inside)
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
