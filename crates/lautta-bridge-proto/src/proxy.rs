// SPDX-License-Identifier: LGPL-2.1-or-later
//! zbus proxy for `org.netvfs.Bridge1` at `/org/netvfs/Bridge` (XB-8..XB-10).
//! The interface text is `tests/bridge-contract/org.netvfs.Bridge1.xml`.

use crate::wire::{Opts, WireCapabilities, WireEntry, WireLocation, WireNearby, WireWalkItem};
use zbus::zvariant::{Fd, OwnedValue};

#[zbus::proxy(
    interface = "org.netvfs.Bridge1",
    default_path = "/org/netvfs/Bridge",
    // A peer-to-peer connection has no bus to route by name; zbus still wants a
    // destination and the bridge ignores it.
    default_service = "org.netvfs.Bridge",
    gen_blocking = false
)]
pub trait Bridge1 {
    // Session
    fn hello(&self, protocol: u32, client: &str) -> zbus::Result<(u32, String, Vec<String>)>;
    fn get_consent(&self) -> zbus::Result<String>;
    fn request_consent(&self) -> zbus::Result<()>;
    #[zbus(signal)]
    fn consent_changed(&self, consent: String) -> zbus::Result<()>;

    // Locations
    fn list_locations(&self) -> zbus::Result<Vec<WireLocation>>;
    #[zbus(signal)]
    fn locations_changed(&self) -> zbus::Result<()>;
    fn capabilities(&self, loc: &str) -> zbus::Result<WireCapabilities>;
    fn disconnect(&self, loc: &str) -> zbus::Result<()>;

    // Ad-hoc locations
    fn connect_ad_hoc(&self, url: &str, secret: &[u8], opts: &Opts) -> zbus::Result<String>;
    fn forget_ad_hoc(&self, loc: &str) -> zbus::Result<()>;

    // Discovery
    fn discover(&self, on: bool) -> zbus::Result<()>;
    #[zbus(signal)]
    fn nearby_changed(&self, nearby: Vec<WireNearby>) -> zbus::Result<()>;

    // Listing
    fn list(&self, loc: &str, dir: &[u8], lane: &str, batch: u32) -> zbus::Result<u32>;
    #[zbus(signal)]
    fn list_batch(&self, req: u32, entries: Vec<WireEntry>) -> zbus::Result<()>;
    #[zbus(signal)]
    fn list_done(&self, req: u32, error: String, message: String) -> zbus::Result<()>;

    // Metadata
    fn stat(&self, loc: &str, path: &[u8], follow: bool, lane: &str) -> zbus::Result<WireEntry>;
    fn read_link(&self, loc: &str, path: &[u8]) -> zbus::Result<Vec<u8>>;
    fn space_info(&self, loc: &str, path: &[u8]) -> zbus::Result<(i64, i64, i64)>;
    fn checksum(&self, loc: &str, path: &[u8], algo: &str) -> zbus::Result<Vec<u8>>;

    // Namespace
    fn make_dir(&self, loc: &str, path: &[u8], exclusive: bool) -> zbus::Result<()>;
    fn remove_file(&self, loc: &str, path: &[u8]) -> zbus::Result<()>;
    fn remove_dir(&self, loc: &str, path: &[u8]) -> zbus::Result<()>;
    fn rename(&self, loc: &str, from: &[u8], to: &[u8], replace: bool) -> zbus::Result<()>;
    fn set_attributes(&self, loc: &str, path: &[u8], changes: &Opts) -> zbus::Result<()>;
    fn make_symlink(&self, loc: &str, target: &[u8], link_path: &[u8]) -> zbus::Result<()>;
    fn make_hardlink(&self, loc: &str, existing: &[u8], new_path: &[u8]) -> zbus::Result<()>;
    fn server_copy(&self, loc: &str, from: &[u8], to: &[u8], opts: &Opts) -> zbus::Result<()>;

    // Handles
    fn open_read(&self, loc: &str, path: &[u8], lane: &str) -> zbus::Result<(u32, i64)>;
    fn read(&self, handle: u32, offset: i64, max: u32) -> zbus::Result<Vec<u8>>;
    fn read_ahead(&self, handle: u32, offset: i64, bytes: i64) -> zbus::Result<()>;
    fn close(&self, handle: u32) -> zbus::Result<()>;

    // Jobs
    fn upload(&self, loc: &str, path: &[u8], fd: Fd<'_>, opts: &Opts) -> zbus::Result<u32>;
    fn download(&self, loc: &str, path: &[u8], fd: Fd<'_>, opts: &Opts) -> zbus::Result<u32>;
    fn copy_across(
        &self,
        src_loc: &str,
        src: &[u8],
        dst_loc: &str,
        dst: &[u8],
        opts: &Opts,
    ) -> zbus::Result<u32>;
    fn remove_tree(&self, loc: &str, path: &[u8]) -> zbus::Result<u32>;
    fn walk(&self, loc: &str, root: &[u8], opts: &Opts) -> zbus::Result<u32>;
    fn cancel(&self, id: u32) -> zbus::Result<()>;
    #[zbus(signal)]
    fn job_progress(&self, job: u32, done: i64, total: i64) -> zbus::Result<()>;
    #[zbus(signal)]
    fn walk_batch(&self, job: u32, entries: Vec<WireWalkItem>) -> zbus::Result<()>;
    #[zbus(signal)]
    fn job_finished(
        &self,
        job: u32,
        error: String,
        message: String,
        extra: std::collections::HashMap<String, OwnedValue>,
    ) -> zbus::Result<()>;

    // Questions
    #[zbus(signal)]
    fn question(
        &self,
        id: String,
        kind: String,
        details: std::collections::HashMap<String, OwnedValue>,
    ) -> zbus::Result<()>;
    fn answer(&self, id: &str, answer: &Opts) -> zbus::Result<()>;

    // Handoff
    fn open_account_settings(&self, loc: &str) -> zbus::Result<()>;
    fn add_account(&self, provider: &str) -> zbus::Result<()>;
}
