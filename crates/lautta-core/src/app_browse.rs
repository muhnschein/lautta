// SPDX-License-Identifier: LGPL-2.1-or-later
//! User-level actions of the browse area on [`Core`](crate::app::Core): the
//! rows of the Browse page (SPEC §15.2), recent ad-hoc servers (NVB-6),
//! volume space (LOC-3) and the purge of what the app remembers about a
//! removed account (SEC-5).
//!
//! Everything blocking here runs in `spawn_blocking`; the Qt layer only
//! turns the returned values into model rows.

use crate::app::Core;
use crate::bridge::{AdHocOptions, BridgeStatus, NearbyServer, RemoteKind};
use crate::error::{Error, ErrorKind, Result};
use crate::locations::{Location, LocationKind, LocationStatus};
use crate::org::{self, Recent, RecentsFilter};
use crate::settings::LocationPrefs;
use crate::uri::Uri;
use crate::vpath::{display_name, VPath};
use rusqlite::params;
use std::collections::{BTreeMap, BTreeSet};
use std::sync::{Arc, Weak};
use zeroize::Zeroizing;

/// Separator of the "Location › folder" lines.
const PLACE_SEPARATOR: &str = " › ";

/// One row of the Browse page, with plain values only (the Qt layer maps
/// them to model roles; QML turns engineering values into translated text).
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct Row {
    /// `favourites`, `device`, `android`, `volumes`, `servers`, `nearby`.
    pub section: &'static str,
    /// `favourite`, `folder`, `deleted`, `volume`, `account`, `adhoc`,
    /// `adhocRecent`, `consent`, `unavailable`, `nearby`.
    pub kind: &'static str,
    pub uri: String,
    pub name: String,
    pub icon: String,
    /// `ready`, `connecting`, `offline`, `attention`.
    pub status: &'static str,
    /// `auth-failed`, `server-identity-changed` or empty.
    pub attention: String,
    pub colour: String,
    /// Items in the folder, `-1` when not shown.
    pub count: i64,
    /// Favourite or location id; the nearby index.
    pub item_id: String,
    /// `Location › folder` for favourites, the address for servers.
    pub place: String,
    /// Protocol label such as `SFTP`.
    pub provider: String,
    pub host: String,
    pub free: i64,
    pub total: i64,
    pub fs: String,
}

impl Row {
    fn new(section: &'static str, kind: &'static str) -> Row {
        Row {
            section,
            kind,
            status: "ready",
            count: -1,
            free: -1,
            total: -1,
            ..Row::default()
        }
    }

    /// Identity of the row, to tell a changed row from a new one.
    pub fn key(&self) -> (&'static str, &'static str, &str, &str) {
        (self.section, self.kind, &self.uri, &self.item_id)
    }
}

/// Free and total space and the file system of a volume (LOC-3).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct VolumeInfo {
    pub free: u64,
    pub total: u64,
    pub fs: String,
}

/// A recent ad-hoc server, shown in Servers while not connected (NVB-6).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AdHocRecent {
    pub location_id: String,
    /// Address without secrets.
    pub url: String,
    pub name: String,
    pub last_used_ms: i64,
}

/// A recents row with what the page shows (ORG-2).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RecentRow {
    pub id: i64,
    pub uri: String,
    pub name: String,
    pub kind: &'static str,
    pub place: String,
    pub at_ms: i64,
    pub day: &'static str,
}

/// The name of the file system with `statfs` magic `magic`, for the volume
/// line ("58.2 GB free of 128 GB · exFAT"). Unknown ones are `""`.
pub fn fs_kind_name(magic: u64) -> &'static str {
    match magic {
        0x2011_BAB0 => "exFAT",
        0x4d44 => "vfat",
        0x5346_544e => "NTFS",
        0x6573_5546 => "FUSE",
        0xEF53 => "ext4",
        0x9123_683E => "btrfs",
        0xF2F5_2010 => "f2fs",
        0x5846_5342 => "XFS",
        0x0102_1994 => "tmpfs",
        _ => "",
    }
}

/// Protocol label for a netvfs provider name.
pub fn provider_label(provider: &str) -> String {
    match provider {
        "sftp" | "ssh" => "SFTP".to_owned(),
        "smb" | "cifs" => "SMB".to_owned(),
        "webdav" | "dav" | "davs" => "WebDAV".to_owned(),
        "ftp" => "FTP".to_owned(),
        "ftps" => "FTPS".to_owned(),
        "nfs" => "NFS".to_owned(),
        other => other.to_uppercase(),
    }
}

/// The URL scheme `Connect to server` understands for a netvfs provider.
fn scheme_for(provider: &str) -> &str {
    match provider {
        "webdav" | "dav" => "davs",
        other => other,
    }
}

/// The address a nearby server prefills in *Connect to server* (LOC-6).
pub fn nearby_url(n: &NearbyServer) -> String {
    let default_port = match n.provider.as_str() {
        "sftp" | "ssh" => 22,
        "smb" | "cifs" => 445,
        "ftp" => 21,
        "webdav" | "dav" | "davs" => 443,
        _ => 0,
    };
    let port = if n.port == 0 || n.port == default_port {
        String::new()
    } else {
        format!(":{}", n.port)
    };
    let path = n.path.display();
    format!("{}://{}{port}/{path}", scheme_for(&n.provider), n.host)
}

/// `scheme://user@host/path` without password, query or fragment.
pub fn public_url(url: &str) -> String {
    let cut = url.find(['?', '#']).unwrap_or(url.len());
    let url = &url[..cut];
    let Some(at) = url.find("://") else {
        return url.to_owned();
    };
    let (scheme, rest) = url.split_at(at + 3);
    let end = rest.find('/').unwrap_or(rest.len());
    let (authority, path) = rest.split_at(end);
    match authority.rfind('@') {
        Some(i) => {
            let user = authority[..i].split(':').next().unwrap_or("");
            format!("{scheme}{user}@{}{path}", &authority[i + 1..])
        }
        None => url.to_owned(),
    }
}

/// The host part of a URL, for naming a recent ad-hoc server.
pub fn url_host(url: &str) -> String {
    let rest = url.split_once("://").map_or(url, |(_, r)| r);
    let authority = rest.split('/').next().unwrap_or(rest);
    authority.rsplit('@').next().unwrap_or(authority).to_owned()
}

/// The recents section of `at_ms`: `today`, `yesterday`, `week` (the last
/// seven days) or `earlier`; `offset_secs` is the local UTC offset.
pub fn day_bucket_at(at_ms: i64, now_ms: i64, offset_secs: i64) -> &'static str {
    let day = |ms: i64| (ms / 1000 + offset_secs).div_euclid(86_400);
    match day(now_ms) - day(at_ms) {
        i64::MIN..=0 => "today",
        1 => "yesterday",
        2..=6 => "week",
        _ => "earlier",
    }
}

/// [`day_bucket_at`] in the device's time zone.
pub fn day_bucket(at_ms: i64, now_ms: i64) -> &'static str {
    use chrono::{Local, Offset, TimeZone};
    let offset = Local
        .timestamp_millis_opt(now_ms)
        .single()
        .map_or(0, |t| i64::from(t.offset().fix().local_minus_utc()));
    day_bucket_at(at_ms, now_ms, offset)
}

fn status_name(s: LocationStatus) -> &'static str {
    match s {
        LocationStatus::Ready => "ready",
        LocationStatus::Connecting => "connecting",
        LocationStatus::Offline => "offline",
        LocationStatus::NeedsAttention => "attention",
    }
}

fn clamp_i64(v: u64) -> i64 {
    i64::try_from(v).unwrap_or(i64::MAX)
}

/// Visible items of a local folder (hidden files are not counted).
fn visible_count(path: &std::path::Path) -> Option<i64> {
    let n = std::fs::read_dir(path)
        .ok()?
        .filter_map(Result::ok)
        .filter(|e| !e.file_name().as_encoded_bytes().starts_with(b"."))
        .count();
    Some(clamp_i64(n as u64))
}

/// The account ids (`nv-account:…`) of a bridge location list.
fn account_ids(list: &[crate::bridge::RemoteLocation]) -> BTreeSet<String> {
    list.iter()
        .filter(|l| l.kind == RemoteKind::Account)
        .map(|l| l.id.clone())
        .collect()
}

/// The location id inside a stored `lautta://<location>/…` string.
fn location_of(uri: &str) -> Option<&str> {
    uri.strip_prefix("lautta://")?.split('/').next()
}

impl Core {
    /// `Location › folder` for a URI: components joined by `›` for local
    /// locations, `/path` for remote ones (the board lines of favourites).
    pub fn place_of(&self, uri: &Uri) -> String {
        let name = self
            .location(&uri.location)
            .map(|l| l.name)
            .unwrap_or_else(|| uri.location.clone());
        if uri.path.is_root() {
            return name;
        }
        let remote = self.location(&uri.location).is_some_and(|l| !l.is_local());
        if remote {
            return format!("{name}{PLACE_SEPARATOR}/{}", uri.path.display());
        }
        let parts: Vec<String> = uri.path.components().map(display_name).collect();
        format!("{name}{PLACE_SEPARATOR}{}", parts.join(PLACE_SEPARATOR))
    }

    /// Visible items of the cached listing of a folder (the cover, INT-3);
    /// `None` when nothing is cached.
    pub fn cached_folder_count(&self, uri: &Uri) -> Option<usize> {
        let hit = self.dircache.get(uri).ok()??;
        Some(hit.entries.iter().filter(|e| !e.is_hidden()).count())
    }

    /// Space and file system of a volume (LOC-3), through the provider.
    pub async fn volume_info(&self, uri: &Uri) -> Result<VolumeInfo> {
        let provider = self.provider(&uri.location)?;
        let space = provider.space(&VPath::root()).await?;
        let path = self.locations.to_local_path(uri);
        let fs = match path {
            Some(p) => tokio::task::spawn_blocking(move || crate::sys::fs_magic(&p).ok())
                .await
                .ok()
                .flatten()
                .map(|m| fs_kind_name(m).to_owned())
                .unwrap_or_default(),
            None => String::new(),
        };
        Ok(VolumeInfo {
            free: space.free,
            total: space.total,
            fs,
        })
    }

    /// Every row of the Browse page in display order (SPEC §15.2).
    pub async fn browse_rows(self: &Arc<Self>) -> Vec<Row> {
        let core = self.clone();
        let blocking = tokio::task::spawn_blocking(move || core.local_rows())
            .await
            .unwrap_or_default();
        let mut rows = blocking.favourites;
        rows.extend(blocking.device);
        rows.extend(blocking.android);
        rows.extend(self.volume_rows().await);
        rows.extend(self.server_rows(&blocking.prefs).await);
        rows.extend(self.nearby_rows());
        rows
    }

    fn local_rows(&self) -> LocalRows {
        let prefs: BTreeMap<String, LocationPrefs> = self
            .location_prefs
            .list()
            .unwrap_or_default()
            .into_iter()
            .map(|p| (p.location_id.clone(), p))
            .collect();
        let mut device = self.folder_rows("device", &LocationKind::UserFolder, &prefs);
        if self.settings().recently_deleted {
            let mut row = Row::new("device", "deleted");
            row.icon = "image://theme/icon-m-delete".to_owned();
            row.count = clamp_i64(self.trash.list().map_or(0, |l| l.len()) as u64);
            device.push(row);
        }
        LocalRows {
            favourites: self.favourite_rows(),
            device,
            android: self.folder_rows("android", &LocationKind::Android, &prefs),
            prefs,
        }
    }

    fn folder_rows(
        &self,
        section: &'static str,
        kind: &LocationKind,
        prefs: &BTreeMap<String, LocationPrefs>,
    ) -> Vec<Row> {
        self.locations
            .locations()
            .into_iter()
            .filter(|l| &l.kind == kind)
            .map(|l| {
                let mut row = Row::new(section, "folder");
                row.uri = Uri::root(l.id.clone()).to_string();
                row.name = display_of(&l, prefs);
                row.icon = "image://theme/icon-m-file-folder".to_owned();
                row.count = l.local_root.as_deref().and_then(visible_count).unwrap_or(-1);
                row.item_id = l.id;
                row
            })
            .collect()
    }

    fn favourite_rows(&self) -> Vec<Row> {
        let mut rows = Vec::new();
        for f in self.favourites.list().unwrap_or_default() {
            let mut row = Row::new("favourites", "favourite");
            row.uri = f.uri.to_string();
            row.name = f.label;
            row.colour = f.colour.unwrap_or_default();
            row.place = self.place_of(&f.uri);
            row.icon = "image://theme/icon-m-favorite".to_owned();
            row.item_id = f.id.to_string();
            rows.push(row);
        }
        rows
    }

    async fn volume_rows(&self) -> Vec<Row> {
        let mut rows = Vec::new();
        for l in self
            .locations
            .locations()
            .into_iter()
            .filter(|l| l.kind == LocationKind::Volume)
        {
            let uri = Uri::root(l.id.clone());
            let mut row = Row::new("volumes", "volume");
            row.uri = uri.to_string();
            row.name = l.name.clone();
            row.item_id = l.id.clone();
            row.icon = "image://theme/icon-m-sd-card".to_owned();
            if let Ok(info) = self.volume_info(&uri).await {
                row.free = clamp_i64(info.free);
                row.total = clamp_i64(info.total);
                row.fs = info.fs;
            }
            rows.push(row);
        }
        rows
    }

    /// The Servers section (SPEC §3.4, NVB-3, NVB-12): nothing without a
    /// usable bridge, the consent row until the user decided.
    async fn server_rows(&self, prefs: &BTreeMap<String, LocationPrefs>) -> Vec<Row> {
        let Some(client) = &self.bridge else {
            return Vec::new();
        };
        match client.status() {
            BridgeStatus::ConsentUnknown => {
                let mut row = Row::new("servers", "consent");
                row.icon = "image://theme/icon-m-computer".to_owned();
                vec![row]
            }
            BridgeStatus::Reconnecting => {
                let mut rows = vec![Row::new("servers", "unavailable")];
                rows.extend(self.listed_servers(prefs));
                rows
            }
            BridgeStatus::Ready => {
                let mut rows = self.listed_servers(prefs);
                rows.extend(self.recent_adhoc_rows(&rows));
                rows
            }
            _ => Vec::new(),
        }
    }

    fn listed_servers(&self, prefs: &BTreeMap<String, LocationPrefs>) -> Vec<Row> {
        let Some(client) = &self.bridge else {
            return Vec::new();
        };
        self.locations
            .locations()
            .into_iter()
            .filter(|l| matches!(l.kind, LocationKind::Server { .. } | LocationKind::AdHoc))
            .map(|l| {
                let remote = client.location(&l.id);
                let adhoc = l.kind == LocationKind::AdHoc;
                let mut row = Row::new("servers", if adhoc { "adhoc" } else { "account" });
                let start = prefs
                    .get(&l.id)
                    .and_then(|p| p.start_folder.clone())
                    .or_else(|| {
                        remote
                            .as_ref()
                            .map(|r| Uri::new(l.id.clone(), r.start_path.clone()))
                    })
                    .unwrap_or_else(|| Uri::root(l.id.clone()));
                row.uri = start.to_string();
                row.name = display_of(&l, prefs);
                row.status = status_name(l.status);
                row.attention = l.attention.clone().unwrap_or_default();
                row.icon = "image://theme/icon-m-computer".to_owned();
                row.place = l.url.clone().unwrap_or_default();
                if let Some(r) = &remote {
                    row.provider = provider_label(&r.provider);
                    row.host = r.host.clone();
                }
                row.item_id = l.id;
                row
            })
            .collect()
    }

    /// Recent ad-hoc servers that are not connected now (NVB-6).
    fn recent_adhoc_rows(&self, listed: &[Row]) -> Vec<Row> {
        let urls: BTreeSet<String> = listed.iter().map(|r| r.place.clone()).collect();
        self.adhoc_recents()
            .unwrap_or_default()
            .into_iter()
            .filter(|r| !urls.contains(&r.url))
            .map(|r| {
                let mut row = Row::new("servers", "adhocRecent");
                row.name = r.name;
                row.status = "offline";
                row.icon = "image://theme/icon-m-computer".to_owned();
                row.provider = r
                    .url
                    .split_once("://")
                    .map(|(s, _)| provider_label(s))
                    .unwrap_or_default();
                row.place = r.url.clone();
                row.uri = r.url;
                row.item_id = r.location_id;
                row
            })
            .collect()
    }

    fn nearby_rows(&self) -> Vec<Row> {
        let Some(client) = &self.bridge else {
            return Vec::new();
        };
        if client.status() != BridgeStatus::Ready {
            return Vec::new();
        }
        client
            .nearby()
            .iter()
            .enumerate()
            .map(|(i, n)| {
                let mut row = Row::new("nearby", "nearby");
                row.name = n.name.clone();
                row.provider = provider_label(&n.provider);
                row.host = n.host.clone();
                row.uri = nearby_url(n);
                row.icon = "image://theme/icon-m-wlan".to_owned();
                row.item_id = i.to_string();
                row
            })
            .collect()
    }

    // ---- recent ad-hoc servers (NVB-6) ------------------------------------

    /// Recent ad-hoc servers, newest first.
    pub fn adhoc_recents(&self) -> Result<Vec<AdHocRecent>> {
        let conn = self.db.lock();
        let mut stmt = conn.prepare(
            "SELECT location_id, url, name, last_used_ms FROM adhoc_servers ORDER BY last_used_ms DESC",
        )?;
        let rows = stmt
            .query_map([], |r| {
                Ok(AdHocRecent {
                    location_id: r.get(0)?,
                    url: r.get(1)?,
                    name: r.get(2)?,
                    last_used_ms: r.get(3)?,
                })
            })?
            .collect::<std::result::Result<Vec<_>, _>>()?;
        Ok(rows)
    }

    fn remember_adhoc(&self, location_id: &str, url: &str, name: &str) -> Result<()> {
        let conn = self.db.lock();
        // One row per address: a new connection to a known server replaces it.
        conn.execute("DELETE FROM adhoc_servers WHERE url = ?1", [url])?;
        conn.execute(
            "INSERT OR REPLACE INTO adhoc_servers(location_id, url, name, last_used_ms) VALUES (?1, ?2, ?3, ?4)",
            params![location_id, url, name, now_ms()],
        )?;
        Ok(())
    }

    /// Forgets a recent ad-hoc server (and what the app remembers about it).
    pub fn remove_adhoc_recent(&self, location_id: &str) -> Result<()> {
        self.db
            .lock()
            .execute("DELETE FROM adhoc_servers WHERE location_id = ?1", [location_id])?;
        self.purge_location_data(location_id)
    }

    /// *Connect to server* (NVB-6, SEC-1): the secret is moved into a wiping
    /// buffer at once and wiped when the call ends, whatever its outcome.
    /// Returns the location id.
    pub async fn connect_adhoc(&self, url: &str, secret: Vec<u8>, opts: AdHocOptions) -> Result<String> {
        let mut secret = Zeroizing::new(secret);
        let client = self
            .bridge
            .as_ref()
            .ok_or_else(|| Error::new(ErrorKind::BridgeUnavailable, "network locations are unavailable"))?;
        let id = client.connect_adhoc_wiping(url, &mut secret, &opts).await?;
        let shown = public_url(url);
        self.remember_adhoc(&id, &shown, &url_host(&shown))?;
        Ok(id)
    }

    /// Forgets an ad-hoc server in the bridge and here (XB-14, SEC-5).
    pub async fn forget_adhoc(&self, location_id: &str) -> Result<()> {
        if let Some(client) = &self.bridge {
            // A server that is not connected has nothing to forget there.
            let _ = client.forget_adhoc(location_id).await;
        }
        self.remove_adhoc_recent(location_id)
    }

    // ---- purge (SEC-5) ------------------------------------------------------

    /// Removes everything remembered about a location: favourites, recents,
    /// preferences and cached listings (SEC-5).
    pub fn purge_location_data(&self, location: &str) -> Result<()> {
        org::purge_location(&self.db, location)?;
        self.location_prefs.remove(location)?;
        self.dircache.invalidate_location(location)?;
        Ok(())
    }

    /// Remote account ids that favourites, recents or preferences refer to.
    pub fn remembered_accounts(&self) -> Result<BTreeSet<String>> {
        let conn = self.db.lock();
        let mut found = BTreeSet::new();
        let mut stmt = conn.prepare("SELECT uri FROM favourites")?;
        let uris = stmt
            .query_map([], |r| r.get::<_, String>(0))?
            .collect::<std::result::Result<Vec<_>, _>>()?;
        found.extend(uris.iter().filter_map(|u| location_of(u)).map(str::to_owned));
        for sql in [
            "SELECT DISTINCT location_id FROM recents",
            "SELECT location_id FROM location_prefs",
        ] {
            let mut stmt = conn.prepare(sql)?;
            let ids = stmt
                .query_map([], |r| r.get::<_, String>(0))?
                .collect::<std::result::Result<Vec<_>, _>>()?;
            found.extend(ids);
        }
        found.retain(|id| id.starts_with("nv-account:"));
        Ok(found)
    }

    /// Starts following the bridge's location list: an account that
    /// disappears for good (it was in the list while the bridge was usable
    /// and is not any more) is purged (SEC-5). A list that is empty because
    /// consent was withdrawn or the bridge is away never purges. At the first
    /// usable list, accounts remembered from earlier runs that are gone are
    /// purged too. `on_purged` is called after a purge, so that what shows
    /// the data can read it again. Needs a tokio runtime; ends with the core.
    pub fn spawn_account_purge(self: &Arc<Self>, on_purged: impl Fn() + Send + Sync + 'static) {
        let Some(client) = self.bridge.clone() else {
            return;
        };
        let weak = Arc::downgrade(self);
        tokio::spawn(async move {
            let mut status = client.watch_status();
            let mut locations = client.watch_locations();
            let mut known: Option<BTreeSet<String>> = None;
            loop {
                let current = *status.borrow_and_update();
                let list = locations.borrow_and_update().clone();
                if purge_step(&weak, current, &list, &mut known, &on_purged)
                    .await
                    .is_none()
                {
                    return;
                }
                tokio::select! {
                    changed = status.changed() => if changed.is_err() { return },
                    changed = locations.changed() => if changed.is_err() { return },
                }
            }
        });
    }

    // ---- recents (ORG-2) ----------------------------------------------------

    /// Recents for the page, newest first, with their day sections.
    pub fn recent_rows(&self, filter: &RecentsFilter) -> Result<Vec<RecentRow>> {
        let now = now_ms();
        Ok(self
            .recents
            .list(filter)?
            .into_iter()
            .map(|r| self.recent_row(r, now))
            .collect())
    }

    fn recent_row(&self, r: Recent, now: i64) -> RecentRow {
        let folder = r.uri.parent().unwrap_or_else(|| r.uri.clone());
        // Transfers say where they went: the location, not the folder.
        let place = if r.kind == org::RecentKind::Transferred {
            self.location(&r.uri.location).map(|l| l.name).unwrap_or_default()
        } else {
            self.place_of(&folder)
        };
        RecentRow {
            id: r.id,
            uri: r.uri.to_string(),
            name: r.name,
            kind: r.kind.as_str(),
            place,
            at_ms: r.at_ms,
            day: day_bucket(r.at_ms, now),
        }
    }
}

#[derive(Default)]
struct LocalRows {
    favourites: Vec<Row>,
    device: Vec<Row>,
    android: Vec<Row>,
    prefs: BTreeMap<String, LocationPrefs>,
}

fn display_of(l: &Location, prefs: &BTreeMap<String, LocationPrefs>) -> String {
    prefs
        .get(&l.id)
        .and_then(|p| p.display_name.clone())
        .unwrap_or_else(|| l.name.clone())
}

fn now_ms() -> i64 {
    use std::time::{SystemTime, UNIX_EPOCH};
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_or(0, |d| i64::try_from(d.as_millis()).unwrap_or(i64::MAX))
}

/// One look at the bridge state for [`Core::spawn_account_purge`]. `None`
/// when the core is gone.
async fn purge_step(
    weak: &Weak<Core>,
    status: BridgeStatus,
    list: &[crate::bridge::RemoteLocation],
    known: &mut Option<BTreeSet<String>>,
    on_purged: &impl Fn(),
) -> Option<()> {
    let core = weak.upgrade()?;
    if !matches!(status, BridgeStatus::Ready | BridgeStatus::Reconnecting) {
        return Some(());
    }
    if known.is_none() && status != BridgeStatus::Ready {
        return Some(());
    }
    let current = account_ids(list);
    let gone: Vec<String> = match known.as_ref() {
        Some(k) => k.difference(&current).cloned().collect(),
        None => {
            let c = core.clone();
            let remembered = tokio::task::spawn_blocking(move || c.remembered_accounts())
                .await
                .ok()
                .and_then(|r| r.ok())
                .unwrap_or_default();
            remembered.difference(&current).cloned().collect()
        }
    };
    *known = Some(current);
    if !gone.is_empty() {
        let c = core.clone();
        let _ = tokio::task::spawn_blocking(move || {
            for id in gone {
                if let Err(e) = c.purge_location_data(&id) {
                    log::warn!("cannot purge a removed account: {e}");
                }
            }
        })
        .await;
        on_purged();
    }
    Some(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn file_systems_have_names() {
        assert_eq!(fs_kind_name(0x2011_BAB0), "exFAT");
        assert_eq!(fs_kind_name(0x4d44), "vfat");
        assert_eq!(fs_kind_name(0xEF53), "ext4");
        assert_eq!(fs_kind_name(1), "");
    }

    #[test]
    fn provider_labels() {
        assert_eq!(provider_label("sftp"), "SFTP");
        assert_eq!(provider_label("webdav"), "WebDAV");
        assert_eq!(provider_label("ftps"), "FTPS");
        assert_eq!(provider_label("weird"), "WEIRD");
    }

    #[test]
    fn nearby_urls_skip_default_ports() {
        let mut n = NearbyServer {
            name: "Mac".into(),
            provider: "smb".into(),
            host: "mac-mini.local".into(),
            port: 445,
            path: VPath::parse(b"share/x").unwrap(),
        };
        assert_eq!(nearby_url(&n), "smb://mac-mini.local/share/x");
        n.port = 4445;
        assert_eq!(nearby_url(&n), "smb://mac-mini.local:4445/share/x");
        n.provider = "webdav".into();
        n.port = 443;
        n.path = VPath::root();
        assert_eq!(nearby_url(&n), "davs://mac-mini.local/");
    }

    #[test]
    fn urls_lose_secrets() {
        assert_eq!(
            public_url("sftp://pi:hunter2@raspberrypi.local/home?x=1#f"),
            "sftp://pi@raspberrypi.local/home"
        );
        assert_eq!(public_url("smb://nas/share"), "smb://nas/share");
        assert_eq!(url_host("sftp://pi@raspberrypi.local/home"), "raspberrypi.local");
        assert_eq!(url_host("nas"), "nas");
    }

    #[test]
    fn day_buckets_follow_the_local_day() {
        let day = 86_400_000;
        let now = 10 * day + 3_600_000; // 01:00 UTC
        assert_eq!(day_bucket_at(now, now, 0), "today");
        assert_eq!(day_bucket_at(now - 2 * 3_600_000, now, 0), "yesterday");
        // The same instants at UTC+2 are both on the same local day.
        assert_eq!(day_bucket_at(now - 2 * 3_600_000, now, 7200), "today");
        assert_eq!(day_bucket_at(now - 3 * day, now, 0), "week");
        assert_eq!(day_bucket_at(now - 6 * day, now, 0), "week");
        assert_eq!(day_bucket_at(now - 7 * day, now, 0), "earlier");
        assert_eq!(day_bucket_at(now + day, now, 0), "today");
    }

    #[test]
    fn locations_in_stored_uris() {
        assert_eq!(location_of("lautta://nv-account:1/a/b"), Some("nv-account:1"));
        assert_eq!(location_of("lautta://user-documents/"), Some("user-documents"));
        assert_eq!(location_of("file:///x"), None);
    }

    #[test]
    fn visible_items_are_counted() {
        let dir = tempfile::tempdir().unwrap();
        std::fs::write(dir.path().join("a"), "").unwrap();
        std::fs::write(dir.path().join(".hidden"), "").unwrap();
        std::fs::create_dir(dir.path().join("d")).unwrap();
        assert_eq!(visible_count(dir.path()), Some(2));
        assert_eq!(visible_count(&dir.path().join("none")), None);
    }

    #[test]
    fn row_keys_tell_rows_apart() {
        let mut a = Row::new("favourites", "favourite");
        a.item_id = "1".into();
        let mut b = a.clone();
        assert_eq!(a.key(), b.key());
        b.item_id = "2".into();
        assert_ne!(a.key(), b.key());
        assert_eq!(a.count, -1);
    }
}
