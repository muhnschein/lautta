// SPDX-License-Identifier: LGPL-2.1-or-later

use super::args::{self, Dict};
use super::fail::Fail;
use super::state::{CancelOutcome, Consent, FakeDrive, Session, Shared, State};
use super::transfer::{self, Env};
use super::tree::Tree;
use super::{tree::TreeError, DEFAULT_CAPS};
use crate::access::{self, After, Level, Op, Verdict};
use crate::wire::{
    feature, location_state, WireCapabilities, WireCheckItem, WireCheckResult, WireDrive, WireEntry,
    WireLocation, WireNearby, WireWalkItem, INTERFACE, OBJECT_PATH,
};
use sha2::Digest;
use std::collections::HashMap;
use std::sync::{Arc, OnceLock};
use zbus::fdo;
use zbus::names::{InterfaceName, MemberName};
use zbus::object_server::{DispatchResult, Interface, SignalContext};
use zbus::zvariant::{OwnedFd, OwnedValue};
use zbus::ObjectServer;

const MAX_HANDLES: usize = 16;

const CONTRACT_XML: &str = include_str!("../../../../tests/bridge-contract/org.netvfs.Bridge1.xml");

pub(super) fn contract_methods() -> &'static HashMap<String, String> {
    static TABLE: OnceLock<HashMap<String, String>> = OnceLock::new();
    TABLE.get_or_init(|| parse_methods(CONTRACT_XML))
}

fn attr<'a>(line: &'a str, name: &str) -> Option<&'a str> {
    let marker = format!("{name}=\"");
    let start = line.find(&marker)? + marker.len();
    let len = line[start..].find('"')?;
    Some(&line[start..start + len])
}

fn parse_methods(xml: &str) -> HashMap<String, String> {
    let mut table = HashMap::new();
    let mut current: Option<String> = None;
    let mut in_bridge = false;
    for line in xml.lines().map(str::trim) {
        if line.starts_with("<interface") {
            in_bridge = attr(line, "name") == Some(INTERFACE);
        } else if in_bridge && line.starts_with("<method") {
            if let Some(name) = attr(line, "name") {
                table.insert(name.to_owned(), String::new());
                current = (!line.ends_with("/>")).then(|| name.to_owned());
            }
        } else if line.starts_with("</method") {
            current = None;
        } else if line.starts_with("<arg") && attr(line, "direction") == Some("in") {
            if let (Some(m), Some(t)) = (&current, attr(line, "type")) {
                if let Some(sig) = table.get_mut(m) {
                    sig.push_str(t);
                }
            }
        }
    }
    table
}

pub(super) struct Checked(Iface);

impl Checked {
    pub fn new(shared: Arc<Shared>, session: Arc<Session>) -> Checked {
        Checked(Iface::new(shared, session))
    }
}

#[async_trait::async_trait]
impl Interface for Checked {
    fn name() -> InterfaceName<'static> {
        Iface::name()
    }

    async fn get(&self, property_name: &str) -> Option<fdo::Result<OwnedValue>> {
        self.0.get(property_name).await
    }

    async fn get_all(&self) -> fdo::Result<HashMap<String, OwnedValue>> {
        self.0.get_all().await
    }

    async fn set_mut(
        &mut self,
        property_name: &str,
        value: &zbus::zvariant::Value<'_>,
        ctxt: &SignalContext<'_>,
    ) -> Option<fdo::Result<()>> {
        self.0.set_mut(property_name, value, ctxt).await
    }

    fn call<'call>(
        &'call self,
        server: &'call ObjectServer,
        connection: &'call zbus::Connection,
        msg: &'call zbus::Message,
        name: MemberName<'call>,
    ) -> DispatchResult<'call> {
        self.0.session.attach(connection);
        let wanted = contract_methods().get(name.as_str());
        let body = msg.body();
        let given = body.signature();
        let given = given.as_ref().map_or("", |s| s.as_str());
        if wanted.is_some_and(|w| w != given) {
            let message = format!("{} takes ({})", name.as_str(), wanted.map_or("", String::as_str));
            return DispatchResult::new_async(connection, msg, async move {
                Err::<(), Fail>(Fail::args(&message))
            });
        }
        self.0.call(server, connection, msg, name)
    }

    fn call_mut<'call>(
        &'call mut self,
        server: &'call ObjectServer,
        connection: &'call zbus::Connection,
        msg: &'call zbus::Message,
        name: MemberName<'call>,
    ) -> DispatchResult<'call> {
        self.0.call_mut(server, connection, msg, name)
    }

    fn introspect_to_writer(&self, writer: &mut dyn std::fmt::Write, level: usize) {
        self.0.introspect_to_writer(writer, level);
    }
}

pub(super) struct Iface {
    shared: Arc<Shared>,
    session: Arc<Session>,
}

impl Iface {
    pub fn new(shared: Arc<Shared>, session: Arc<Session>) -> Iface {
        Iface { shared, session }
    }

    async fn begin(&self, method: &str, lane: &str, target: &str, needs_consent: bool) -> Result<(), Fail> {
        self.shared.record_call(method, lane, target);
        if method != "Hello" && !self.session.with(|s| s.hello) {
            return Err(Fail::net("ProtocolError", "Hello must be the first call"));
        }
        if needs_consent && self.shared.with(|s| s.consent) == Consent::Unknown {
            return Err(Fail::net(
                "PermissionDenied",
                "The user has not decided yet what this app may do",
            ));
        }
        let (latency, failure) = self.shared.with(|s| (s.latency, s.due_failure(method)));
        if !latency.is_zero() {
            tokio::time::sleep(latency).await;
        }
        match failure {
            Some(f) => Err(Fail::net(&f.name, &f.message).with_detail(f.detail, f.retry_after_ms)),
            None => Ok(()),
        }
    }

    fn require_drives(&self, method: &str) -> Result<(), Fail> {
        if self.shared.with(|s| s.offers_drives()) {
            Ok(())
        } else {
            self.shared.record_call(method, "interactive", "");
            Err(Fail::unknown_method(method))
        }
    }

    async fn mount_or_unmount(&self, method: &str, drive: &str, mount: bool) -> Result<Vec<u8>, Fail> {
        self.require_drives(method)?;
        let drive = args::drive(drive)?;
        self.begin(method, "interactive", &drive, false).await?;
        if mount
            && self
                .shared
                .with(|s| s.drives.iter().any(|d| d.id == drive && d.locked))
        {
            return Err(Fail::net("Unsupported", "The drive is locked"));
        }
        let Some((before, after)) = self.shared.with(|s| s.mount(&drive, mount)) else {
            return Err(Fail::net("NotFound", "No such drive"));
        };
        if before != after {
            if self.shared.with(|s| s.mount_root.is_some()) {
                mount_folder(&before, &after).map_err(|e| Fail::net("Internal", &e.to_string()))?;
            }
            self.shared.broadcast_drives().await;
        }
        Ok(after)
    }

    fn require_location(&self, loc: &str) -> Result<Level, Fail> {
        self.shared.with(|s| {
            if !s.locations.contains_key(loc) {
                return Err(Fail::net("NotFound", "No such location"));
            }
            s.level(loc).ok_or_else(|| {
                Fail::net(
                    "PermissionDenied",
                    "The user has not allowed this app to use this account",
                )
                .with_detail(Some("denied".to_owned()), None)
            })
        })
    }

    fn admit(&self, loc: &str, path: &[u8], op: Op) -> Result<Verdict, Fail> {
        let level = self.require_location(loc)?;
        let own = self.shared.with(|s| s.own(loc, path));
        match access::check(level, op, own) {
            Verdict::Deny => Err(refused(level)),
            verdict => Ok(verdict),
        }
    }

    fn after<T>(&self, loc: &str, path: &[u8], call: After, result: &Result<T, Fail>) {
        self.shared
            .with(|s| s.after_call(loc, path, call, result.as_ref().err().map(Fail::bare)));
    }

    fn remove(
        &self,
        loc: &str,
        path: &[u8],
        f: impl FnOnce(&mut Tree) -> Result<(), TreeError>,
    ) -> Result<(), Fail> {
        if self.admit(loc, path, Op::Remove)? == Verdict::DenyUnlessMissing {
            self.tree(loc, |t| t.stat(path, false))?;
            return Err(refused(self.require_location(loc)?));
        }
        let result = self.tree(loc, f);
        self.after(loc, path, After::Gone, &result);
        result
    }

    fn reach(&self, loc: &str) -> Result<(), Fail> {
        self.require_location(loc)?;
        self.set_state(loc, location_state::CONNECTED);
        Ok(())
    }

    fn set_state(&self, loc: &str, state: &str) {
        if self.shared.with(|s| s.set_state(loc, state, None)) {
            let shared = Arc::clone(&self.shared);
            tokio::spawn(async move { shared.broadcast_locations().await });
        }
    }

    fn tree<R>(&self, loc: &str, f: impl FnOnce(&mut Tree) -> Result<R, TreeError>) -> Result<R, Fail> {
        self.reach(loc)?;
        self.shared.with(|s| f(s.tree_mut(loc))).map_err(Fail::from)
    }

    fn env(&self, ctxt: &SignalContext<'_>) -> Env {
        if self.session.conn().is_none() {
            self.session.set_conn(ctxt.connection().clone());
        }
        Env {
            shared: self.shared.clone(),
            session: self.session.clone(),
            ctx: ctxt.to_owned(),
        }
    }

    fn ask_consent(&self) {
        let auto = self.shared.with(|s| {
            s.consent_requests += 1;
            s.auto_consent
        });
        if let Some(answer) = auto {
            let shared = self.shared.clone();
            tokio::spawn(async move { shared.decide(|s| s.consent = answer).await });
        }
    }

    fn start_job<F, Fut>(&self, ctxt: &SignalContext<'_>, work: F) -> u32
    where
        F: FnOnce(Env, u32, Arc<std::sync::atomic::AtomicBool>) -> Fut,
        Fut: std::future::Future<Output = ()> + Send + 'static,
    {
        let job = self.session.next_id();
        let cancel = self.session.register_job(job);
        let env = self.env(ctxt);
        tokio::spawn(work(env, job, cancel));
        job
    }
}

fn refused(level: Level) -> Fail {
    let what = match level {
        Level::Full => "do this",
        Level::ReadAdd => "read and add",
        Level::Read => "read",
        Level::Add => "add",
    };
    Fail::net(
        "PermissionDenied",
        &format!("This app may only {what} in this account"),
    )
    .with_detail(Some(level.as_str().to_owned()), None)
}

fn copy_op(replace: bool) -> Op {
    if replace {
        Op::Unrestricted
    } else {
        Op::Create
    }
}

fn mount_folder(before: &[u8], after: &[u8]) -> std::io::Result<()> {
    use std::ffi::OsStr;
    use std::os::unix::ffi::OsStrExt;
    if after.is_empty() {
        std::fs::remove_dir(OsStr::from_bytes(before))
    } else {
        std::fs::create_dir_all(OsStr::from_bytes(after))
    }
}

#[zbus::interface(name = "org.netvfs.Bridge1")]
impl Iface {
    async fn hello(&self, client: String) -> Result<String, Fail> {
        if client.len() > 256 {
            return Err(Fail::args("Argument 1 is too long"));
        }
        self.begin("Hello", "interactive", &client, false).await?;
        self.session.with(|s| s.hello = true);
        if self.shared.with(|s| s.consent) == Consent::Unknown {
            self.ask_consent();
        }
        Ok(self.shared.with(|s| s.bridge_version.clone()))
    }

    async fn get_consent(&self) -> Result<String, Fail> {
        self.begin("GetConsent", "interactive", "", false).await?;
        Ok(self.shared.with(|s| s.consent).wire().to_owned())
    }

    async fn request_consent(&self) -> Result<(), Fail> {
        self.begin("RequestConsent", "interactive", "", false).await?;
        if self.shared.with(|s| s.consent) == Consent::Unknown {
            self.ask_consent();
        }
        Ok(())
    }

    async fn get_features(&self) -> Result<Vec<String>, Fail> {
        let Some(features) = self.shared.with(|s| s.features.clone()) else {
            return Err(Fail::unknown_method("GetFeatures"));
        };
        self.begin("GetFeatures", "interactive", "", false).await?;
        Ok(features)
    }

    async fn list_locations(&self) -> Result<Vec<WireLocation>, Fail> {
        let list = self.shared.with(|s| s.wire_locations());
        self.begin("ListLocations", "interactive", "", false).await?;
        self.shared.with(|s| s.location_replies += 1);
        Ok(list)
    }

    async fn capabilities(&self, loc: String) -> Result<(WireCapabilities,), Fail> {
        let loc = args::location(&loc)?;
        self.begin("Capabilities", "interactive", &loc, true).await?;
        self.reach(&loc)?;
        Ok((self.shared.with(|s| {
            s.caps.get(&loc).cloned().unwrap_or_else(|| WireCapabilities {
                flags: DEFAULT_CAPS.iter().map(|f| (*f).to_owned()).collect(),
                checksum_algorithms: vec!["sha256".to_owned()],
                max_name_bytes: 255,
            })
        }),))
    }

    async fn disconnect(&self, loc: String) -> Result<(), Fail> {
        let loc = args::location(&loc)?;
        self.begin("Disconnect", "interactive", &loc, true).await?;
        self.require_location(&loc)?;
        self.set_state(&loc, location_state::DISCONNECTED);
        Ok(())
    }

    async fn discover(&self, on: bool, #[zbus(signal_context)] ctxt: SignalContext<'_>) -> Result<(), Fail> {
        self.begin("Discover", "interactive", "", true).await?;
        self.env(&ctxt);
        self.session.with(|s| s.discovering = on);
        if on {
            let shared = self.shared.clone();
            tokio::spawn(async move { shared.broadcast_nearby().await });
        } else if !self.shared.any_discovering() {
            self.shared.with(|s| s.nearby.clear());
        }
        Ok(())
    }

    async fn list(
        &self,
        loc: String,
        dir: Vec<u8>,
        lane: String,
        batch: u32,
        #[zbus(signal_context)] ctxt: SignalContext<'_>,
    ) -> Result<u32, Fail> {
        let loc = args::location(&loc)?;
        let dir = args::path(&dir)?;
        let lane = args::lane(&lane, "interactive")?;
        let batch = args::list_batch(batch)?;
        self.begin("List", lane, &loc, true).await?;
        let folders_only = self.admit(&loc, &dir, Op::Browse)? == Verdict::FoldersOnly;
        self.reach(&loc)?;
        let listing = self.tree(&loc, |t| t.list(&dir)).map(|mut entries| {
            entries.retain(|e| !folders_only || access::is_folder(e.kind, e.target_kind));
            entries
        });
        Ok(self.start_job(&ctxt, move |env, req, cancel| {
            transfer::run_list(env, req, listing, batch, cancel)
        }))
    }

    async fn stat(
        &self,
        loc: String,
        path: Vec<u8>,
        follow: bool,
        lane: String,
    ) -> Result<(WireEntry,), Fail> {
        let loc = args::location(&loc)?;
        let path = args::path(&path)?;
        let lane = args::lane(&lane, "interactive")?;
        self.begin("Stat", lane, &loc, true).await?;
        let verdict = self.admit(&loc, &path, Op::Look)?;
        let entry = self.tree(&loc, |t| t.stat(&path, follow))?;
        if verdict == Verdict::IfFolder && !access::is_folder(entry.kind, entry.target_kind) {
            return Err(refused(self.require_location(&loc)?));
        }
        Ok((entry,))
    }

    async fn read_link(&self, loc: String, path: Vec<u8>) -> Result<Vec<u8>, Fail> {
        let loc = args::location(&loc)?;
        let path = args::path(&path)?;
        self.begin("ReadLink", "interactive", &loc, true).await?;
        self.admit(&loc, &path, Op::ReadLink)?;
        self.tree(&loc, |t| t.read_link(&path))
    }

    async fn space_info(&self, loc: String, path: Vec<u8>) -> Result<(i64, i64, i64), Fail> {
        let loc = args::location(&loc)?;
        args::path(&path)?;
        self.begin("SpaceInfo", "interactive", &loc, true).await?;
        let used = self.tree(&loc, |t| Ok(t.used_bytes()))?;
        let total = self
            .shared
            .with(|s| s.space_total.get(&loc).copied())
            .unwrap_or(1_000_000)
            .max(used);
        Ok((total - used, total, used))
    }

    async fn checksum(&self, loc: String, path: Vec<u8>, algo: String) -> Result<Vec<u8>, Fail> {
        let loc = args::location(&loc)?;
        let path = args::path(&path)?;
        let algo = args::token(&algo, 32)?;
        self.begin("Checksum", "bulk", &loc, true).await?;
        self.admit(&loc, &path, Op::Read)?;
        if algo != "sha256" {
            return Err(Fail::net("Unsupported", "Unknown checksum algorithm"));
        }
        self.tree(&loc, |t| {
            t.file_data(&path).map(|d| sha2::Sha256::digest(d).to_vec())
        })
    }

    async fn make_dir(&self, loc: String, path: Vec<u8>, exclusive: bool) -> Result<(), Fail> {
        let loc = args::location(&loc)?;
        let path = args::path(&path)?;
        self.begin("MakeDir", "interactive", &loc, true).await?;
        self.admit(&loc, &path, Op::MakeDir)?;
        self.tree(&loc, |t| t.make_dir(&path, exclusive))
    }

    async fn remove_file(&self, loc: String, path: Vec<u8>) -> Result<(), Fail> {
        let loc = args::location(&loc)?;
        let path = args::path(&path)?;
        self.begin("RemoveFile", "interactive", &loc, true).await?;
        self.remove(&loc, &path, |t| t.remove_file(&path))
    }

    async fn remove_dir(&self, loc: String, path: Vec<u8>) -> Result<(), Fail> {
        let loc = args::location(&loc)?;
        let path = args::path(&path)?;
        self.begin("RemoveDir", "interactive", &loc, true).await?;
        self.remove(&loc, &path, |t| t.remove_dir(&path))
    }

    async fn rename(&self, loc: String, from: Vec<u8>, to: Vec<u8>, replace: bool) -> Result<(), Fail> {
        let loc = args::location(&loc)?;
        let from = args::path(&from)?;
        let to = args::path(&to)?;
        self.begin("Rename", "interactive", &loc, true).await?;
        self.admit(&loc, &from, if replace { Op::Unrestricted } else { Op::Rename })?;
        let result = self.tree(&loc, |t| t.rename(&from, &to, replace));
        self.after(&loc, &from, After::Gone, &result);
        result
    }

    async fn set_attributes(&self, loc: String, path: Vec<u8>, changes: Dict) -> Result<(), Fail> {
        let loc = args::location(&loc)?;
        let path = args::path(&path)?;
        let changes = args::attributes(&changes)?;
        self.begin("SetAttributes", "interactive", &loc, true).await?;
        self.admit(&loc, &path, Op::SetAttributes)?;
        if changes.is_empty() {
            return Ok(());
        }
        self.tree(&loc, |t| t.set_attributes(&path, changes.mode, changes.mtime_ms))
    }

    async fn make_symlink(&self, loc: String, target: Vec<u8>, link_path: Vec<u8>) -> Result<(), Fail> {
        let loc = args::location(&loc)?;
        if target.is_empty() || target.len() > 4096 || target.contains(&0) {
            return Err(Fail::net("InvalidName", "Invalid link target"));
        }
        let link = args::path(&link_path)?;
        self.begin("MakeSymlink", "interactive", &loc, true).await?;
        self.admit(&loc, &link, Op::Unrestricted)?;
        self.tree(&loc, |t| t.make_symlink(&target, &link))
    }

    async fn make_hardlink(&self, loc: String, existing: Vec<u8>, new_path: Vec<u8>) -> Result<(), Fail> {
        let loc = args::location(&loc)?;
        let existing = args::path(&existing)?;
        let new_path = args::path(&new_path)?;
        self.begin("MakeHardlink", "interactive", &loc, true).await?;
        self.admit(&loc, &new_path, Op::Unrestricted)?;
        self.tree(&loc, |t| t.make_hardlink(&existing, &new_path))
    }

    async fn server_copy(&self, loc: String, from: Vec<u8>, to: Vec<u8>, opts: Dict) -> Result<(), Fail> {
        let loc = args::location(&loc)?;
        let from = args::path(&from)?;
        let to = args::path(&to)?;
        let opts = args::copy_opts(&opts)?;
        self.begin("ServerCopy", "bulk", &loc, true).await?;
        self.admit(&loc, &from, Op::Read)?;
        self.admit(&loc, &to, copy_op(opts.replace))?;
        let result = self.tree(&loc, |t| t.server_copy(&from, &to, opts.recursive, opts.replace));
        if !opts.recursive {
            self.after(&loc, &to, After::Created, &result);
        }
        result
    }

    async fn open_read(&self, loc: String, path: Vec<u8>, lane: String) -> Result<(u32, i64), Fail> {
        let loc = args::location(&loc)?;
        let path = args::path(&path)?;
        let lane = args::lane(&lane, "stream")?;
        self.begin("OpenRead", lane, &loc, true).await?;
        self.admit(&loc, &path, Op::Read)?;
        let offered = self.shared.with(|s| {
            s.caps
                .get(&loc)
                .is_none_or(|c| c.flags.iter().any(|f| f == "ReadHandles"))
        });
        if !offered {
            return Err(Fail::net("Unsupported", "openRead is not supported"));
        }
        let size = self.tree(&loc, |t| {
            t.file_data(&path)
                .map(|d| i64::try_from(d.len()).unwrap_or(i64::MAX))
        })?;
        if self.session.open_handles() >= MAX_HANDLES {
            return Err(
                Fail::net("TooManyConnections", "Too many open handles").with_detail(None, Some(1000))
            );
        }
        Ok((self.session.add_handle(&loc, &path), size))
    }

    async fn read(&self, handle: u32, offset: i64, max: u32) -> Result<Vec<u8>, Fail> {
        let offset = args::offset(offset)?;
        let max = args::read_size(max)?;
        self.begin("Read", "stream", &handle.to_string(), true).await?;
        let (loc, path) = self.handle(handle)?;
        let offset = usize::try_from(offset).unwrap_or(usize::MAX);
        self.tree(&loc, |t| {
            let data = t.file_data(&path)?;
            let start = offset.min(data.len());
            Ok(data[start..data.len().min(start.saturating_add(max))].to_vec())
        })
    }

    async fn read_ahead(&self, handle: u32, offset: i64, bytes: i64) -> Result<(), Fail> {
        args::offset(offset)?;
        args::offset(bytes)?;
        self.begin("ReadAhead", "stream", &handle.to_string(), true)
            .await?;
        self.handle(handle).map(|_| ())
    }

    async fn close(&self, handle: u32) -> Result<(), Fail> {
        self.begin("Close", "stream", &handle.to_string(), true).await?;
        match self.session.with(|s| s.handles.remove(&handle)) {
            Some(_) => Ok(()),
            None => Err(Fail::net("NotFound", "No such handle")),
        }
    }

    async fn upload(
        &self,
        loc: String,
        path: Vec<u8>,
        fd: OwnedFd,
        opts: Dict,
        #[zbus(signal_context)] ctxt: SignalContext<'_>,
    ) -> Result<u32, Fail> {
        let loc = args::location(&loc)?;
        let path = args::path(&path)?;
        let opts = args::transfer_opts(&opts, true)?;
        self.begin("Upload", opts.lane, &loc, true).await?;
        let (op, call) = match opts.disposition {
            args::Disposition::Create => (Op::Create, After::Created),
            args::Disposition::Truncate | args::Disposition::Resume => (Op::Rewrite, After::Rewritten),
        };
        self.admit(&loc, &path, op)?;
        let file = transfer::Source::new(std::fs::File::from(std::os::fd::OwnedFd::from(fd)))?;
        self.reach(&loc)?;
        Ok(self.start_job(&ctxt, move |env, job, cancel| {
            transfer::run_upload(env, job, cancel, (loc, path, call), file, opts)
        }))
    }

    async fn download(
        &self,
        loc: String,
        path: Vec<u8>,
        fd: OwnedFd,
        opts: Dict,
        #[zbus(signal_context)] ctxt: SignalContext<'_>,
    ) -> Result<u32, Fail> {
        let loc = args::location(&loc)?;
        let path = args::path(&path)?;
        let opts = args::transfer_opts(&opts, false)?;
        self.begin("Download", opts.lane, &loc, true).await?;
        self.admit(&loc, &path, Op::Read)?;
        let file = transfer::Source::new(std::fs::File::from(std::os::fd::OwnedFd::from(fd)))?;
        self.reach(&loc)?;
        Ok(self.start_job(&ctxt, move |env, job, cancel| {
            transfer::run_download(env, job, cancel, loc, path, file, opts)
        }))
    }

    async fn copy_across(
        &self,
        src_loc: String,
        src: Vec<u8>,
        dst_loc: String,
        dst: Vec<u8>,
        opts: Dict,
        #[zbus(signal_context)] ctxt: SignalContext<'_>,
    ) -> Result<u32, Fail> {
        let src_loc = args::location(&src_loc)?;
        let src = args::path(&src)?;
        let dst_loc = args::location(&dst_loc)?;
        let dst = args::path(&dst)?;
        let opts = args::copy_opts(&opts)?;
        self.begin("CopyAcross", "bulk", &src_loc, true).await?;
        self.admit(&src_loc, &src, Op::Read)?;
        self.admit(&dst_loc, &dst, copy_op(opts.replace))?;
        Ok(self.start_job(&ctxt, move |env, job, cancel| {
            transfer::run_copy_across(env, job, cancel, (src_loc, src), (dst_loc, dst), opts)
        }))
    }

    async fn remove_tree(
        &self,
        loc: String,
        path: Vec<u8>,
        #[zbus(signal_context)] ctxt: SignalContext<'_>,
    ) -> Result<u32, Fail> {
        let loc = args::location(&loc)?;
        let path = args::path(&path)?;
        self.begin("RemoveTree", "bulk", &loc, true).await?;
        self.admit(&loc, &path, Op::Unrestricted)?;
        self.reach(&loc)?;
        Ok(self.start_job(&ctxt, move |env, job, cancel| {
            transfer::run_remove_tree(env, job, cancel, loc, path)
        }))
    }

    async fn walk(
        &self,
        loc: String,
        root: Vec<u8>,
        opts: Dict,
        #[zbus(signal_context)] ctxt: SignalContext<'_>,
    ) -> Result<u32, Fail> {
        let loc = args::location(&loc)?;
        let root = args::path(&root)?;
        let opts = args::walk_opts(&opts)?;
        self.begin("Walk", "bulk", &loc, true).await?;
        let folders_only = self.admit(&loc, &root, Op::Browse)? == Verdict::FoldersOnly;
        self.reach(&loc)?;
        Ok(self.start_job(&ctxt, move |env, job, cancel| {
            transfer::run_walk(env, job, cancel, (loc, root, folders_only), opts)
        }))
    }

    async fn check(
        &self,
        loc: String,
        items: Vec<WireCheckItem>,
        lane: String,
        #[zbus(signal_context)] ctxt: SignalContext<'_>,
    ) -> Result<u32, Fail> {
        let loc = args::location(&loc)?;
        let items = args::check_items(items)?;
        let lane = args::lane(&lane, "bulk")?;
        self.begin("Check", lane, &loc, true).await?;
        let mut folders_only = Vec::with_capacity(items.len());
        for item in &items {
            folders_only.push(self.admit(&loc, &item.path, Op::Browse)? == Verdict::FoldersOnly);
        }
        self.reach(&loc)?;
        Ok(self.start_job(&ctxt, move |env, job, cancel| {
            transfer::run_check(env, job, cancel, loc, (items, folders_only))
        }))
    }

    async fn search(
        &self,
        loc: String,
        root: Vec<u8>,
        text: String,
        opts: Dict,
        #[zbus(signal_context)] ctxt: SignalContext<'_>,
    ) -> Result<u32, Fail> {
        let loc = args::location(&loc)?;
        let root = args::path(&root)?;
        args::search_text(&text)?;
        let limit = args::search_limit(&opts)?;
        self.begin("Search", "bulk", &loc, true).await?;
        let folders_only = self.admit(&loc, &root, Op::Browse)? == Verdict::FoldersOnly;
        self.reach(&loc)?;
        let offered = self.shared.with(|s| {
            s.caps
                .get(&loc)
                .is_none_or(|c| c.flags.iter().any(|f| f == "Search"))
        });
        if !offered {
            return Err(Fail::net("Unsupported", "Search is not supported"));
        }
        Ok(self.start_job(&ctxt, move |env, job, cancel| {
            transfer::run_search(env, job, cancel, (loc, root, folders_only), (text, limit))
        }))
    }

    async fn get_network(&self) -> Result<String, Fail> {
        self.begin("GetNetwork", "interactive", "", false).await?;
        Ok(self.shared.with(|s| s.network.clone()))
    }

    #[zbus(signal)]
    pub async fn network_changed(ctxt: &SignalContext<'_>, network: String) -> zbus::Result<()>;
    #[zbus(signal)]
    pub async fn check_batch(
        ctxt: &SignalContext<'_>,
        job: u32,
        results: Vec<WireCheckResult>,
    ) -> zbus::Result<()>;

    async fn cancel(&self, id: u32) -> Result<(), Fail> {
        self.begin("Cancel", "interactive", &id.to_string(), false)
            .await?;
        match self.session.cancel(id) {
            CancelOutcome::Unknown => Err(Fail::net("NotFound", "No such request or job")),
            CancelOutcome::Canceled => {
                self.shared.with(|s| s.jobs_canceled += 1);
                Ok(())
            }
            CancelOutcome::Finished => Ok(()),
        }
    }

    async fn open_account_settings(&self, loc: String) -> Result<(), Fail> {
        let loc = args::location(&loc)?;
        self.begin("OpenAccountSettings", "interactive", &loc, true)
            .await?;
        if !loc.starts_with("account:") || !self.shared.with(|s| s.locations.contains_key(&loc)) {
            return Err(Fail::net("NotFound", "No such account"));
        }
        self.shared.with(|s| s.handoffs.push(format!("settings:{loc}")));
        Ok(())
    }

    async fn add_account(&self, provider: String) -> Result<(), Fail> {
        let provider = args::token(&provider, 32)?;
        self.begin("AddAccount", "interactive", &provider, true).await?;
        self.shared.with(|s| s.handoffs.push(format!("add:{provider}")));
        Ok(())
    }

    async fn open_storage_settings(&self) -> Result<(), Fail> {
        let offered = self.shared.with(|s| s.features.clone());
        let Some(offered) = offered else {
            return Err(Fail::unknown_method("OpenStorageSettings"));
        };
        self.begin("OpenStorageSettings", "interactive", "", false)
            .await?;
        if !offered.iter().any(|f| f == feature::STORAGE_SETTINGS) {
            return Err(Fail::net("Unsupported", "Settings > Storage cannot be opened"));
        }
        self.shared.with(|s| s.handoffs.push("storage".to_owned()));
        Ok(())
    }

    async fn list_drives(&self) -> Result<Vec<WireDrive>, Fail> {
        self.require_drives("ListDrives")?;
        self.begin("ListDrives", "interactive", "", false).await?;
        Ok(self
            .shared
            .with(|s| s.drives.iter().map(FakeDrive::wire).collect()))
    }

    async fn mount(&self, drive: String) -> Result<Vec<u8>, Fail> {
        self.mount_or_unmount("Mount", &drive, true).await
    }

    async fn unmount(&self, drive: String) -> Result<(), Fail> {
        self.mount_or_unmount("Unmount", &drive, false).await.map(drop)
    }

    #[zbus(signal)]
    pub async fn drives_changed(ctxt: &SignalContext<'_>) -> zbus::Result<()>;
    #[zbus(signal)]
    pub async fn consent_changed(ctxt: &SignalContext<'_>, consent: String) -> zbus::Result<()>;
    #[zbus(signal)]
    pub async fn locations_changed(ctxt: &SignalContext<'_>) -> zbus::Result<()>;
    #[zbus(signal)]
    pub async fn nearby_changed(ctxt: &SignalContext<'_>, nearby: Vec<WireNearby>) -> zbus::Result<()>;
    #[zbus(signal)]
    pub async fn list_batch(ctxt: &SignalContext<'_>, req: u32, entries: Vec<WireEntry>) -> zbus::Result<()>;
    #[zbus(signal)]
    pub async fn list_done(
        ctxt: &SignalContext<'_>,
        req: u32,
        error: String,
        message: String,
    ) -> zbus::Result<()>;
    #[zbus(signal)]
    pub async fn job_progress(ctxt: &SignalContext<'_>, job: u32, done: i64, total: i64) -> zbus::Result<()>;
    #[zbus(signal)]
    pub async fn walk_batch(
        ctxt: &SignalContext<'_>,
        job: u32,
        entries: Vec<WireWalkItem>,
    ) -> zbus::Result<()>;
    #[zbus(signal)]
    pub async fn job_finished(
        ctxt: &SignalContext<'_>,
        job: u32,
        error: String,
        message: String,
        extra: HashMap<String, OwnedValue>,
    ) -> zbus::Result<()>;
}

impl Iface {
    fn handle(&self, handle: u32) -> Result<(String, Vec<u8>), Fail> {
        self.session
            .with(|s| s.handles.get(&handle).cloned())
            .ok_or_else(|| Fail::net("NotFound", "No such handle"))
    }
}

pub(super) fn context(session: &Session) -> Option<SignalContext<'static>> {
    let conn = session.conn()?;
    SignalContext::new(&conn, OBJECT_PATH).ok()
}

impl Shared {
    pub async fn decide(&self, change: impl FnOnce(&mut State)) {
        let (before, after, narrowed) = self.with(|s| {
            let ids: Vec<String> = s.locations.keys().cloned().collect();
            let old: Vec<Option<Level>> = ids.iter().map(|id| s.level(id)).collect();
            let before = s.consent.wire();
            change(s);
            let narrowed = ids.iter().zip(old).any(|(id, old)| match (old, s.level(id)) {
                (Some(old), Some(new)) => !new.covers(old),
                (Some(_), None) => true,
                (None, _) => false,
            });
            if s.consent == Consent::Unknown {
                s.own.clear();
            }
            (before, s.consent.wire(), narrowed)
        });
        if before != after {
            for session in self.sessions_snapshot() {
                if let Some(ctx) = context(&session) {
                    let _ = Iface::consent_changed(&ctx, after.to_owned()).await;
                }
            }
        }
        if narrowed {
            let stall = self.with(|s| s.narrowing_stall);
            if !stall.is_zero() {
                self.broadcast_locations().await;
                tokio::time::sleep(stall).await;
            }
            for session in self.take_sessions() {
                session.shut();
                if let Some(conn) = session.take_conn() {
                    let _ = conn.close().await;
                }
            }
        } else {
            self.broadcast_locations().await;
        }
    }

    pub async fn broadcast_locations(&self) {
        for session in self.sessions_snapshot() {
            if let Some(ctx) = context(&session) {
                let _ = Iface::locations_changed(&ctx).await;
            }
        }
    }

    pub async fn broadcast_network(&self) {
        let network = self.with(|s| s.network.clone());
        for session in self.sessions_snapshot() {
            if let Some(ctx) = context(&session) {
                let _ = Iface::network_changed(&ctx, network.clone()).await;
            }
        }
    }

    pub async fn broadcast_drives(&self) {
        for session in self.sessions_snapshot() {
            if let Some(ctx) = context(&session) {
                let _ = Iface::drives_changed(&ctx).await;
            }
        }
    }

    pub async fn broadcast_nearby(&self) {
        let nearby = self.with(|s| s.nearby.clone());
        for session in self.sessions_snapshot() {
            if !session.with(|s| s.discovering) {
                continue;
            }
            if let Some(ctx) = context(&session) {
                let _ = Iface::nearby_changed(&ctx, nearby.clone()).await;
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_contract_gives_every_input_signature() {
        let m = contract_methods();
        assert!(m.len() >= 35, "{}", m.len());
        assert_eq!(m["Hello"], "s");
        assert_eq!(m["GetConsent"], "");
        assert_eq!(m["Stat"], "saybs");
        assert_eq!(m["Upload"], "sayha{sv}");
        assert_eq!(m["CopyAcross"], "saysaya{sv}");
        assert!(!m.contains_key("ListBatch"), "signals are not methods");
    }

    #[test]
    fn the_xml_reader_follows_methods_and_ignores_other_interfaces() {
        let xml = r#"
            <interface name="org.netvfs.Bridge1">
              <method name="A">
                <arg name="x" type="s" direction="in"/>
                <arg name="y" type="u" direction="out"/>
                <arg name="z" type="ay" direction="in"/>
              </method>
              <method name="B"/>
              <signal name="S"><arg name="q" type="s"/></signal>
            </interface>
            <interface name="org.other">
              <method name="C"><arg type="s" direction="in"/></method>
            </interface>"#;
        let m = parse_methods(xml);
        assert_eq!(m.len(), 2);
        assert_eq!(m["A"], "say");
        assert_eq!(m["B"], "");
    }
}
