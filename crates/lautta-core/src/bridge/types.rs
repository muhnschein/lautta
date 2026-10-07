// SPDX-License-Identifier: LGPL-2.1-or-later
//! Public value types of the bridge client: status, consent, locations, nearby
//! servers, configuration (SPEC NVB-3, NVB-4, NVB-12, LOC-6, LOC-7).

use crate::paths::AppPaths;
use crate::uri::LocationId;
use crate::vpath::VPath;
use lautta_bridge_proto::{value_bytes, value_i64, value_str, WireLocation, WireNearby};
use std::path::PathBuf;
use std::time::Duration;

/// What the app knows about the bridge (SPEC §3.4 detection, NVB-1..3, NVB-12).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum BridgeStatus {
    /// No socket, or nothing answers: standalone mode, no message (NVB-1).
    Absent,
    /// A connection and the handshake are in progress.
    Connecting,
    /// The bridge speaks an older protocol than the app needs (NVB-2).
    TooOld,
    /// Waiting for the user's decision in netvfs' notification (NVB-3).
    ConsentUnknown,
    ConsentDenied,
    Ready,
    /// The connection was lost; reconnecting with backoff (NVB-12).
    Reconnecting,
}

impl BridgeStatus {
    /// True when bridge locations can be browsed.
    pub fn is_ready(self) -> bool {
        self == BridgeStatus::Ready
    }

    /// True when network locations are shown as "Reconnecting…" (NVB-12).
    pub fn is_reconnecting(self) -> bool {
        self == BridgeStatus::Reconnecting
    }
}

/// The user's decision for this app (netvfs XB-6).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Consent {
    Unknown,
    Granted,
    Denied,
}

impl Consent {
    /// Parses the wire string; anything unrecognised is `Unknown`, the
    /// state in which nothing is shown.
    pub fn from_wire(s: &str) -> Consent {
        match s {
            "granted" => Consent::Granted,
            "denied" => Consent::Denied,
            _ => Consent::Unknown,
        }
    }
}

/// Attention state netvfs reports for an account (NVB-4, netvfs `location.cpp`).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
pub enum Attention {
    #[default]
    None,
    /// The server did not accept the sign-in: *Update sign-in* (NVB-5).
    AuthFailed,
    /// The server presents another identity: *Review in Settings* (NVB-5).
    ServerIdentityChanged,
}

impl Attention {
    pub fn from_wire(s: &str) -> Attention {
        match s {
            "auth-failed" => Attention::AuthFailed,
            "server-identity-changed" => Attention::ServerIdentityChanged,
            _ => Attention::None,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum RemoteKind {
    /// A netvfs account with *Files* enabled.
    Account,
    /// A server of this app, from *Connect to server* (NVB-6).
    AdHoc,
}

/// A bridge location as the Servers section shows it (NVB-4).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RemoteLocation {
    /// `nv-<bridge id>` (LOC-7).
    pub id: LocationId,
    /// The id the bridge uses (`account:1`, `adhoc:3`).
    pub bridge_id: String,
    pub provider: String,
    pub name: String,
    pub kind: RemoteKind,
    pub host: String,
    pub port: Option<u16>,
    pub user: Option<String>,
    pub account_id: Option<i64>,
    /// Where the account starts browsing (`info.path`).
    pub start_path: VPath,
    /// Display and copy address, never containing a secret (LOC-7).
    pub url: Option<String>,
    pub attention: Attention,
}

/// Prefix of internal location ids of bridge locations (LOC-7).
pub const LOCATION_PREFIX: &str = "nv-";

/// `nv-account:1` for `account:1`.
pub fn location_id(bridge_id: &str) -> LocationId {
    format!("{LOCATION_PREFIX}{bridge_id}")
}

/// The bridge's id for an internal location id; ids without the prefix are
/// taken as bridge ids already.
pub fn bridge_id(location: &str) -> &str {
    location.strip_prefix(LOCATION_PREFIX).unwrap_or(location)
}

impl RemoteLocation {
    /// Reads a `ListLocations` element. A location with an odd `info` still
    /// appears (a newer bridge may add keys; missing ones are unknown).
    pub fn from_wire(w: &WireLocation) -> RemoteLocation {
        let str_of = |k: &str| w.info.get(k).and_then(|v| value_str(v));
        let int_of = |k: &str| w.info.get(k).and_then(|v| value_i64(v));
        let kind = if str_of("kind").as_deref() == Some("adhoc") {
            RemoteKind::AdHoc
        } else {
            RemoteKind::Account
        };
        RemoteLocation {
            id: location_id(&w.id),
            bridge_id: w.id.clone(),
            provider: w.provider.clone(),
            name: w.name.clone(),
            kind,
            host: str_of("host").unwrap_or_default(),
            port: int_of("port").and_then(|p| u16::try_from(p).ok()),
            user: str_of("user"),
            account_id: int_of("accountId"),
            start_path: w
                .info
                .get("path")
                .and_then(|v| value_bytes(v))
                .and_then(|b| VPath::parse(&b).ok())
                .unwrap_or_default(),
            url: str_of("url"),
            attention: str_of("attention").map_or(Attention::None, |a| Attention::from_wire(&a)),
        }
    }
}

/// A server found by discovery; tapping prefills *Connect to server*, nothing
/// connects automatically (LOC-6).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct NearbyServer {
    pub name: String,
    pub provider: String,
    pub host: String,
    pub port: u16,
    pub path: VPath,
}

impl NearbyServer {
    pub fn from_wire(w: &WireNearby) -> NearbyServer {
        NearbyServer {
            name: w.name.clone(),
            provider: w.provider.clone(),
            host: w.host.clone(),
            port: w.port,
            path: VPath::parse(&w.path).unwrap_or_default(),
        }
    }
}

/// Options of *Connect to server* besides the URL and the secret (NVB-6).
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct AdHocOptions {
    pub user: Option<String>,
    /// SMB only: `strict`, `signed`, `legacy` or `guest`.
    pub security_profile: Option<String>,
}

/// How the client finds and keeps the bridge.
#[derive(Debug, Clone)]
pub struct BridgeConfig {
    pub socket: PathBuf,
    /// Sent in `Hello`.
    pub client_name: String,
    /// Oldest bridge protocol the app can work with (NVB-2).
    pub required_protocol: u32,
    /// Waits before reconnect attempts after a loss; the last repeats (NVB-12).
    pub backoff: Vec<Duration>,
    /// Re-checks for a missing socket this often while in the foreground, in
    /// addition to start and [`crate::bridge::BridgeClient::poke`] (NVB-1).
    pub poll_interval: Option<Duration>,
    /// A bridge that accepts a connection but does not answer within this is as
    /// good as gone; `None` waits forever (tests on a paused clock).
    pub handshake_timeout: Option<Duration>,
    /// Whether the app is in the foreground at start.
    pub foreground: bool,
    /// How long nearby servers shown before a restart of the discovery stay
    /// while it finds them again (LOC-6).
    pub nearby_settle: Duration,
}

impl BridgeConfig {
    /// The defaults of the app: the socket of `paths`, `Hello("lautta <version>")`,
    /// backoff 1, 2, 4, 8 s then 15 s (NVB-12).
    pub fn for_app(paths: &AppPaths) -> BridgeConfig {
        BridgeConfig {
            socket: paths.bridge_socket(),
            client_name: format!("lautta {}", env!("CARGO_PKG_VERSION")),
            required_protocol: lautta_bridge_proto::PROTOCOL_VERSION,
            backoff: [1, 2, 4, 8, 15].map(Duration::from_secs).to_vec(),
            poll_interval: Some(Duration::from_secs(30)),
            handshake_timeout: Some(Duration::from_secs(10)),
            foreground: true,
            nearby_settle: Duration::from_secs(3),
        }
    }

    /// The wait before attempt number `attempt` (0 based) after a loss.
    pub fn backoff_for(&self, attempt: usize) -> Duration {
        self.backoff
            .get(attempt)
            .or_else(|| self.backoff.last())
            .copied()
            .unwrap_or(Duration::from_secs(15))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use lautta_bridge_proto::{OptsBuilder, WireLocation};

    fn wire(info: OptsBuilder) -> WireLocation {
        WireLocation {
            id: "account:7".into(),
            provider: "sftp".into(),
            name: "NAS".into(),
            info: info.build_owned(),
        }
    }

    #[test]
    fn location_ids_use_the_prefix() {
        assert_eq!(location_id("account:1"), "nv-account:1");
        assert_eq!(bridge_id("nv-adhoc:3"), "adhoc:3");
        assert_eq!(bridge_id("adhoc:3"), "adhoc:3");
    }

    #[test]
    fn account_info_is_read() {
        let loc = RemoteLocation::from_wire(&wire(
            OptsBuilder::new()
                .str("kind", "account")
                .str("host", "nas.local")
                .int("port", 22)
                .str("user", "me")
                .int("accountId", 7)
                .str("url", "sftp://nas.local/")
                .bytes("path", b"/docs//x")
                .str("attention", "auth-failed"),
        ));
        assert_eq!(loc.id, "nv-account:7");
        assert_eq!(loc.kind, RemoteKind::Account);
        assert_eq!((loc.host.as_str(), loc.port), ("nas.local", Some(22)));
        assert_eq!(loc.user.as_deref(), Some("me"));
        assert_eq!(loc.account_id, Some(7));
        assert_eq!(loc.start_path.as_bytes(), b"docs/x");
        assert_eq!(loc.attention, Attention::AuthFailed);
        assert_eq!(loc.url.as_deref(), Some("sftp://nas.local/"));
    }

    #[test]
    fn adhoc_and_sparse_info() {
        let loc = RemoteLocation::from_wire(&wire(OptsBuilder::new().str("kind", "adhoc")));
        assert_eq!(loc.kind, RemoteKind::AdHoc);
        assert_eq!(loc.host, "");
        assert_eq!(loc.attention, Attention::None);
        assert!(loc.start_path.is_root());
        let odd = RemoteLocation::from_wire(&wire(
            OptsBuilder::new().int("port", 70_000).str("attention", "mystery"),
        ));
        assert_eq!(odd.port, None);
        assert_eq!(odd.attention, Attention::None);
        assert_eq!(odd.kind, RemoteKind::Account);
    }

    #[test]
    fn attention_and_consent_strings() {
        assert_eq!(
            Attention::from_wire("server-identity-changed"),
            Attention::ServerIdentityChanged
        );
        assert_eq!(Attention::from_wire(""), Attention::None);
        assert_eq!(Consent::from_wire("granted"), Consent::Granted);
        assert_eq!(Consent::from_wire("denied"), Consent::Denied);
        assert_eq!(Consent::from_wire("whatever"), Consent::Unknown);
    }

    #[test]
    fn status_helpers() {
        assert!(BridgeStatus::Ready.is_ready());
        assert!(!BridgeStatus::ConsentUnknown.is_ready());
        assert!(BridgeStatus::Reconnecting.is_reconnecting());
        assert!(!BridgeStatus::Ready.is_reconnecting());
    }

    #[test]
    fn backoff_is_1_2_4_8_then_15() {
        let c = BridgeConfig::for_app(&AppPaths::new("/h"));
        let secs: Vec<u64> = (0..8).map(|i| c.backoff_for(i).as_secs()).collect();
        assert_eq!(secs, vec![1, 2, 4, 8, 15, 15, 15, 15]);
        assert!(c.client_name.starts_with("lautta "));
        assert!(c.socket.ends_with("netvfs/bridge.sock"));
        let none = BridgeConfig { backoff: vec![], ..c };
        assert_eq!(none.backoff_for(3).as_secs(), 15);
    }

    #[test]
    fn nearby_from_wire() {
        let n = NearbyServer::from_wire(&WireNearby {
            name: "NAS".into(),
            provider: "smb".into(),
            host: "nas".into(),
            port: 445,
            path: b"share/x".to_vec(),
        });
        assert_eq!(n.port, 445);
        assert_eq!(n.path.as_bytes(), b"share/x");
    }
}
