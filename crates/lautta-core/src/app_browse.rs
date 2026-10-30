// SPDX-License-Identifier: LGPL-2.1-or-later
//! Browse area actions on `Core`. Blocking work runs in `spawn_blocking`; Qt layer only maps values
//! to model rows.

use crate::app::Core;
use crate::bridge::{AdHocOptions, BridgeStatus, Drive, DriveKind, NearbyServer, RemoteKind};
use crate::browse_layout::{layout_item, Layout, NEARBY_ITEM, SECTION_ITEM, SWITCHED_SECTIONS};
use crate::date_group::DateGroup;
use crate::error::{Error, ErrorKind, Result};
use crate::locations::{ConnectionState, Location, LocationKind, LocationStatus};
use crate::org::favourites::Favourite;
use crate::org::{self, Recent, RecentsFilter};
use crate::settings::LocationPrefs;
use crate::uri::Uri;
use crate::vpath::{display_name, VPath};
use crate::workcopy::blocking;
use rusqlite::params;
use std::collections::{BTreeMap, BTreeSet};
use std::ffi::OsStr;
use std::path::Path;
use std::sync::{Arc, Weak};
use std::time::Duration;
use zeroize::Zeroizing;

const PLACE_SEPARATOR: &str = " › ";
const SERVER_ICON: &str = "server";

/// Drive mounted by bridge takes time to show in sandbox: this many looks, this far apart.
const MOUNT_LOOKS: u32 = 10;
const MOUNT_LOOK_EVERY: Duration = Duration::from_millis(200);

#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct Row {
    /// `favourites`, `device`, `android`, `volumes`, `servers`, `nearby`.
    pub section: &'static str,
    /// `favourite`, `folder`, `recents`, `deleted`, `volume`, `account`,
    /// `adhoc`, `adhocRecent`, `consent`, `unavailable`, `nearby`.
    pub kind: &'static str,
    pub uri: String,
    pub name: String,
    pub icon: String,
    /// `ready`, `connecting`, `offline`, `attention`; `unmounted` for a mountable
    /// volume, `locked` for one Settings › Storage unlocks first.
    pub status: &'static str,
    /// `auth-failed`, `server-identity-changed` or empty.
    pub attention: String,
    /// `connected`, `connecting`, `disconnected`, `failed`, or empty.
    pub connection: &'static str,
    /// netvfs error name of a `failed` connection (`AuthFailed`, `Timeout`, …).
    pub connection_error: String,
    pub colour: String,
    pub item_id: String,
    pub place: String,
    pub provider: String,
    pub host: String,
    pub free: i64,
    pub total: i64,
    pub fs: String,
    pub is_file: bool,
    pub drive: String,
}

impl Row {
    fn new(section: &'static str, kind: &'static str) -> Row {
        Row {
            section,
            kind,
            status: "ready",
            free: -1,
            total: -1,
            ..Row::default()
        }
    }

    pub fn key(&self) -> (&'static str, &'static str, &str, &str) {
        (self.section, self.kind, &self.uri, &self.item_id)
    }
}

pub const SECTION_KIND: &str = "section";

const SECTIONS: [&str; 5] = ["favourites", "device", "android", "volumes", "servers"];

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LayoutRow {
    pub section: &'static str,
    /// `nearby` stands for every nearby server, [`SECTION_KIND`] for the switch of
    /// *Volumes* or *Servers*.
    pub kind: &'static str,
    pub item: String,
    pub name: String,
    pub hidden: bool,
}

fn is_hidden(layout: &Layout, row: &Row) -> bool {
    layout_item(row.kind, &row.item_id).is_some_and(|item| layout.is_hidden(row.section, &item))
}

fn layout_row(layout: &Layout, row: &Row) -> Option<LayoutRow> {
    let item = match row.kind {
        "favourite" => Some(row.item_id.clone()),
        _ => layout_item(row.kind, &row.item_id),
    }?;
    Some(LayoutRow {
        section: row.section,
        kind: row.kind,
        hidden: is_hidden(layout, row),
        item,
        name: row.name.clone(),
    })
}

/// Switch is always there so an empty section can be switched. No Servers switch standalone.
fn section_rows(
    section: &'static str,
    rows: Vec<LayoutRow>,
    layout: &Layout,
    bridge_there: bool,
) -> Vec<LayoutRow> {
    if !SWITCHED_SECTIONS.contains(&section) {
        return rows;
    }
    if section == "servers" && !bridge_there {
        return Vec::new();
    }
    let shown = layout.section_shown(section);
    let switch = LayoutRow {
        section,
        kind: SECTION_KIND,
        item: SECTION_ITEM.to_owned(),
        name: String::new(),
        hidden: !shown,
    };
    let mut out = vec![switch];
    if shown {
        out.extend(rows);
    }
    out
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct VolumeInfo {
    pub free: u64,
    pub total: u64,
    pub fs: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AdHocRecent {
    pub location_id: String,
    pub url: String,
    pub name: String,
    pub last_used_ms: i64,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RecentRow {
    pub id: i64,
    pub uri: String,
    pub name: String,
    pub kind: &'static str,
    pub place: String,
    pub at_ms: i64,
    pub date_group: DateGroup,
}

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

pub fn provider_label(provider: &str) -> String {
    match provider {
        "sftp" | "ssh" => "SFTP".to_owned(),
        "smb" | "cifs" => "SMB".to_owned(),
        "nfs" => "NFS".to_owned(),
        other => other.to_uppercase(),
    }
}

pub fn nearby_url(n: &NearbyServer) -> String {
    let default_port = match n.provider.as_str() {
        "sftp" | "ssh" => 22,
        "smb" | "cifs" => 445,
        _ => 0,
    };
    let port = if n.port == 0 || n.port == default_port {
        String::new()
    } else {
        format!(":{}", n.port)
    };
    format!("{}://{}{port}/", n.provider, n.host)
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

pub fn url_host(url: &str) -> String {
    let rest = url.split_once("://").map_or(url, |(_, r)| r);
    let authority = rest.split('/').next().unwrap_or(rest);
    authority.rsplit('@').next().unwrap_or(authority).to_owned()
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

fn account_ids(list: &[crate::bridge::RemoteLocation]) -> BTreeSet<String> {
    list.iter()
        .filter(|l| l.kind == RemoteKind::Account)
        .map(|l| l.id.clone())
        .collect()
}

fn location_of(uri: &str) -> Option<&str> {
    uri.strip_prefix("lautta://")?.split('/').next()
}

impl Core {
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

    pub fn favourite_place(&self, f: &Favourite) -> String {
        match f.uri.parent().filter(|_| !f.is_dir) {
            Some(folder) => self.place_of(&folder),
            None => self.place_of(&f.uri),
        }
    }

    pub fn cached_folder_count(&self, uri: &Uri) -> Option<usize> {
        let hit = self.dircache.get(uri).ok()??;
        Some(hit.entries.iter().filter(|e| !e.is_hidden()).count())
    }

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

    pub async fn browse_rows(self: &Arc<Self>) -> Vec<Row> {
        let (rows, layout) = self.arranged_rows().await;
        rows.into_iter()
            .filter(|r| layout.section_shown(r.section) && !is_hidden(&layout, r))
            .collect()
    }

    pub async fn browse_layout_rows(self: &Arc<Self>) -> Vec<LayoutRow> {
        let (rows, layout) = self.arranged_rows().await;
        let bridge_there = self
            .bridge
            .as_ref()
            .is_some_and(|b| b.status() != BridgeStatus::Absent);
        let items: Vec<LayoutRow> = rows
            .iter()
            .filter(|r| r.kind != "nearby")
            .filter_map(|r| layout_row(&layout, r))
            .collect();
        let mut out: Vec<LayoutRow> = SECTIONS
            .into_iter()
            .flat_map(|section| {
                let own = items.iter().filter(|r| r.section == section).cloned().collect();
                section_rows(section, own, &layout, bridge_there)
            })
            .collect();
        if bridge_there {
            out.push(LayoutRow {
                section: "nearby",
                kind: "nearby",
                item: NEARBY_ITEM.to_owned(),
                name: String::new(),
                hidden: layout.is_hidden("nearby", NEARBY_ITEM),
            });
        }
        out
    }

    pub async fn move_browse_row(self: &Arc<Self>, section: &str, item: &str, index: usize) -> Result<()> {
        let core = self.clone();
        if section == "favourites" {
            let id: i64 = item.parse().map_err(|_| Error::kind(ErrorKind::NotFound))?;
            return blocking(move || core.favourites.move_to(id, index)).await;
        }
        let mut items: Vec<String> = self
            .browse_layout_rows()
            .await
            .into_iter()
            .filter(|r| r.section == section && r.kind != SECTION_KIND)
            .map(|r| r.item)
            .collect();
        let from = items
            .iter()
            .position(|i| i == item)
            .ok_or_else(|| Error::kind(ErrorKind::NotFound))?;
        let moving = items.remove(from);
        items.insert(index.min(items.len()), moving);
        let section = section.to_owned();
        blocking(move || core.browse_layout.set_order(&section, &items)).await
    }

    /// Rescans local locations first: inotify can miss mounts inside the sandbox's mount namespace.
    async fn arranged_rows(self: &Arc<Self>) -> (Vec<Row>, Layout) {
        let core = self.clone();
        let blocking = tokio::task::spawn_blocking(move || {
            core.locations.refresh();
            core.local_rows()
        })
        .await
        .unwrap_or_default();
        let mut rows = blocking.favourites;
        rows.extend(blocking.device);
        rows.extend(blocking.android);
        rows.extend(self.volume_rows().await);
        rows.extend(self.server_rows(&blocking.prefs).await);
        rows.extend(self.nearby_rows());
        let layout = blocking.layout;
        let rows = layout.arrange(rows, |r| (r.section.to_owned(), layout_item(r.kind, &r.item_id)));
        (rows, layout)
    }

    fn local_rows(&self) -> LocalRows {
        let prefs: BTreeMap<String, LocationPrefs> = self
            .location_prefs
            .list()
            .unwrap_or_default()
            .into_iter()
            .map(|p| (p.location_id.clone(), p))
            .collect();
        let settings = self.settings();
        let mut device = Vec::new();
        if settings.recents_enabled {
            let mut row = Row::new("device", "recents");
            row.icon = "image://theme/icon-m-history".to_owned();
            device.push(row);
        }
        device.extend(self.folder_rows("device", &LocationKind::UserFolder, &prefs));
        if settings.recently_deleted {
            let mut row = Row::new("device", "deleted");
            row.icon = "image://theme/icon-m-delete".to_owned();
            device.push(row);
        }
        LocalRows {
            favourites: self.favourite_rows(),
            device,
            android: self.folder_rows("android", &LocationKind::Android, &prefs),
            prefs,
            layout: self.browse_layout.load().unwrap_or_default(),
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
                row.icon = "file-folder".to_owned();
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
            row.place = self.favourite_place(&f);
            row.icon = f.icon_source();
            row.is_file = !f.is_dir;
            row.item_id = f.id.to_string();
            row.name = f.label;
            row.colour = f.colour.unwrap_or_default();
            rows.push(row);
        }
        rows
    }

    async fn volume_rows(&self) -> Vec<Row> {
        let drives = self.bridge.as_ref().map(|b| b.drives()).unwrap_or_default();
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
            row.icon = drive_icon(DriveKind::Card).to_owned();
            if let Some(d) = drives
                .iter()
                .find(|d| d.mount_point.is_some() && d.mount_point == l.local_root)
            {
                row.drive = d.id.clone();
                row.icon = drive_icon(d.kind).to_owned();
            }
            if let Ok(info) = self.volume_info(&uri).await {
                row.free = clamp_i64(info.free);
                row.total = clamp_i64(info.total);
                row.fs = info.fs;
            }
            rows.push(row);
        }
        rows.extend(
            drives
                .iter()
                .filter(|d| d.mount_point.is_none())
                .map(unmounted_row),
        );
        rows.sort_by(|a, b| a.name.cmp(&b.name));
        rows
    }

    pub async fn mount_drive(self: &Arc<Self>, drive: &str) -> Result<Uri> {
        let point = self.drive_client()?.mount_drive(drive).await?;
        for look in 0..MOUNT_LOOKS {
            if look > 0 {
                tokio::time::sleep(MOUNT_LOOK_EVERY).await;
            }
            let (core, point) = (self.clone(), point.clone());
            if let Some(uri) = blocking(move || Ok(core.volume_at(&point))).await? {
                return Ok(uri);
            }
        }
        Err(Error::new(
            ErrorKind::NotFound,
            format!("{} is mounted but does not show", point.display()),
        ))
    }

    pub async fn unmount_drive(self: &Arc<Self>, drive: &str) -> Result<()> {
        self.drive_client()?.unmount_drive(drive).await?;
        let core = self.clone();
        blocking(move || Ok(core.locations.refresh_volumes())).await?;
        Ok(())
    }

    fn drive_client(&self) -> Result<&crate::bridge::BridgeClient> {
        self.bridge
            .as_ref()
            .ok_or_else(|| Error::new(ErrorKind::BridgeUnavailable, "drives are mounted by netvfs"))
    }

    fn volume_at(&self, point: &Path) -> Option<Uri> {
        self.locations.refresh_volumes();
        self.locations
            .locations()
            .into_iter()
            .find(|l| l.kind == LocationKind::Volume && l.local_root.as_deref() == Some(point))
            .map(|l| Uri::root(l.id))
    }

    async fn server_rows(&self, prefs: &BTreeMap<String, LocationPrefs>) -> Vec<Row> {
        let Some(client) = &self.bridge else {
            return Vec::new();
        };
        match client.status() {
            BridgeStatus::ConsentUnknown => {
                let mut row = Row::new("servers", "consent");
                row.icon = SERVER_ICON.to_owned();
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
                row.connection = l.connection.name();
                row.connection_error = l.connection.error().to_owned();
                row.icon = SERVER_ICON.to_owned();
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
                row.connection = ConnectionState::Disconnected.name();
                row.icon = SERVER_ICON.to_owned();
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
        // One row per address: a new connection replaces it.
        conn.execute("DELETE FROM adhoc_servers WHERE url = ?1", [url])?;
        conn.execute(
            "INSERT OR REPLACE INTO adhoc_servers(location_id, url, name, last_used_ms) VALUES (?1, ?2, ?3, ?4)",
            params![location_id, url, name, now_ms()],
        )?;
        Ok(())
    }

    pub fn remove_adhoc_recent(&self, location_id: &str) -> Result<()> {
        self.db
            .lock()
            .execute("DELETE FROM adhoc_servers WHERE location_id = ?1", [location_id])?;
        self.purge_location_data(location_id)
    }

    /// Secret moves into a wiping buffer at once, wiped when the call ends whatever the outcome.
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

    pub async fn forget_adhoc(&self, location_id: &str) -> Result<()> {
        if let Some(client) = &self.bridge {
            let _ = client.forget_adhoc(location_id).await;
        }
        self.remove_adhoc_recent(location_id)
    }

    pub fn purge_location_data(&self, location: &str) -> Result<()> {
        org::purge_location(&self.db, location)?;
        self.location_prefs.remove(location)?;
        self.browse_layout.forget(location)?;
        self.dircache.invalidate_location(location)?;
        Ok(())
    }

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
            "SELECT DISTINCT item FROM browse_layout",
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

    /// Empty list from withdrawn consent or absent bridge never purges. Needs tokio runtime.
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
        // Transfers record the location, not the folder.
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
            date_group: DateGroup::at(r.at_ms, now),
        }
    }
}

#[derive(Default)]
struct LocalRows {
    favourites: Vec<Row>,
    device: Vec<Row>,
    android: Vec<Row>,
    prefs: BTreeMap<String, LocationPrefs>,
    layout: Layout,
}

fn drive_icon(kind: DriveKind) -> &'static str {
    match kind {
        DriveKind::Card => "image://theme/icon-m-sd-card",
        DriveKind::Usb => "image://theme/icon-m-usb",
    }
}

/// Locked drive is named by LUKS label or UUID: filesystem unknown before Settings › Storage
/// unlocks it.
fn unmounted_row(d: &Drive) -> Row {
    let name = d.mount_name();
    let mut row = Row::new("volumes", "volume");
    row.status = if d.locked { "locked" } else { "unmounted" };
    row.item_id = crate::volumes::location_id_of(OsStr::new(&name));
    row.name = name;
    row.icon = drive_icon(d.kind).to_owned();
    row.total = d.size;
    row.drive = d.id.clone();
    row
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
        assert_eq!(provider_label("smb"), "SMB");
        assert_eq!(provider_label("weird"), "WEIRD");
    }

    #[test]
    fn nearby_urls_skip_default_ports() {
        let mut n = NearbyServer {
            name: "Mac".into(),
            provider: "smb".into(),
            host: "mac-mini.local".into(),
            port: 445,
        };
        assert_eq!(nearby_url(&n), "smb://mac-mini.local/");
        n.port = 4445;
        assert_eq!(nearby_url(&n), "smb://mac-mini.local:4445/");
        n.port = 0;
        assert_eq!(nearby_url(&n), "smb://mac-mini.local/");
        n.provider = "sftp".into();
        n.port = 22;
        assert_eq!(nearby_url(&n), "sftp://mac-mini.local/");
        n.port = 2222;
        assert_eq!(nearby_url(&n), "sftp://mac-mini.local:2222/");
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
    fn locations_in_stored_uris() {
        assert_eq!(location_of("lautta://nv-account:1/a/b"), Some("nv-account:1"));
        assert_eq!(location_of("lautta://user-documents/"), Some("user-documents"));
        assert_eq!(location_of("file:///x"), None);
    }

    #[test]
    fn row_keys_tell_rows_apart() {
        let mut a = Row::new("favourites", "favourite");
        a.item_id = "1".into();
        let mut b = a.clone();
        assert_eq!(a.key(), b.key());
        b.item_id = "2".into();
        assert_ne!(a.key(), b.key());
    }
}
