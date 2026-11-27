// SPDX-License-Identifier: LGPL-2.1-or-later

use crate::wire::{
    Bytes, Opts, WireCapabilities, WireCheckItem, WireCheckResult, WireDrive, WireEntry, WireLocation,
    WireNearby, WireWalkItem,
};
use zbus::zvariant::{Fd, OwnedValue};

#[zbus::proxy(
    interface = "org.netvfs.Bridge1",
    default_path = "/org/netvfs/Bridge",
    default_service = "org.netvfs.Bridge",
    gen_blocking = false
)]
pub trait Bridge1 {
    fn hello(&self, client: &str) -> zbus::Result<String>;
    fn get_consent(&self) -> zbus::Result<String>;
    fn request_consent(&self) -> zbus::Result<()>;
    #[zbus(signal)]
    fn consent_changed(&self, consent: String) -> zbus::Result<()>;
    fn get_features(&self) -> zbus::Result<Vec<String>>;

    fn list_locations(&self) -> zbus::Result<Vec<WireLocation>>;
    #[zbus(signal)]
    fn locations_changed(&self) -> zbus::Result<()>;
    fn capabilities(&self, loc: &str) -> zbus::Result<WireCapabilities>;
    fn disconnect(&self, loc: &str) -> zbus::Result<()>;

    fn discover(&self, on: bool) -> zbus::Result<()>;
    #[zbus(signal)]
    fn nearby_changed(&self, nearby: Vec<WireNearby>) -> zbus::Result<()>;

    fn list(&self, loc: &str, dir: &[u8], lane: &str, batch: u32) -> zbus::Result<u32>;
    #[zbus(signal)]
    fn list_batch(&self, req: u32, entries: Vec<WireEntry>) -> zbus::Result<()>;
    #[zbus(signal)]
    fn list_done(&self, req: u32, error: String, message: String) -> zbus::Result<()>;

    fn stat(&self, loc: &str, path: &[u8], follow: bool, lane: &str) -> zbus::Result<WireEntry>;
    fn read_link(&self, loc: &str, path: &[u8]) -> zbus::Result<Vec<u8>>;
    fn space_info(&self, loc: &str, path: &[u8]) -> zbus::Result<(i64, i64, i64)>;
    fn checksum(&self, loc: &str, path: &[u8], algo: &str) -> zbus::Result<Vec<u8>>;

    fn make_dir(&self, loc: &str, path: &[u8], exclusive: bool) -> zbus::Result<()>;
    fn remove_file(&self, loc: &str, path: &[u8]) -> zbus::Result<()>;
    fn remove_dir(&self, loc: &str, path: &[u8]) -> zbus::Result<()>;
    fn rename(&self, loc: &str, from: &[u8], to: &[u8], replace: bool) -> zbus::Result<()>;
    fn set_attributes(&self, loc: &str, path: &[u8], changes: &Opts) -> zbus::Result<()>;
    fn make_symlink(&self, loc: &str, target: &[u8], link_path: &[u8]) -> zbus::Result<()>;
    fn make_hardlink(&self, loc: &str, existing: &[u8], new_path: &[u8]) -> zbus::Result<()>;
    fn server_copy(&self, loc: &str, from: &[u8], to: &[u8], opts: &Opts) -> zbus::Result<()>;

    fn open_read(&self, loc: &str, path: &[u8], lane: &str) -> zbus::Result<(u32, i64)>;
    fn read(&self, handle: u32, offset: i64, max: u32) -> zbus::Result<Bytes>;
    fn read_ahead(&self, handle: u32, offset: i64, bytes: i64) -> zbus::Result<()>;
    fn close(&self, handle: u32) -> zbus::Result<()>;

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

    fn open_account_settings(&self, loc: &str) -> zbus::Result<()>;
    fn add_account(&self, provider: &str) -> zbus::Result<()>;
    fn open_storage_settings(&self) -> zbus::Result<()>;

    fn list_drives(&self) -> zbus::Result<Vec<WireDrive>>;
    #[zbus(signal)]
    fn drives_changed(&self) -> zbus::Result<()>;
    fn mount(&self, drive: &str) -> zbus::Result<Vec<u8>>;
    fn unmount(&self, drive: &str) -> zbus::Result<()>;

    fn check(&self, loc: &str, items: &[WireCheckItem], lane: &str) -> zbus::Result<u32>;
    #[zbus(signal)]
    fn check_batch(&self, job: u32, results: Vec<WireCheckResult>) -> zbus::Result<()>;
    fn get_network(&self) -> zbus::Result<String>;
    #[zbus(signal)]
    fn network_changed(&self, network: String) -> zbus::Result<()>;
    fn search(&self, loc: &str, root: &[u8], text: &str, opts: &Opts) -> zbus::Result<u32>;
}
