// SPDX-License-Identifier: LGPL-2.1-or-later
//! In-process fake bridge (SPEC TST-2): serves `org.netvfs.Bridge1` over a
//! unix socket pair (or a real socket path) with an in-memory file tree per
//! location, configurable consent, locations, capabilities, scripted latency,
//! failures and disconnects. The same fake replays the shared contract
//! sequences ([`replay`]).

mod args;
mod fail;
mod iface;
pub mod replay;
mod state;
mod transfer;
pub mod tree;

pub use state::{Consent, Failure, InfoValue, JobFailure, RecordedAnswer, ScriptedQuestion};

use crate::connect::Connection;
use crate::errors::BridgeError;
use crate::wire::{WireCapabilities, WireLocation, WireNearby, OBJECT_PATH};
use state::{CallRecord, Shared, State};
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::Duration;
use tokio::net::{UnixListener, UnixStream};
use tokio::time::Instant;

/// Default capability set of a fake location.
pub const DEFAULT_CAPS: &[&str] = &[
    "Symlinks",
    "Hardlinks",
    "PosixModes",
    "SetModified",
    "ReadHandles",
    "EfficientRanges",
    "WriteResume",
    "ServerCopy",
    "SpaceInfo",
    "Checksums",
];

/// A fake bridge. Cheap to clone; clones share all state.
#[derive(Clone)]
pub struct FakeBridge {
    shared: Arc<Shared>,
}

impl Default for FakeBridge {
    fn default() -> FakeBridge {
        FakeBridge::new()
    }
}

impl FakeBridge {
    /// A bridge with consent granted and no locations.
    pub fn new() -> FakeBridge {
        FakeBridge {
            shared: Arc::new(Shared::new(State::default())),
        }
    }

    // ---- serving -------------------------------------------------------

    /// A client connection to a fresh in-process session over a socket pair.
    pub async fn connect(&self) -> Result<Connection, BridgeError> {
        let (client, server) = UnixStream::pair()?;
        let serving = self.serve_stream(server);
        let connecting = Connection::from_stream(client);
        let (served, connected) = tokio::join!(serving, connecting);
        served?;
        connected
    }

    /// Serves one client on `stream` (the server handshake).
    pub async fn serve_stream(&self, stream: UnixStream) -> Result<(), BridgeError> {
        let session = self.shared.new_session();
        let guid = zbus::Guid::generate();
        let conn = zbus::connection::Builder::unix_stream(stream)
            .server(guid)?
            .p2p()
            .serve_at(
                OBJECT_PATH,
                iface::Checked::new(self.shared.clone(), session.clone()),
            )?
            .build()
            .await?;
        session.set_conn(conn);
        Ok(())
    }

    /// Listens on a socket path like the bridge's socket-activated unit (the
    /// real app never creates this path; tests of the discovery do, here).
    pub fn listen(&self, path: &Path) -> std::io::Result<Listener> {
        if let Some(dir) = path.parent() {
            std::fs::create_dir_all(dir)?;
        }
        let _ = std::fs::remove_file(path);
        let listener = UnixListener::bind(path)?;
        let me = self.clone();
        let task = tokio::spawn(async move {
            loop {
                let Ok((stream, _)) = listener.accept().await else {
                    break;
                };
                me.shared.record_accept();
                if !me.shared.accepting() {
                    drop(stream);
                    continue;
                }
                let serving = me.clone();
                tokio::spawn(async move {
                    // A client that hangs up during the handshake is not an error here.
                    let _ = serving.serve_stream(stream).await;
                });
            }
        });
        Ok(Listener {
            path: path.to_path_buf(),
            task,
        })
    }

    /// While `false`, connections are accepted and closed at once (a bridge that
    /// keeps crashing), so reconnect attempts can be counted.
    pub fn set_accepting(&self, accepting: bool) {
        self.shared.with(|s| s.accepting = accepting);
    }

    /// Times at which connections reached the listener.
    pub fn accept_times(&self) -> Vec<Instant> {
        self.shared.with(|s| s.accepts.clone())
    }

    /// Closes every client connection (the client sees EOF, NVB-12).
    pub async fn disconnect_clients(&self) {
        for session in self.shared.take_sessions() {
            session.shut();
            if let Some(conn) = session.take_conn() {
                // The peer may already be gone.
                let _ = conn.close().await;
            }
        }
    }

    pub fn session_count(&self) -> usize {
        self.shared.with(|s| s.sessions.len())
    }

    // ---- bridge identity ------------------------------------------------

    /// What `Hello` answers (protocol version, bridge version).
    pub fn set_hello(&self, protocol: u32, version: &str) {
        self.shared.with(|s| {
            s.protocol = protocol;
            s.bridge_version = version.to_owned();
        });
    }

    // ---- consent --------------------------------------------------------

    pub fn consent(&self) -> Consent {
        self.shared.with(|s| s.consent)
    }

    /// Changes consent and tells every client (`ConsentChanged`).
    pub async fn set_consent(&self, consent: Consent) {
        self.shared.with(|s| s.consent = consent);
        self.shared.broadcast_consent(consent).await;
    }

    /// How often the user was asked (first `Hello` while `unknown`,
    /// `RequestConsent`).
    pub fn consent_requests(&self) -> u32 {
        self.shared.with(|s| s.consent_requests)
    }

    /// Answer the notification at once, as if the user tapped it.
    pub fn auto_consent(&self, answer: Option<Consent>) {
        self.shared.with(|s| s.auto_consent = answer);
    }

    // ---- locations ------------------------------------------------------

    /// An account location `account:<n>` (netvfs info keys `kind`, `host`,
    /// `accountId`, `path`).
    pub async fn add_account(&self, n: i32, provider: &str, name: &str, host: &str) -> String {
        let id = format!("account:{n}");
        let info = vec![
            ("kind".to_owned(), InfoValue::Str("account".into())),
            ("host".to_owned(), InfoValue::Str(host.into())),
            ("path".to_owned(), InfoValue::Bytes(Vec::new())),
            ("accountId".to_owned(), InfoValue::Int(n)),
        ];
        self.add_location(&id, provider, name, info).await;
        id
    }

    pub async fn add_location(&self, id: &str, provider: &str, name: &str, info: Vec<(String, InfoValue)>) {
        self.shared.with(|s| s.add_location(id, provider, name, info));
        self.shared.broadcast_locations().await;
    }

    pub async fn remove_location(&self, id: &str) {
        self.shared.with(|s| s.remove_location(id));
        self.shared.broadcast_locations().await;
    }

    /// Sets or clears the `attention` info key (`auth-failed`,
    /// `server-identity-changed`).
    pub async fn set_attention(&self, id: &str, attention: Option<&str>) {
        self.shared.with(|s| {
            if let Some(rec) = s.locations.get_mut(id) {
                rec.info.retain(|(k, _)| k != "attention");
                if let Some(a) = attention {
                    rec.info
                        .push(("attention".to_owned(), InfoValue::Str(a.to_owned())));
                }
            }
        });
        self.shared.broadcast_locations().await;
    }

    /// The `ListLocations` result as sent now.
    pub fn locations(&self) -> Vec<WireLocation> {
        self.shared.with(|s| s.wire_locations())
    }

    pub fn set_capabilities(
        &self,
        loc: &str,
        flags: &[&str],
        checksum_algorithms: &[&str],
        max_name_bytes: i64,
    ) {
        self.shared.with(|s| {
            s.caps.insert(
                loc.to_owned(),
                WireCapabilities {
                    flags: flags.iter().map(|f| (*f).to_owned()).collect(),
                    checksum_algorithms: checksum_algorithms.iter().map(|f| (*f).to_owned()).collect(),
                    max_name_bytes,
                },
            );
        });
    }

    pub fn set_space(&self, loc: &str, total: i64) {
        self.shared.with(|s| {
            s.space_total.insert(loc.to_owned(), total);
        });
    }

    // ---- discovery ------------------------------------------------------

    /// Replaces the discovery result and notifies clients that asked for it.
    pub async fn set_nearby(&self, nearby: Vec<WireNearby>) {
        self.shared.with(|s| s.nearby = nearby);
        self.shared.broadcast_nearby().await;
    }

    // ---- files ----------------------------------------------------------

    pub fn put_file(&self, loc: &str, path: &[u8], data: &[u8]) {
        self.shared.with(|s| s.tree_mut(loc).put_file(path, data));
    }

    pub fn put_dir(&self, loc: &str, path: &[u8]) {
        self.shared.with(|s| s.tree_mut(loc).put_dirs(path));
    }

    pub fn put_symlink(&self, loc: &str, path: &[u8], target: &[u8]) {
        self.shared.with(|s| s.tree_mut(loc).put_symlink(path, target));
    }

    /// Contents of a file on the fake server.
    pub fn file(&self, loc: &str, path: &[u8]) -> Option<Vec<u8>> {
        self.shared.with(|s| {
            s.trees
                .get(loc)
                .and_then(|t| t.file_data(path).ok().map(<[u8]>::to_vec))
        })
    }

    pub fn exists(&self, loc: &str, path: &[u8]) -> bool {
        self.shared
            .with(|s| s.trees.get(loc).is_some_and(|t| t.node(path).is_some()))
    }

    // ---- scripting ------------------------------------------------------

    /// Every call takes at least `latency` (virtual time under a paused clock).
    pub fn set_latency(&self, latency: Duration) {
        self.shared.with(|s| s.latency = latency);
    }

    /// Bytes per transfer step; progress is sent after each step.
    pub fn set_chunk_size(&self, bytes: usize) {
        self.shared.with(|s| s.chunk = bytes.max(1));
    }

    /// Fails matching calls with `org.netvfs.Error.<name>`.
    pub fn fail(&self, failure: Failure) {
        self.shared.with(|s| s.failures.push(failure));
    }

    /// Fails the next call of `method` once.
    pub fn fail_next(&self, method: &str, name: &str, message: &str) {
        self.fail(Failure::new(method, name, message).times(1));
    }

    /// Makes jobs end with an error in `JobFinished` after some bytes.
    pub fn fail_job(&self, failure: JobFailure) {
        self.shared.with(|s| s.job_failures.push(failure));
    }

    pub fn clear_failures(&self) {
        self.shared.with(|s| {
            s.failures.clear();
            s.job_failures.clear();
        });
    }

    /// Questions `ConnectAdHoc` asks (in order) before it connects.
    pub fn script_adhoc_questions(&self, questions: Vec<ScriptedQuestion>) {
        self.shared.with(|s| s.adhoc_questions = questions);
    }

    /// The bridge asks a question out of the blue (keyboard-interactive during
    /// a session); resolves with the app's answer.
    pub async fn ask(&self, kind: &str, details: Vec<(String, InfoValue)>) -> Option<RecordedAnswer> {
        self.shared.ask(kind, &details).await
    }

    // ---- observation ----------------------------------------------------

    /// Secrets that reached `ConnectAdHoc`.
    pub fn received_secrets(&self) -> Vec<Vec<u8>> {
        self.shared.with(|s| s.received_secrets.clone())
    }

    pub fn answers(&self) -> Vec<RecordedAnswer> {
        self.shared.with(|s| s.answers.clone())
    }

    /// Calls received, in order.
    pub fn calls(&self) -> Vec<CallRecord> {
        self.shared.with(|s| s.calls.clone())
    }

    pub fn calls_of(&self, method: &str) -> Vec<CallRecord> {
        self.calls().into_iter().filter(|c| c.method == method).collect()
    }

    pub fn clear_calls(&self) {
        self.shared.with(|s| s.calls.clear());
    }

    /// `settings:<loc>` and `add:<provider>` handoffs.
    pub fn handoffs(&self) -> Vec<String> {
        self.shared.with(|s| s.handoffs.clone())
    }

    pub fn open_handles(&self) -> usize {
        self.shared
            .sessions_snapshot()
            .iter()
            .map(|s| s.open_handles())
            .sum()
    }
}

/// A listening socket of the fake; dropping it closes and removes the socket
/// (the bridge is gone).
pub struct Listener {
    path: PathBuf,
    task: tokio::task::JoinHandle<()>,
}

impl Listener {
    pub fn path(&self) -> &Path {
        &self.path
    }
}

impl Drop for Listener {
    fn drop(&mut self) {
        self.task.abort();
        let _ = std::fs::remove_file(&self.path);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn locations_files_and_scripts_are_visible_to_the_test() {
        let fake = FakeBridge::new();
        let id = fake.add_account(7, "sftp", "NAS", "nas.local").await;
        assert_eq!(id, "account:7");
        fake.put_file(&id, b"d/f", b"data");
        fake.put_dir(&id, b"e");
        fake.put_symlink(&id, b"ln", b"d/f");
        assert_eq!(fake.file(&id, b"d/f").unwrap(), b"data");
        assert!(fake.exists(&id, b"e") && fake.exists(&id, b"ln"));
        assert!(!fake.exists(&id, b"nope") && !fake.exists("account:9", b"d"));
        assert_eq!(fake.locations().len(), 1);

        fake.set_attention(&id, Some("auth-failed")).await;
        let attention = crate::wire::value_str(&fake.locations()[0].info["attention"]);
        assert_eq!(attention.as_deref(), Some("auth-failed"));
        fake.set_attention(&id, None).await;
        assert!(!fake.locations()[0].info.contains_key("attention"));

        fake.set_consent(Consent::Unknown).await;
        assert_eq!(fake.consent(), Consent::Unknown);
        assert_eq!(fake.consent_requests(), 0);
        fake.remove_location(&id).await;
        assert!(fake.locations().is_empty());
        assert_eq!(fake.session_count(), 0);
        assert!(fake.calls().is_empty() && fake.handoffs().is_empty() && fake.answers().is_empty());
    }

    #[tokio::test]
    async fn the_listener_creates_and_removes_its_socket() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("a/b/bridge.sock");
        let fake = FakeBridge::new();
        let listener = fake.listen(&path).unwrap();
        assert_eq!(listener.path(), path);
        assert!(path.exists());
        drop(listener);
        assert!(!path.exists());
    }
}
