// SPDX-License-Identifier: LGPL-2.1-or-later

use crate::app::Core;
use crate::error::{Error, ErrorKind, Result};
use crate::locations::{Location, LocationKind};
use crate::search::{
    self, classify_by_extension, MatchMode, SearchHit, SearchOptions, SearchQuery, SearchSummary,
};
use crate::settings::LocationPrefs;
use crate::uri::{LocationId, Uri};
use crate::vpath::display_name;
use chrono::{DateTime, Datelike, Duration, Local, TimeZone};
use std::collections::BTreeSet;
use std::path::Path;
use std::sync::atomic::AtomicBool;
use std::sync::Arc;
use std::time::SystemTime;
use tokio::sync::mpsc;

const MB: u64 = 1_000_000;
const KB: u64 = 1_000;

pub const SEARCH_TYPES: [&str; 7] = ["folder", "image", "video", "audio", "document", "archive", "text"];
pub const SIZE_PRESETS: [&str; 6] = ["any", "gt1m", "gt10m", "gt100m", "lt100k", "lt1m"];
pub const DATE_PRESETS: [&str; 5] = ["any", "today", "week", "month", "year"];

#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct SearchRequest {
    pub text: String,
    pub mode: MatchMode,
    pub types: BTreeSet<String>,
    pub size_preset: String,
    pub date_preset: String,
    pub include_hidden: bool,
}

pub fn size_bounds(preset: &str) -> (Option<u64>, Option<u64>) {
    match preset {
        "gt1m" => (Some(MB), None),
        "gt10m" => (Some(10 * MB), None),
        "gt100m" => (Some(100 * MB), None),
        "lt100k" => (None, Some(100 * KB)),
        "lt1m" => (None, Some(MB)),
        _ => (None, None),
    }
}

pub fn date_lower_bound(preset: &str, now: DateTime<Local>) -> Option<SystemTime> {
    let midnight =
        |date: chrono::NaiveDate| Local.from_local_datetime(&date.and_hms_opt(0, 0, 0)?).earliest();
    let bound = match preset {
        "today" => midnight(now.date_naive())?,
        "week" => now - Duration::days(7),
        "month" => now - Duration::days(30),
        "year" => midnight(chrono::NaiveDate::from_ymd_opt(now.year(), 1, 1)?)?,
        _ => return None,
    };
    Some(SystemTime::from(bound))
}

impl SearchRequest {
    pub fn is_restricted(&self) -> bool {
        !self.text.trim().is_empty()
            || !self.types.is_empty()
            || size_bounds(&self.size_preset) != (None, None)
            || date_lower_bound(&self.date_preset, Local::now()).is_some()
    }

    pub fn narrows(&self, prior: &SearchRequest) -> bool {
        let unset = |preset: &str| preset.is_empty() || preset == "any";
        let (text, prior_text) = (self.text.trim(), prior.text.trim());
        let text_narrows = match (self.mode, prior.mode) {
            (MatchMode::Substring, MatchMode::Substring) => {
                text.to_lowercase().contains(&prior_text.to_lowercase())
            }
            _ => self.mode == prior.mode && text == prior_text,
        };
        text_narrows
            && self.include_hidden == prior.include_hidden
            && (prior.types.is_empty() || (!self.types.is_empty() && self.types.is_subset(&prior.types)))
            && (unset(&prior.size_preset) || self.size_preset == prior.size_preset)
            && (unset(&prior.date_preset) || self.date_preset == prior.date_preset)
    }

    pub fn narrow<T>(&self, items: Vec<T>, hit: impl Fn(&T) -> &SearchHit) -> Result<Vec<T>> {
        search::filter_hits(items, hit, &self.to_query(Local::now()), &classify_by_extension)
    }

    pub fn to_query(&self, now: DateTime<Local>) -> SearchQuery {
        let (min_size, max_size) = size_bounds(&self.size_preset);
        SearchQuery {
            text: self.text.trim().to_owned(),
            mode: self.mode,
            types: self
                .types
                .iter()
                .filter(|t| SEARCH_TYPES.contains(&t.as_str()))
                .cloned()
                .collect(),
            min_size,
            max_size,
            modified_after: date_lower_bound(&self.date_preset, now),
            modified_before: None,
            include_hidden: self.include_hidden,
        }
    }
}

fn lower_chars(s: &str) -> Vec<char> {
    s.chars().map(|c| c.to_lowercase().next().unwrap_or(c)).collect()
}

fn literal_needle(text: &str, mode: MatchMode) -> String {
    match mode {
        MatchMode::Substring => text.to_owned(),
        MatchMode::Glob => text
            .split(['*', '?', '[', ']'])
            .max_by_key(|piece| piece.chars().count())
            .unwrap_or("")
            .to_owned(),
    }
}

pub fn highlight_span(name: &str, text: &str, mode: MatchMode) -> Option<(usize, usize)> {
    let needle = lower_chars(&literal_needle(text.trim(), mode));
    if needle.is_empty() {
        return None;
    }
    let hay = lower_chars(name);
    hay.windows(needle.len())
        .position(|w| w == needle.as_slice())
        .map(|start| (start, needle.len()))
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SearchScope {
    Folder,
    Device,
    Everywhere,
    Location(LocationId),
}

pub const LOCATION_SCOPE_PREFIX: &str = "location:";

impl SearchScope {
    pub fn parse(name: &str) -> Option<SearchScope> {
        match name {
            "folder" => Some(SearchScope::Folder),
            "device" => Some(SearchScope::Device),
            "everywhere" => Some(SearchScope::Everywhere),
            _ => name
                .strip_prefix(LOCATION_SCOPE_PREFIX)
                .filter(|id| !id.is_empty())
                .map(|id| SearchScope::Location(id.to_owned())),
        }
    }

    pub fn recents_key(&self, folder: Option<&Uri>) -> String {
        match (self, folder) {
            (SearchScope::Folder, Some(f)) => f.location.clone(),
            (SearchScope::Folder, None) => String::new(),
            (SearchScope::Device, _) => "@device".to_owned(),
            (SearchScope::Everywhere, _) => "@everywhere".to_owned(),
            (SearchScope::Location(id), _) => id.clone(),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ScopeLocation {
    pub id: LocationId,
    pub name: String,
    pub kind: GroupKind,
    pub icon: &'static str,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum GroupKind {
    Folder,
    Device,
    Volume,
    Server,
}

impl GroupKind {
    pub fn name(self) -> &'static str {
        match self {
            GroupKind::Folder => "folder",
            GroupKind::Device => "device",
            GroupKind::Volume => "volume",
            GroupKind::Server => "server",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SearchRoot {
    pub uri: Uri,
    pub name: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SearchGroup {
    pub key: String,
    pub name: String,
    pub kind: GroupKind,
    pub roots: Vec<SearchRoot>,
}

impl SearchGroup {
    pub fn place(&self, root: &SearchRoot, folder: &Uri) -> String {
        match self.kind {
            GroupKind::Folder | GroupKind::Device => section_label(&root.uri, &root.name, folder),
            GroupKind::Volume | GroupKind::Server => {
                let rel = folder
                    .path
                    .strip_prefix(&root.uri.path)
                    .map(|rel| rel.components().map(display_name).collect::<Vec<_>>())
                    .unwrap_or_default();
                if rel.is_empty() {
                    self.name.clone()
                } else {
                    rel.join(" \u{203a} ")
                }
            }
        }
    }
}

#[derive(Debug)]
pub enum ScopeEvent {
    Hits {
        group: usize,
        root: usize,
        hits: Vec<SearchHit>,
    },
    Progress {
        group: usize,
        folders: u64,
    },
    Done {
        group: usize,
        result: Result<SearchSummary>,
    },
    Remembered,
}

pub const PROGRESS_EVERY_FOLDERS: u64 = 25;

fn root_of(location: &Location) -> SearchRoot {
    SearchRoot {
        uri: Uri::root(location.id.clone()),
        name: location.name.clone(),
    }
}

fn location_group_kind(location: &Location) -> Option<GroupKind> {
    match location.kind {
        LocationKind::Volume => Some(GroupKind::Volume),
        LocationKind::Server { .. } => Some(GroupKind::Server),
        _ => None,
    }
}

pub fn scope_groups(
    scope: &SearchScope,
    folder: Option<(&Uri, &str)>,
    locations: &[Location],
) -> Vec<SearchGroup> {
    if let SearchScope::Location(id) = scope {
        return locations
            .iter()
            .filter(|l| &l.id == id)
            .filter_map(|l| {
                location_group_kind(l).map(|kind| SearchGroup {
                    key: l.id.clone(),
                    name: l.name.clone(),
                    kind,
                    roots: vec![root_of(l)],
                })
            })
            .collect();
    }
    if *scope == SearchScope::Folder {
        return folder
            .map(|(uri, name)| SearchGroup {
                key: uri.to_string(),
                name: name.to_owned(),
                kind: GroupKind::Folder,
                roots: vec![SearchRoot {
                    uri: uri.clone(),
                    name: name.to_owned(),
                }],
            })
            .into_iter()
            .collect();
    }
    let mut groups = Vec::new();
    let device: Vec<SearchRoot> = locations
        .iter()
        .filter(|l| matches!(l.kind, LocationKind::UserFolder | LocationKind::Android))
        .map(root_of)
        .collect();
    if !device.is_empty() {
        groups.push(SearchGroup {
            key: "@device".to_owned(),
            name: String::new(),
            kind: GroupKind::Device,
            roots: device,
        });
    }
    let single = |kind: GroupKind, l: &Location| SearchGroup {
        key: l.id.clone(),
        name: l.name.clone(),
        kind,
        roots: vec![root_of(l)],
    };
    groups.extend(
        locations
            .iter()
            .filter(|l| l.kind == LocationKind::Volume)
            .map(|l| single(GroupKind::Volume, l)),
    );
    if *scope == SearchScope::Everywhere {
        groups.extend(
            locations
                .iter()
                .filter(|l| matches!(l.kind, LocationKind::Server { .. }))
                .map(|l| single(GroupKind::Server, l)),
        );
    }
    groups
}

pub fn section_label(root: &Uri, root_name: &str, folder: &Uri) -> String {
    let mut parts = vec![root_name.to_owned()];
    if let Some(rel) = folder.path.strip_prefix(&root.path) {
        parts.extend(rel.components().map(display_name));
    }
    parts.join(" \u{203a} ")
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct CacheSizes {
    pub thumbnails: u64,
    pub listings: u64,
    pub archives: u64,
}

impl CacheSizes {
    pub fn total(&self) -> u64 {
        self.thumbnails + self.listings + self.archives
    }
}

pub fn dir_size(path: &Path) -> u64 {
    let Ok(read) = std::fs::read_dir(path) else {
        return 0;
    };
    read.filter_map(Result::ok)
        .map(|e| match e.file_type() {
            Ok(t) if t.is_dir() => dir_size(&e.path()),
            Ok(t) if t.is_file() => e.metadata().map(|m| m.len()).unwrap_or(0),
            _ => 0,
        })
        .sum()
}

fn remove_contents(dir: &Path) -> Result<u64> {
    let freed = dir_size(dir);
    let read = match std::fs::read_dir(dir) {
        Ok(r) => r,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(0),
        Err(e) => return Err(e.into()),
    };
    for entry in read {
        let entry = entry?;
        if entry.file_type()?.is_dir() {
            std::fs::remove_dir_all(entry.path())?;
        } else {
            std::fs::remove_file(entry.path())?;
        }
    }
    Ok(freed)
}

fn remove_tree(dir: &Path) -> Result<()> {
    match std::fs::remove_dir_all(dir) {
        Ok(()) => Ok(()),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(()),
        Err(e) => Err(e.into()),
    }
}

impl Core {
    pub fn search_depth(&self, root: &Uri) -> Option<u32> {
        if self.locations.to_local_path(root).is_some() {
            None
        } else {
            Some(self.settings().search_remote_depth)
        }
    }

    pub async fn search_tree(
        &self,
        root: Uri,
        request: &SearchRequest,
        cancel: Arc<AtomicBool>,
        out: mpsc::Sender<Vec<SearchHit>>,
    ) -> Result<SearchSummary> {
        let query = request.to_query(Local::now());
        let options = SearchOptions::new(self.search_depth(&root), cancel);
        if !query.text.is_empty() {
            let (searches, location, text) = (
                self.recent_searches.clone(),
                root.location.clone(),
                query.text.clone(),
            );
            let _ = tokio::task::spawn_blocking(move || searches.add(&location, &text)).await;
        }
        search::search(
            &self.locations,
            root,
            &query,
            options,
            &classify_by_extension,
            out,
        )
        .await
    }

    pub fn scope_locations(&self) -> Vec<ScopeLocation> {
        self.locations
            .locations()
            .into_iter()
            .filter_map(|l| {
                let kind = location_group_kind(&l)?;
                let icon = match kind {
                    GroupKind::Volume => crate::app_home::drive_icon(
                        self.locations
                            .volume_kind(&l.id)
                            .unwrap_or(crate::bridge::DriveKind::Usb),
                    ),
                    _ => crate::app_home::SERVER_ICON,
                };
                Some(ScopeLocation {
                    id: l.id,
                    name: l.name,
                    kind,
                    icon,
                })
            })
            .collect()
    }

    pub fn search_groups(&self, scope: &SearchScope, folder: Option<&Uri>) -> Vec<SearchGroup> {
        let named = folder.map(|uri| (uri, self.folder_name(uri)));
        scope_groups(
            scope,
            named.as_ref().map(|(uri, name)| (*uri, name.as_str())),
            &self.locations.locations(),
        )
    }

    fn folder_name(&self, uri: &Uri) -> String {
        match uri.name() {
            Some(n) => String::from_utf8_lossy(n).into_owned(),
            None => self.locations.name(&uri.location).unwrap_or_default(),
        }
    }

    pub async fn search_scope(
        &self,
        groups: &[SearchGroup],
        request: &SearchRequest,
        recents_key: &str,
        cancel: Arc<AtomicBool>,
        out: mpsc::Sender<ScopeEvent>,
    ) {
        let query = request.to_query(Local::now());
        if !query.text.is_empty() && !recents_key.is_empty() {
            let (searches, key, text) = (
                self.recent_searches.clone(),
                recents_key.to_owned(),
                query.text.clone(),
            );
            let _ = tokio::task::spawn_blocking(move || searches.add(&key, &text)).await;
            let _ = out.send(ScopeEvent::Remembered).await;
        }
        let runs = groups
            .iter()
            .enumerate()
            .map(|(index, group)| self.search_group(index, group, &query, cancel.clone(), out.clone()));
        futures::future::join_all(runs).await;
    }

    async fn search_group(
        &self,
        group: usize,
        spec: &SearchGroup,
        query: &search::SearchQuery,
        cancel: Arc<AtomicBool>,
        out: mpsc::Sender<ScopeEvent>,
    ) {
        let mut total = SearchSummary::default();
        let mut failure = None;
        let mut searched = 0usize;
        for (root_index, root) in spec.roots.iter().enumerate() {
            if cancel.load(std::sync::atomic::Ordering::Relaxed) {
                break;
            }
            let before = total.folders;
            let progress_out = out.clone();
            let progress: search::FolderProgress = Arc::new(move |folders: u64| {
                if folders.is_multiple_of(PROGRESS_EVERY_FOLDERS) {
                    let _ = progress_out.try_send(ScopeEvent::Progress {
                        group,
                        folders: before + folders,
                    });
                }
            });
            let options =
                SearchOptions::new(self.search_depth(&root.uri), cancel.clone()).with_progress(progress);
            let (tx, mut rx) = mpsc::channel::<Vec<SearchHit>>(4);
            let forward_out = out.clone();
            let forward = async move {
                while let Some(hits) = rx.recv().await {
                    if forward_out
                        .send(ScopeEvent::Hits {
                            group,
                            root: root_index,
                            hits,
                        })
                        .await
                        .is_err()
                    {
                        break;
                    }
                }
            };
            let walk = search::search(
                &self.locations,
                root.uri.clone(),
                query,
                options,
                &classify_by_extension,
                tx,
            );
            let (result, ()) = tokio::join!(walk, forward);
            match result {
                Ok(summary) => {
                    searched += 1;
                    total.folders += summary.folders;
                    total.hits += summary.hits;
                    total.errors += summary.errors;
                    total.canceled |= summary.canceled;
                }
                Err(e) => {
                    total.errors += 1;
                    failure.get_or_insert(e);
                }
            }
        }
        let result = match failure {
            Some(e) if searched == 0 => Err(e),
            _ => Ok(total),
        };
        let _ = out.send(ScopeEvent::Done { group, result }).await;
    }

    pub fn recent_searches_of(&self, root: &Uri) -> Vec<String> {
        self.recent_searches.list(&root.location).unwrap_or_default()
    }

    pub fn cache_sizes(&self) -> CacheSizes {
        let listings: i64 = self
            .db
            .lock()
            .query_row(
                "SELECT coalesce(sum(length(entries)), 0) FROM dircache",
                [],
                |r| r.get(0),
            )
            .unwrap_or(0);
        CacheSizes {
            thumbnails: dir_size(&self.paths.thumbs_dir()),
            listings: u64::try_from(listings).unwrap_or(0),
            archives: dir_size(&self.paths.cache_dir().join("archives")),
        }
    }

    pub fn clear_cache(&self) -> Result<u64> {
        let before = self.cache_sizes();
        remove_contents(&self.paths.thumbs_dir())?;
        remove_contents(&self.paths.cache_dir().join("archives"))?;
        self.dircache.clear()?;
        Ok(before.total())
    }

    pub fn clear_app_data(&self) -> Result<()> {
        if self.engine.pending_summary() > 0 {
            return Err(Error::new(ErrorKind::Locked, "transfers are still running"));
        }
        self.wipe_tables()?;
        self.dircache.clear()?;
        remove_contents(&self.paths.trash_dir())?;
        remove_contents(&self.paths.cache_dir())?;
        remove_tree(&self.paths.data_dir().join("netvfs"))
    }

    fn wipe_tables(&self) -> Result<()> {
        let mut conn = self.db.lock();
        let tx = conn.transaction()?;
        let names = {
            let mut stmt = tx.prepare(
                "SELECT name FROM sqlite_master WHERE type = 'table' AND name NOT LIKE 'sqlite_%'",
            )?;
            let rows = stmt.query_map([], |r| r.get::<_, String>(0))?;
            rows.collect::<std::result::Result<Vec<_>, _>>()?
        };
        for name in names {
            tx.execute(&format!("DELETE FROM \"{name}\""), [])?;
        }
        tx.commit()?;
        Ok(())
    }

    pub fn set_location_prefs(&self, prefs: &LocationPrefs) -> Result<()> {
        self.dircache
            .set_disabled(&prefs.location_id, prefs.no_listing_cache)?;
        self.location_prefs.set(prefs)?;
        let lanes = prefs.bulk_lanes.unwrap_or(self.settings().remote_lanes);
        self.engine.set_remote_limit(&prefs.location_id, lanes as usize);
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::vpath::VPath;

    fn loc(id: &str, kind: LocationKind, name: &str) -> Location {
        Location::remote(id, kind, name, None)
    }

    fn server() -> LocationKind {
        LocationKind::Server {
            provider: "sftp".into(),
        }
    }

    fn fleet() -> Vec<Location> {
        vec![
            loc("user-documents", LocationKind::UserFolder, "Documents"),
            loc("user-pictures", LocationKind::UserFolder, "Pictures"),
            loc("android-dcim", LocationKind::Android, "DCIM"),
            loc("vol-sd", LocationKind::Volume, "SD card"),
            loc("nv-nas", server(), "NAS"),
            loc("arc-1", LocationKind::Archive, "photos.zip"),
        ]
    }

    fn summary(groups: &[SearchGroup]) -> Vec<(String, GroupKind, Vec<String>)> {
        groups
            .iter()
            .map(|g| {
                (
                    g.key.clone(),
                    g.kind,
                    g.roots.iter().map(|r| format!("{}={}", r.uri, r.name)).collect(),
                )
            })
            .collect()
    }

    #[test]
    fn device_scope_groups_user_and_android_folders_and_lists_volumes_apart() {
        assert_eq!(
            summary(&scope_groups(&SearchScope::Device, None, &fleet())),
            vec![
                (
                    "@device".to_owned(),
                    GroupKind::Device,
                    vec![
                        "lautta://user-documents/=Documents".to_owned(),
                        "lautta://user-pictures/=Pictures".to_owned(),
                        "lautta://android-dcim/=DCIM".to_owned()
                    ]
                ),
                (
                    "vol-sd".to_owned(),
                    GroupKind::Volume,
                    vec!["lautta://vol-sd/=SD card".to_owned()]
                ),
            ]
        );
    }

    #[test]
    fn everywhere_adds_servers_after_the_device_and_never_archives() {
        let groups = scope_groups(&SearchScope::Everywhere, None, &fleet());
        let kinds: Vec<(&str, GroupKind)> = groups.iter().map(|g| (g.key.as_str(), g.kind)).collect();
        assert_eq!(
            kinds,
            vec![
                ("@device", GroupKind::Device),
                ("vol-sd", GroupKind::Volume),
                ("nv-nas", GroupKind::Server)
            ]
        );
        assert_eq!(groups[2].name, "NAS");
    }

    #[test]
    fn without_device_folders_there_is_no_device_group() {
        let only_server = vec![loc("nv-nas", server(), "NAS")];
        let groups = scope_groups(&SearchScope::Everywhere, None, &only_server);
        assert_eq!(groups.len(), 1);
        assert_eq!(groups[0].kind, GroupKind::Server);
        assert!(scope_groups(&SearchScope::Device, None, &only_server).is_empty());
    }

    #[test]
    fn folder_scope_is_the_folder_alone() {
        let folder = Uri::parse("lautta://nv-nas/srv/Uni").unwrap();
        assert_eq!(
            summary(&scope_groups(
                &SearchScope::Folder,
                Some((&folder, "Uni")),
                &fleet()
            )),
            vec![(
                "lautta://nv-nas/srv/Uni".to_owned(),
                GroupKind::Folder,
                vec!["lautta://nv-nas/srv/Uni=Uni".to_owned()]
            )]
        );
        assert!(scope_groups(&SearchScope::Folder, None, &fleet()).is_empty());
    }

    fn req(text: &str, types: &[&str], date: &str, hidden: bool) -> SearchRequest {
        SearchRequest {
            text: text.into(),
            mode: MatchMode::for_text(text),
            types: types.iter().map(|t| (*t).to_owned()).collect(),
            size_preset: "any".into(),
            date_preset: date.into(),
            include_hidden: hidden,
        }
    }

    #[test]
    fn a_request_narrows_one_it_only_adds_to() {
        let base = req("test", &[], "any", false);
        let narrower = [
            req("test", &[], "any", false),
            req("tests", &[], "any", false),
            req("My TEST", &[], "any", false),
            req("test", &["document"], "any", false),
            req("test", &[], "today", false),
            req("test", &["document"], "week", false),
        ];
        for r in &narrower {
            assert!(r.narrows(&base), "{r:?}");
        }
        let wider_or_other = [
            req("tes", &[], "any", false),
            req("best", &[], "any", false),
            req("test", &[], "any", true),
            req("test*", &[], "any", false),
        ];
        for r in &wider_or_other {
            assert!(!r.narrows(&base), "{r:?}");
        }
        assert!(!req("budget", &[], "any", false).narrows(&req("", &["document"], "any", false)));
        assert!(req("budget", &["document"], "any", false).narrows(&req("", &["document"], "any", false)));
        let typed = req("test", &["document", "image"], "today", true);
        assert!(req("test", &["document"], "today", true).narrows(&typed));
        assert!(!req("test", &[], "today", true).narrows(&typed));
        assert!(!req("test", &["video"], "today", true).narrows(&typed));
        assert!(!req("test", &["document"], "any", true).narrows(&typed));
        assert!(!req("test", &["document"], "week", true).narrows(&typed));
        assert!(!req("test", &["document"], "today", false).narrows(&typed));
        let glob = req("*.pdf", &[], "any", false);
        assert!(req("*.pdf", &["document"], "any", false).narrows(&glob));
        assert!(!req("a*.pdf", &[], "any", false).narrows(&glob));
        let sized = SearchRequest {
            size_preset: "gt1m".into(),
            ..base.clone()
        };
        assert!(sized.narrows(&base));
        assert!(!base.narrows(&sized));
        assert!(!SearchRequest {
            size_preset: "gt10m".into(),
            ..base.clone()
        }
        .narrows(&sized));
    }

    #[test]
    fn narrowing_filters_earlier_hits_with_the_search_matcher() {
        use crate::entry::{Entry, Kind};
        let hit = |name: &str, kind: Kind| SearchHit {
            uri: Uri::parse(&format!("lautta://l/{name}")).unwrap(),
            entry: Entry::new(name.as_bytes(), kind),
            folder_uri: Uri::parse("lautta://l/").unwrap(),
        };
        let hits = vec![
            (0, hit("test-plan.odt", Kind::File)),
            (1, hit("test.jpg", Kind::File)),
            (0, hit("tests", Kind::Dir)),
            (1, hit("Latest notes.pdf", Kind::File)),
        ];
        let names = |kept: Vec<(i32, SearchHit)>| -> Vec<String> {
            kept.into_iter()
                .map(|(g, h)| format!("{g}:{}", h.entry.display_name()))
                .collect()
        };
        assert_eq!(
            names(
                req("test", &["document"], "any", false)
                    .narrow(hits.clone(), |(_, h)| h)
                    .unwrap()
            ),
            vec!["0:test-plan.odt", "1:Latest notes.pdf"]
        );
        assert_eq!(
            names(
                req("tests", &[], "any", false)
                    .narrow(hits.clone(), |(_, h)| h)
                    .unwrap()
            ),
            vec!["0:tests"]
        );
        assert_eq!(
            names(
                req("test", &["folder"], "any", false)
                    .narrow(hits, |(_, h)| h)
                    .unwrap()
            ),
            vec!["0:tests"]
        );
    }

    #[test]
    fn a_location_scope_is_that_server_or_volume_alone() {
        let nas = scope_groups(&SearchScope::Location("nv-nas".into()), None, &fleet());
        assert_eq!(
            summary(&nas),
            vec![(
                "nv-nas".to_owned(),
                GroupKind::Server,
                vec!["lautta://nv-nas/=NAS".to_owned()]
            )]
        );
        assert_eq!(nas[0].name, "NAS");
        let sd = scope_groups(&SearchScope::Location("vol-sd".into()), None, &fleet());
        assert_eq!(
            summary(&sd),
            vec![(
                "vol-sd".to_owned(),
                GroupKind::Volume,
                vec!["lautta://vol-sd/=SD card".to_owned()]
            )]
        );
        for id in ["user-documents", "arc-1", "nv-gone"] {
            assert!(
                scope_groups(&SearchScope::Location(id.into()), None, &fleet()).is_empty(),
                "{id} is no location scope"
            );
        }
    }

    #[test]
    fn places_name_the_root_on_the_device_and_only_the_path_on_servers() {
        let groups = scope_groups(&SearchScope::Everywhere, None, &fleet());
        let docs = Uri::parse("lautta://user-documents/Finance/2026").unwrap();
        assert_eq!(
            groups[0].place(&groups[0].roots[0], &docs),
            "Documents \u{203a} Finance \u{203a} 2026"
        );
        let nas = &groups[2];
        let deep = Uri::new("nv-nas", VPath::parse(b"/home/shared/Plans").unwrap());
        assert_eq!(
            nas.place(&nas.roots[0], &deep),
            "home \u{203a} shared \u{203a} Plans"
        );
        assert_eq!(nas.place(&nas.roots[0], &Uri::root("nv-nas")), "NAS");
        let sd = &groups[1];
        assert_eq!(sd.place(&sd.roots[0], &Uri::root("vol-sd")), "SD card");
    }

    #[test]
    fn recents_are_kept_per_folder_location_and_per_wide_scope() {
        let folder = Uri::parse("lautta://user-documents/Uni").unwrap();
        assert_eq!(SearchScope::Folder.recents_key(Some(&folder)), "user-documents");
        assert_eq!(SearchScope::Folder.recents_key(None), "");
        assert_eq!(SearchScope::Device.recents_key(Some(&folder)), "@device");
        assert_eq!(SearchScope::Everywhere.recents_key(None), "@everywhere");
        assert_eq!(SearchScope::parse("device"), Some(SearchScope::Device));
        assert_eq!(SearchScope::parse("folder"), Some(SearchScope::Folder));
        assert_eq!(SearchScope::parse("everywhere"), Some(SearchScope::Everywhere));
        assert_eq!(SearchScope::parse("Device"), None);
        assert_eq!(
            SearchScope::parse("location:nv-nas"),
            Some(SearchScope::Location("nv-nas".into()))
        );
        assert_eq!(SearchScope::parse("location:"), None);
        assert_eq!(
            SearchScope::Location("nv-nas".into()).recents_key(Some(&folder)),
            "nv-nas"
        );
        let names: Vec<&str> = [
            GroupKind::Folder,
            GroupKind::Device,
            GroupKind::Volume,
            GroupKind::Server,
        ]
        .iter()
        .map(|k| k.name())
        .collect();
        assert_eq!(names, vec!["folder", "device", "volume", "server"]);
    }

    fn at(y: i32, m: u32, d: u32, h: u32) -> DateTime<Local> {
        Local.with_ymd_and_hms(y, m, d, h, 30, 0).earliest().unwrap()
    }

    #[test]
    fn size_presets_have_the_documented_bounds() {
        assert_eq!(size_bounds("any"), (None, None));
        assert_eq!(size_bounds("nonsense"), (None, None));
        assert_eq!(size_bounds("gt1m"), (Some(1_000_000), None));
        assert_eq!(size_bounds("gt10m"), (Some(10_000_000), None));
        assert_eq!(size_bounds("gt100m"), (Some(100_000_000), None));
        assert_eq!(size_bounds("lt100k"), (None, Some(100_000)));
        assert_eq!(size_bounds("lt1m"), (None, Some(1_000_000)));
        for p in SIZE_PRESETS {
            assert_eq!(p == "any", size_bounds(p) == (None, None), "{p}");
        }
    }

    #[test]
    fn date_presets_are_relative_to_now() {
        let now = at(2026, 10, 3, 15);
        assert_eq!(date_lower_bound("any", now), None);
        let day = SystemTime::from(Local.with_ymd_and_hms(2026, 10, 3, 0, 0, 0).earliest().unwrap());
        assert_eq!(date_lower_bound("today", now), Some(day));
        assert_eq!(
            date_lower_bound("week", now),
            Some(SystemTime::from(now - Duration::days(7)))
        );
        assert_eq!(
            date_lower_bound("month", now),
            Some(SystemTime::from(now - Duration::days(30)))
        );
        let jan = Local.with_ymd_and_hms(2026, 1, 1, 0, 0, 0).earliest().unwrap();
        assert_eq!(date_lower_bound("year", now), Some(SystemTime::from(jan)));
    }

    #[test]
    fn requests_become_queries() {
        let now = at(2026, 10, 3, 12);
        let req = SearchRequest {
            text: "  thesis* ".to_owned(),
            mode: MatchMode::Glob,
            types: ["image".to_owned(), "bogus".to_owned()].into_iter().collect(),
            size_preset: "gt10m".to_owned(),
            date_preset: "year".to_owned(),
            include_hidden: true,
        };
        let q = req.to_query(now);
        assert_eq!(q.text, "thesis*");
        assert_eq!(q.mode, MatchMode::Glob);
        assert_eq!(q.types.iter().collect::<Vec<_>>(), ["image"]);
        assert_eq!((q.min_size, q.max_size), (Some(10_000_000), None));
        assert!(q.modified_after.is_some() && q.modified_before.is_none());
        assert!(q.include_hidden);
    }

    #[test]
    fn restricted_means_something_narrows_the_search() {
        let mut req = SearchRequest::default();
        assert!(!req.is_restricted());
        req.text = "  ".to_owned();
        assert!(!req.is_restricted(), "blank text does not narrow");
        req.text = "a".to_owned();
        assert!(req.is_restricted());
        req.text.clear();
        req.types.insert("image".to_owned());
        assert!(req.is_restricted());
        req.types.clear();
        req.size_preset = "lt1m".to_owned();
        assert!(req.is_restricted());
        req.size_preset.clear();
        req.date_preset = "week".to_owned();
        assert!(req.is_restricted());
    }

    #[test]
    fn highlight_finds_the_literal_part() {
        let sub = MatchMode::Substring;
        let glob = MatchMode::Glob;
        assert_eq!(highlight_span("My Thesis_final.pdf", "thesis", sub), Some((3, 6)));
        assert_eq!(highlight_span("thesis_final.pdf", "thesis*", glob), Some((0, 6)));
        assert_eq!(highlight_span("IMG_2026.raw", "*.raw", glob), Some((8, 4)));
        assert_eq!(highlight_span("a_long_name.txt", "a*long?", glob), Some((2, 4)));
        assert_eq!(highlight_span("abc", "", sub), None);
        assert_eq!(highlight_span("abc", "*", glob), None);
        assert_eq!(highlight_span("abc", "xyz", sub), None);
    }

    #[test]
    fn section_labels_walk_down_from_the_root() {
        let root = Uri::parse("lautta://user-documents/").unwrap();
        let deep = Uri::parse("lautta://user-documents/Uni/Thesis%20b").unwrap();
        assert_eq!(
            section_label(&root, "Documents", &deep),
            "Documents \u{203a} Uni \u{203a} Thesis b"
        );
        assert_eq!(section_label(&root, "Documents", &root), "Documents");
        let sub = Uri::parse("lautta://user-documents/Uni").unwrap();
        assert_eq!(section_label(&sub, "Uni", &deep), "Uni \u{203a} Thesis b");
    }

    #[test]
    fn dir_size_sums_files_and_ignores_links() {
        let dir = tempfile::tempdir().unwrap();
        std::fs::create_dir(dir.path().join("a")).unwrap();
        std::fs::write(dir.path().join("a/x"), [0u8; 10]).unwrap();
        std::fs::write(dir.path().join("y"), [0u8; 5]).unwrap();
        std::os::unix::fs::symlink(dir.path().join("y"), dir.path().join("link")).unwrap();
        assert_eq!(dir_size(dir.path()), 15);
        assert_eq!(dir_size(&dir.path().join("missing")), 0);
    }

    #[test]
    fn remove_contents_keeps_the_folder() {
        let dir = tempfile::tempdir().unwrap();
        std::fs::create_dir(dir.path().join("a")).unwrap();
        std::fs::write(dir.path().join("a/x"), [0u8; 10]).unwrap();
        std::fs::write(dir.path().join("y"), [0u8; 5]).unwrap();
        assert_eq!(remove_contents(dir.path()).unwrap(), 15);
        assert!(dir.path().is_dir());
        assert_eq!(std::fs::read_dir(dir.path()).unwrap().count(), 0);
        assert_eq!(remove_contents(&dir.path().join("missing")).unwrap(), 0);
    }

    #[test]
    fn cache_total_adds_up() {
        let c = CacheSizes {
            thumbnails: 1,
            listings: 20,
            archives: 300,
        };
        assert_eq!(c.total(), 321);
    }
}
