// SPDX-License-Identifier: LGPL-2.1-or-later
//! The `org.netvfs.Bridge1` interface of the fake. Checks happen in the order
//! of the real bridge (netvfs `session.cpp`): argument validation
//! (`InvalidArgs`, `InvalidName`), then `Hello` first, then consent, then the
//! scripted latency and failures, then the operation.

use super::args::{self, Dict};
use super::fail::Fail;
use super::state::{CancelOutcome, Consent, InfoValue, RecordedAnswer, Session, Shared};
use super::transfer::{self, Env};
use super::tree::Tree;
use super::{tree::TreeError, DEFAULT_CAPS};
use crate::wire::{
    WireCapabilities, WireEntry, WireLocation, WireNearby, WireWalkItem, INTERFACE, OBJECT_PATH,
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

/// The contract: input signatures of every method, read from the XML that the
/// real bridge serves.
const CONTRACT_XML: &str = include_str!("../../../../tests/bridge-contract/org.netvfs.Bridge1.xml");

/// Method name -> input signature, from the `Bridge1` interface of the contract.
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

/// One element per line, as in the contract file.
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

/// Wraps the generated interface: a call whose signature differs from the
/// contract fails with `InvalidArgs`, as in the real bridge (zbus alone would
/// answer with its own error name).
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
        if needs_consent && self.shared.with(|s| s.consent) != Consent::Granted {
            return Err(Fail::net(
                "PermissionDenied",
                "The user has not allowed this app to use network locations",
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

    fn require_location(&self, loc: &str) -> Result<(), Fail> {
        if self.shared.with(|s| s.locations.contains_key(loc)) {
            Ok(())
        } else {
            Err(Fail::net("NotFound", "No such location"))
        }
    }

    fn tree<R>(&self, loc: &str, f: impl FnOnce(&mut Tree) -> Result<R, TreeError>) -> Result<R, Fail> {
        self.require_location(loc)?;
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
            self.shared.with(|s| s.consent = answer);
            let shared = self.shared.clone();
            tokio::spawn(async move { shared.broadcast_consent(answer).await });
        }
    }

    /// Starts a job: validates the location, takes the id and runs `work`.
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

    async fn ask_in(
        &self,
        ctxt: &SignalContext<'_>,
        kind: &str,
        details: &[(String, InfoValue)],
    ) -> Option<RecordedAnswer> {
        let env = self.env(ctxt);
        env.ask(kind, details).await
    }

    async fn adhoc_questions(&self, ctxt: &SignalContext<'_>) -> Result<(), Fail> {
        let questions = self.shared.with(|s| s.adhoc_questions.clone());
        for q in questions {
            match self.ask_in(ctxt, &q.kind, &q.details).await {
                Some(a) if a.accept => {}
                Some(_) => return Err(declined(&q.kind)),
                None => return Err(Fail::net("Canceled", "The question was not answered")),
            }
        }
        Ok(())
    }
}

fn declined(kind: &str) -> Fail {
    match kind {
        "identity-unknown" => Fail::net("ServerIdentityUnknown", "The server identity was not accepted"),
        "keyboard-interactive" => Fail::net("AuthFailed", "The sign-in was cancelled"),
        _ => Fail::net("SecurityPolicy", "The insecure connection was declined"),
    }
}

struct AdHocUrl {
    provider: &'static str,
    host: String,
    port: Option<i32>,
    user: Option<String>,
    path: Vec<u8>,
}

fn parse_adhoc(url: &str, opts: &args::AdHocOpts) -> Result<AdHocUrl, Fail> {
    let (scheme, rest) = url
        .split_once("://")
        .ok_or_else(|| Fail::args("Not a valid URL"))?;
    let provider = match scheme {
        "sftp" | "ssh" => "sftp",
        "smb" => "smb",
        "ftp" | "ftps" => "ftp",
        "http" | "https" | "dav" | "davs" | "webdav" => "webdav",
        "fake" => "fake",
        "file" | "local" => {
            return Err(Fail::net(
                "PermissionDenied",
                "The bridge never accesses local files",
            ));
        }
        _ => {
            return Err(Fail::net(
                "Unsupported",
                "No backend for this scheme is installed",
            ))
        }
    };
    let (authority, path) = rest.split_once('/').unwrap_or((rest, ""));
    let (userinfo, hostport) = authority
        .rsplit_once('@')
        .map_or((None, authority), |(u, h)| (Some(u), h));
    if userinfo.is_some_and(|u| u.contains(':')) {
        return Err(Fail::net("SecurityPolicy", "A password in the URL is refused"));
    }
    let (host, port) = match hostport.rsplit_once(':') {
        Some((h, p)) => (h, Some(p.parse::<i32>().map_err(|_| Fail::args("Invalid port"))?)),
        None => (hostport, None),
    };
    if host.is_empty() {
        return Err(Fail::args("A URL needs a host"));
    }
    if opts.security_profile.is_some() && provider != "smb" {
        return Err(Fail::args("security_profile applies to SMB only"));
    }
    Ok(AdHocUrl {
        provider,
        host: host.to_owned(),
        port,
        user: opts.user.clone().or_else(|| userinfo.map(str::to_owned)),
        path: args::path(path.as_bytes())?,
    })
}

fn adhoc_info(p: &AdHocUrl) -> Vec<(String, InfoValue)> {
    let mut info = vec![
        ("kind".to_owned(), InfoValue::Str("adhoc".into())),
        ("host".to_owned(), InfoValue::Str(p.host.clone())),
        ("path".to_owned(), InfoValue::Bytes(p.path.clone())),
    ];
    if let Some(port) = p.port {
        info.push(("port".to_owned(), InfoValue::Int(port)));
    }
    if let Some(user) = &p.user {
        info.push(("user".to_owned(), InfoValue::Str(user.clone())));
    }
    info
}

#[zbus::interface(name = "org.netvfs.Bridge1")]
impl Iface {
    // ---- Session ---------------------------------------------------------

    async fn hello(&self, protocol: u32, client: String) -> Result<(u32, String, Vec<String>), Fail> {
        if protocol == 0 {
            return Err(Fail::args("Protocol version 0 does not exist"));
        }
        if client.len() > 256 {
            return Err(Fail::args("Argument 2 is too long"));
        }
        self.begin("Hello", "interactive", &client, false).await?;
        self.session.with(|s| s.hello = true);
        if self.shared.with(|s| s.consent) == Consent::Unknown {
            self.ask_consent();
        }
        Ok(self
            .shared
            .with(|s| (s.protocol, s.bridge_version.clone(), s.features.clone())))
    }

    async fn get_consent(&self) -> Result<String, Fail> {
        self.begin("GetConsent", "interactive", "", false).await?;
        Ok(self.shared.with(|s| s.consent).as_str().to_owned())
    }

    async fn request_consent(&self) -> Result<(), Fail> {
        self.begin("RequestConsent", "interactive", "", false).await?;
        if self.shared.with(|s| s.consent) != Consent::Granted {
            self.ask_consent();
        }
        Ok(())
    }

    // ---- Locations -------------------------------------------------------

    async fn list_locations(&self) -> Result<Vec<WireLocation>, Fail> {
        self.begin("ListLocations", "interactive", "", false).await?;
        Ok(self.shared.with(|s| {
            if s.consent == Consent::Granted {
                s.wire_locations()
            } else {
                Vec::new()
            }
        }))
    }

    // A one-element tuple keeps the reply a single struct argument on the wire.
    async fn capabilities(&self, loc: String) -> Result<(WireCapabilities,), Fail> {
        let loc = args::location(&loc)?;
        self.begin("Capabilities", "interactive", &loc, true).await?;
        self.require_location(&loc)?;
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
        self.require_location(&loc)
    }

    // ---- Ad-hoc locations --------------------------------------------------

    async fn connect_ad_hoc(
        &self,
        url: String,
        secret: Vec<u8>,
        opts: Dict,
        #[zbus(signal_context)] ctxt: SignalContext<'_>,
    ) -> Result<String, Fail> {
        args::url(&url)?;
        args::secret(&secret)?;
        let opts = args::adhoc_opts(&opts)?;
        self.begin("ConnectAdHoc", "interactive", &url, true).await?;
        let parsed = parse_adhoc(&url, &opts)?;
        self.shared.with(|s| s.received_secrets.push(secret));
        self.adhoc_questions(&ctxt).await?;
        let id = self.shared.with(|s| {
            let id = format!("adhoc:{}", s.next_adhoc);
            s.next_adhoc += 1;
            s.add_location(&id, parsed.provider, &parsed.host, adhoc_info(&parsed));
            id
        });
        self.shared.broadcast_locations().await;
        Ok(id)
    }

    async fn forget_ad_hoc(&self, loc: String) -> Result<(), Fail> {
        let loc = args::location(&loc)?;
        self.begin("ForgetAdHoc", "interactive", &loc, true).await?;
        if !loc.starts_with("adhoc:") || !self.shared.with(|s| s.locations.contains_key(&loc)) {
            return Err(Fail::net("NotFound", "No such ad-hoc location"));
        }
        self.shared.with(|s| s.remove_location(&loc));
        self.shared.broadcast_locations().await;
        Ok(())
    }

    // ---- Discovery -------------------------------------------------------

    async fn discover(&self, on: bool, #[zbus(signal_context)] ctxt: SignalContext<'_>) -> Result<(), Fail> {
        self.begin("Discover", "interactive", "", true).await?;
        self.env(&ctxt);
        self.session.with(|s| s.discovering = on);
        if on {
            // As netvfs' NearbyService: a start signals what is known right
            // away, after the reply, also when that is nothing yet.
            let shared = self.shared.clone();
            tokio::spawn(async move { shared.broadcast_nearby().await });
        } else if !self.shared.any_discovering() {
            // netvfs halts the discovery when its last client stops and
            // forgets what it found (Discovery::Private::halt).
            self.shared.with(|s| s.nearby.clear());
        }
        Ok(())
    }

    // ---- Listing -----------------------------------------------------------

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
        self.require_location(&loc)?;
        let listing = self.tree(&loc, |t| t.list(&dir));
        Ok(self.start_job(&ctxt, move |env, req, cancel| {
            transfer::run_list(env, req, listing, batch, cancel)
        }))
    }

    // ---- Metadata ----------------------------------------------------------

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
        self.tree(&loc, |t| t.stat(&path, follow)).map(|e| (e,))
    }

    async fn read_link(&self, loc: String, path: Vec<u8>) -> Result<Vec<u8>, Fail> {
        let loc = args::location(&loc)?;
        let path = args::path(&path)?;
        self.begin("ReadLink", "interactive", &loc, true).await?;
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
        if algo != "sha256" {
            return Err(Fail::net("Unsupported", "Unknown checksum algorithm"));
        }
        self.tree(&loc, |t| {
            t.file_data(&path).map(|d| sha2::Sha256::digest(d).to_vec())
        })
    }

    // ---- Namespace ---------------------------------------------------------

    async fn make_dir(&self, loc: String, path: Vec<u8>, exclusive: bool) -> Result<(), Fail> {
        let loc = args::location(&loc)?;
        let path = args::path(&path)?;
        self.begin("MakeDir", "interactive", &loc, true).await?;
        self.tree(&loc, |t| t.make_dir(&path, exclusive))
    }

    async fn remove_file(&self, loc: String, path: Vec<u8>) -> Result<(), Fail> {
        let loc = args::location(&loc)?;
        let path = args::path(&path)?;
        self.begin("RemoveFile", "interactive", &loc, true).await?;
        self.tree(&loc, |t| t.remove_file(&path))
    }

    async fn remove_dir(&self, loc: String, path: Vec<u8>) -> Result<(), Fail> {
        let loc = args::location(&loc)?;
        let path = args::path(&path)?;
        self.begin("RemoveDir", "interactive", &loc, true).await?;
        self.tree(&loc, |t| t.remove_dir(&path))
    }

    async fn rename(&self, loc: String, from: Vec<u8>, to: Vec<u8>, replace: bool) -> Result<(), Fail> {
        let loc = args::location(&loc)?;
        let from = args::path(&from)?;
        let to = args::path(&to)?;
        self.begin("Rename", "interactive", &loc, true).await?;
        self.tree(&loc, |t| t.rename(&from, &to, replace))
    }

    async fn set_attributes(&self, loc: String, path: Vec<u8>, changes: Dict) -> Result<(), Fail> {
        let loc = args::location(&loc)?;
        let path = args::path(&path)?;
        let changes = args::attributes(&changes)?;
        self.begin("SetAttributes", "interactive", &loc, true).await?;
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
        self.tree(&loc, |t| t.make_symlink(&target, &link))
    }

    async fn make_hardlink(&self, loc: String, existing: Vec<u8>, new_path: Vec<u8>) -> Result<(), Fail> {
        let loc = args::location(&loc)?;
        let existing = args::path(&existing)?;
        let new_path = args::path(&new_path)?;
        self.begin("MakeHardlink", "interactive", &loc, true).await?;
        self.tree(&loc, |t| t.make_hardlink(&existing, &new_path))
    }

    async fn server_copy(&self, loc: String, from: Vec<u8>, to: Vec<u8>, opts: Dict) -> Result<(), Fail> {
        let loc = args::location(&loc)?;
        let from = args::path(&from)?;
        let to = args::path(&to)?;
        let opts = args::copy_opts(&opts)?;
        self.begin("ServerCopy", "bulk", &loc, true).await?;
        self.tree(&loc, |t| t.server_copy(&from, &to, opts.recursive, opts.replace))
    }

    // ---- Handles -----------------------------------------------------------

    async fn open_read(&self, loc: String, path: Vec<u8>, lane: String) -> Result<(u32, i64), Fail> {
        let loc = args::location(&loc)?;
        let path = args::path(&path)?;
        let lane = args::lane(&lane, "stream")?;
        self.begin("OpenRead", lane, &loc, true).await?;
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

    // ---- Jobs --------------------------------------------------------------

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
        let file = transfer::Source::new(std::fs::File::from(std::os::fd::OwnedFd::from(fd)))?;
        self.require_location(&loc)?;
        Ok(self.start_job(&ctxt, move |env, job, cancel| {
            transfer::run_upload(env, job, cancel, loc, path, file, opts)
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
        let file = transfer::Source::new(std::fs::File::from(std::os::fd::OwnedFd::from(fd)))?;
        self.require_location(&loc)?;
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
        self.require_location(&src_loc)?;
        self.require_location(&dst_loc)?;
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
        self.require_location(&loc)?;
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
        self.require_location(&loc)?;
        Ok(self.start_job(&ctxt, move |env, job, cancel| {
            transfer::run_walk(env, job, cancel, loc, root, opts)
        }))
    }

    async fn cancel(&self, id: u32) -> Result<(), Fail> {
        self.begin("Cancel", "interactive", &id.to_string(), false)
            .await?;
        match self.session.cancel(id) {
            CancelOutcome::Unknown => Err(Fail::net("NotFound", "No such request or job")),
            CancelOutcome::Canceled | CancelOutcome::Finished => Ok(()),
        }
    }

    // ---- Questions ---------------------------------------------------------

    async fn answer(&self, id: String, answer: Dict) -> Result<(), Fail> {
        let id = args::location(&id)?;
        let answer = args::answer_opts(&answer)?;
        self.begin("Answer", "interactive", &id, false).await?;
        let Some(tx) = self.session.take_question(&id) else {
            return Err(Fail::net("NotFound", "No such question"));
        };
        let recorded = RecordedAnswer {
            id,
            accept: answer.accept,
            answers: answer.answers,
        };
        self.shared.with(|s| s.answers.push(recorded.clone()));
        // The asker may have given up; the answer is still recorded.
        let _ = tx.send(recorded);
        Ok(())
    }

    // ---- Handoff -----------------------------------------------------------

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

    // ---- Signals -----------------------------------------------------------

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
    #[zbus(signal)]
    pub async fn question(
        ctxt: &SignalContext<'_>,
        id: String,
        kind: String,
        details: HashMap<String, OwnedValue>,
    ) -> zbus::Result<()>;
}

impl Iface {
    fn handle(&self, handle: u32) -> Result<(String, Vec<u8>), Fail> {
        self.session
            .with(|s| s.handles.get(&handle).cloned())
            .ok_or_else(|| Fail::net("NotFound", "No such handle"))
    }
}

/// Signal context of a session, if its connection is up.
pub(super) fn context(session: &Session) -> Option<SignalContext<'static>> {
    let conn = session.conn()?;
    SignalContext::new(&conn, OBJECT_PATH).ok()
}

impl Shared {
    pub async fn broadcast_consent(&self, consent: Consent) {
        for session in self.sessions_snapshot() {
            if let Some(ctx) = context(&session) {
                // A closed client simply misses the signal.
                let _ = Iface::consent_changed(&ctx, consent.as_str().to_owned()).await;
            }
        }
    }

    pub async fn broadcast_locations(&self) {
        for session in self.sessions_snapshot() {
            if let Some(ctx) = context(&session) {
                let _ = Iface::locations_changed(&ctx).await;
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

    /// A question to the newest client.
    pub async fn ask(&self, kind: &str, details: &[(String, InfoValue)]) -> Option<RecordedAnswer> {
        let session = self.sessions_snapshot().pop()?;
        let ctx = context(&session)?;
        ask_question(self, &session, &ctx, kind, details).await
    }
}

/// Sends a `Question` signal and waits for the matching `Answer`. `None` when
/// the asker is gone before an answer arrives.
pub(super) async fn ask_question(
    shared: &Shared,
    session: &Session,
    ctx: &SignalContext<'_>,
    kind: &str,
    details: &[(String, InfoValue)],
) -> Option<RecordedAnswer> {
    let id = shared.with(|s| {
        let id = format!("q{}", s.next_question);
        s.next_question += 1;
        id
    });
    let rx = session.add_question(&id);
    let details: HashMap<String, OwnedValue> = details
        .iter()
        .map(|(k, v)| (k.clone(), v.to_owned_value()))
        .collect();
    Iface::question(ctx, id, kind.to_owned(), details).await.ok()?;
    rx.await.ok()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::fake::args::AdHocOpts;

    fn plain() -> AdHocOpts {
        AdHocOpts::default()
    }

    #[test]
    fn the_contract_gives_every_input_signature() {
        let m = contract_methods();
        assert!(m.len() >= 35, "{}", m.len());
        assert_eq!(m["Hello"], "us");
        assert_eq!(m["GetConsent"], "");
        assert_eq!(m["Stat"], "saybs");
        assert_eq!(m["ConnectAdHoc"], "saya{sv}");
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

    #[test]
    fn ad_hoc_urls_are_taken_apart() {
        let opts = AdHocOpts {
            user: Some("me".into()),
            security_profile: None,
        };
        let p = parse_adhoc("sftp://other@host.example:2222/a//b/", &opts).unwrap();
        assert_eq!(
            (p.provider, p.host.as_str(), p.port),
            ("sftp", "host.example", Some(2222))
        );
        assert_eq!(p.user.as_deref(), Some("me"), "the option wins over the URL");
        assert_eq!(p.path, b"a/b");
        let p = parse_adhoc("sftp://other@host/", &plain()).unwrap();
        assert_eq!(p.user.as_deref(), Some("other"));
        let info = adhoc_info(&p);
        assert!(info.contains(&("kind".to_owned(), InfoValue::Str("adhoc".into()))));
        assert!(info.contains(&("user".to_owned(), InfoValue::Str("other".into()))));
        assert!(!info.iter().any(|(k, _)| k == "port"));
        for (url, provider) in [
            ("https://h/", "webdav"),
            ("ftps://h/", "ftp"),
            ("smb://h/", "smb"),
        ] {
            assert_eq!(parse_adhoc(url, &plain()).unwrap().provider, provider);
        }
    }

    #[test]
    fn ad_hoc_urls_are_refused_as_the_bridge_refuses_them() {
        let bare = |url: &str, opts: &AdHocOpts| parse_adhoc(url, opts).err().map(|f| f.bare().to_owned());
        assert_eq!(
            bare("sftp://u:pw@h/", &plain()).as_deref(),
            Some("SecurityPolicy")
        );
        assert_eq!(bare("file:///x", &plain()).as_deref(), Some("PermissionDenied"));
        assert_eq!(bare("gopher://h/", &plain()).as_deref(), Some("Unsupported"));
        let legacy = AdHocOpts {
            user: None,
            security_profile: Some("legacy".into()),
        };
        assert!(bare("sftp://h/", &legacy).is_some_and(|n| n.contains("InvalidArgs")));
        assert!(bare("smb://h/", &legacy).is_none());
        assert!(bare("nonsense", &plain()).is_some());
        assert!(bare("sftp:///x", &plain()).is_some());
        assert!(bare("sftp://h:notaport/", &plain()).is_some());
        assert!(bare("sftp://h/a/../b", &plain()).is_some());
    }

    #[test]
    fn declining_maps_to_the_error_of_the_question() {
        assert_eq!(declined("identity-unknown").bare(), "ServerIdentityUnknown");
        assert_eq!(declined("keyboard-interactive").bare(), "AuthFailed");
        assert_eq!(declined("insecure-consent").bare(), "SecurityPolicy");
        assert_eq!(declined("anything-else").bare(), "SecurityPolicy");
    }
}
