// SPDX-License-Identifier: LGPL-2.1-or-later
//! Value types of the bridge client.

use crate::locations::ConnectionState;
use crate::paths::AppPaths;
use crate::uri::LocationId;
use crate::vpath::VPath;
use lautta_bridge_proto::{
    drive_info, location_state, value_bool, value_bytes, value_i64, value_str, WireDrive, WireLocation,
    WireNearby,
};
use std::ffi::OsStr;
use std::os::unix::ffi::OsStrExt;
use std::path::PathBuf;
use std::time::Duration;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum BridgeStatus {
    /// No socket, or nothing answers: standalone, no message.
    Absent,
    Connecting,
    ConsentUnknown,
    ConsentDenied,
    Ready,
    Reconnecting,
}

impl BridgeStatus {
    pub fn is_ready(self) -> bool {
        self == BridgeStatus::Ready
    }

    pub fn is_reconnecting(self) -> bool {
        self == BridgeStatus::Reconnecting
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Consent {
    Unknown,
    Granted,
    Denied,
}

impl Consent {
    /// Anything unrecognised is `Unknown`: nothing shown.
    pub fn from_wire(s: &str) -> Consent {
        match s {
            "granted" => Consent::Granted,
            "denied" => Consent::Denied,
            _ => Consent::Unknown,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
pub enum Attention {
    #[default]
    None,
    AuthFailed,
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
    Account,
    AdHoc,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RemoteLocation {
    pub id: LocationId,
    /// Bridge's id (`account:1`, `adhoc:3`).
    pub bridge_id: String,
    pub provider: String,
    pub name: String,
    pub kind: RemoteKind,
    pub host: String,
    pub port: Option<u16>,
    pub user: Option<String>,
    pub account_id: Option<i64>,
    pub start_path: VPath,
    /// Never contains a secret.
    pub url: Option<String>,
    pub attention: Attention,
    pub connection: ConnectionState,
}

/// Missing or unknown state is [`ConnectionState::Unknown`]: a bridge not reporting states shows
/// none rather than a wrong one.
fn connection_from_wire(state: Option<&str>, error: Option<String>) -> ConnectionState {
    match state {
        Some(location_state::DISCONNECTED) => ConnectionState::Disconnected,
        Some(location_state::CONNECTING) => ConnectionState::Connecting,
        Some(location_state::CONNECTED) => ConnectionState::Connected,
        Some(location_state::FAILED) => ConnectionState::Failed(error.unwrap_or_default()),
        _ => ConnectionState::Unknown,
    }
}

pub const LOCATION_PREFIX: &str = "nv-";

pub fn location_id(bridge_id: &str) -> LocationId {
    format!("{LOCATION_PREFIX}{bridge_id}")
}

/// Ids without the prefix are taken as bridge ids.
pub fn bridge_id(location: &str) -> &str {
    location.strip_prefix(LOCATION_PREFIX).unwrap_or(location)
}

impl RemoteLocation {
    /// A location with an odd `info` still appears (unknown keys ignored; missing ones unknown).
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
            connection: connection_from_wire(
                str_of(location_state::KEY).as_deref(),
                str_of(location_state::ERROR_KEY),
            ),
        }
    }
}

/// Tapping prefills *Connect to server*; nothing connects automatically.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct NearbyServer {
    pub name: String,
    pub provider: String,
    pub host: String,
    /// 0 for the provider's default.
    pub port: u16,
}

impl NearbyServer {
    pub fn from_wire(w: &WireNearby) -> NearbyServer {
        NearbyServer {
            name: w.name.clone(),
            provider: w.provider.clone(),
            host: w.host.clone(),
            port: w.port,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DriveKind {
    Card,
    /// Also one the bridge names in a way this app does not know.
    Usb,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Drive {
    /// Bridge's id (`mmcblk1p1`, `sda1`); opaque.
    pub id: String,
    pub label: String,
    pub uuid: String,
    pub mount_point: Option<PathBuf>,
    pub kind: DriveKind,
    /// Bytes, -1 when unknown.
    pub size: i64,
    /// Locked LUKS device: Settings › Storage unlocks it; bridge never mounts it.
    pub locked: bool,
}

impl Drive {
    pub fn from_wire(w: &WireDrive) -> Drive {
        let text = |key: &str| w.info.get(key).and_then(|v| value_str(v)).unwrap_or_default();
        Drive {
            id: w.id.clone(),
            label: w.label.clone(),
            uuid: text(drive_info::UUID),
            mount_point: (!w.mount_point.is_empty())
                .then(|| PathBuf::from(OsStr::from_bytes(&w.mount_point))),
            kind: if text(drive_info::KIND) == drive_info::KIND_SD {
                DriveKind::Card
            } else {
                DriveKind::Usb
            },
            size: w
                .info
                .get(drive_info::SIZE)
                .and_then(|v| value_i64(v))
                .unwrap_or(-1),
            locked: w
                .info
                .get(drive_info::LOCKED)
                .and_then(|v| value_bool(v))
                .unwrap_or(false),
        }
    }

    /// Folder below the UDisks2 media folder, as Settings mounts it (udisks'
    /// `calculate_mount_point`): label with `/` as `_`, else UUID, else `disk`. Also the name shown
    /// while unmounted.
    pub fn mount_name(&self) -> String {
        if !self.label.is_empty() {
            self.label.replace('/', "_")
        } else if !self.uuid.is_empty() {
            self.uuid.clone()
        } else {
            "disk".to_owned()
        }
    }
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct AdHocOptions {
    pub user: Option<String>,
    /// SMB only: `strict`, `signed`, `legacy` or `guest`.
    pub security_profile: Option<String>,
}

#[derive(Debug, Clone)]
pub struct BridgeConfig {
    pub socket: PathBuf,
    pub client_name: String,
    /// Waits before reconnect attempts after a loss; the last repeats.
    pub backoff: Vec<Duration>,
    pub poll_interval: Option<Duration>,
    /// A bridge that accepts a connection but does not answer within this is as good as gone;
    /// `None` waits forever (tests on a paused clock).
    pub handshake_timeout: Option<Duration>,
    pub foreground: bool,
    pub nearby_settle: Duration,
}

impl BridgeConfig {
    /// Socket of `paths`, `Hello("lautta <version>")`, backoff 1, 2, 4, 8 s then 15 s.
    pub fn for_app(paths: &AppPaths) -> BridgeConfig {
        BridgeConfig {
            socket: paths.bridge_socket(),
            client_name: format!("lautta {}", env!("CARGO_PKG_VERSION")),
            backoff: [1, 2, 4, 8, 15].map(Duration::from_secs).to_vec(),
            poll_interval: Some(Duration::from_secs(30)),
            handshake_timeout: Some(Duration::from_secs(10)),
            foreground: true,
            nearby_settle: Duration::from_secs(3),
        }
    }

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
        assert_eq!(loc.connection, ConnectionState::Unknown, "no state was sent");
    }

    #[test]
    fn connection_states_are_read() {
        let state = |info: OptsBuilder| RemoteLocation::from_wire(&wire(info)).connection;
        assert_eq!(
            state(OptsBuilder::new().str("state", "disconnected")),
            ConnectionState::Disconnected
        );
        assert_eq!(
            state(OptsBuilder::new().str("state", "connecting")),
            ConnectionState::Connecting
        );
        assert_eq!(
            state(
                OptsBuilder::new()
                    .str("state", "connected")
                    .str("stateError", "Timeout")
            ),
            ConnectionState::Connected,
            "only a failure has an error"
        );
        assert_eq!(
            state(
                OptsBuilder::new()
                    .str("state", "failed")
                    .str("stateError", "AuthFailed")
            ),
            ConnectionState::Failed("AuthFailed".into())
        );
        assert_eq!(
            state(OptsBuilder::new().str("state", "failed")),
            ConnectionState::Failed(String::new())
        );
        // Older bridges send none; a newer bridge's state is not guessed at.
        assert_eq!(state(OptsBuilder::new()), ConnectionState::Unknown);
        assert_eq!(
            state(OptsBuilder::new().str("state", "suspended")),
            ConnectionState::Unknown
        );
        assert_eq!(
            state(OptsBuilder::new().int("state", 1)),
            ConnectionState::Unknown
        );
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
        });
        assert_eq!(
            (n.name.as_str(), n.provider.as_str(), n.host.as_str(), n.port),
            ("NAS", "smb", "nas", 445)
        );
    }

    fn wire_drive(label: &str, mount_point: &[u8], info: OptsBuilder) -> WireDrive {
        WireDrive {
            id: "sda1".into(),
            label: label.into(),
            mount_point: mount_point.to_vec(),
            info: info.build_owned(),
        }
    }

    #[test]
    fn drives_from_wire() {
        let card = Drive::from_wire(&wire_drive(
            "CARD",
            b"/run/media/defaultuser/CARD",
            OptsBuilder::new()
                .str("kind", "sd")
                .int("size", 64)
                .str("uuid", "1234-ABCD"),
        ));
        assert_eq!(card.kind, DriveKind::Card);
        assert_eq!(
            (card.id.as_str(), card.size, card.uuid.as_str()),
            ("sda1", 64, "1234-ABCD")
        );
        assert_eq!(
            card.mount_point,
            Some(PathBuf::from("/run/media/defaultuser/CARD"))
        );
        assert_eq!(card.mount_name(), "CARD");
        assert!(!card.locked);
        let other = Drive::from_wire(&wire_drive("a/b", b"", OptsBuilder::new().str("kind", "nvme")));
        assert_eq!(
            (other.kind, other.size, other.mount_point.as_deref()),
            (DriveKind::Usb, -1, None)
        );
        assert_eq!(other.mount_name(), "a_b");
    }

    #[test]
    fn a_locked_drive_from_wire() {
        let locked = |info: OptsBuilder| Drive::from_wire(&wire_drive("VAULT", b"", info)).locked;
        assert!(locked(OptsBuilder::new().str("kind", "sd").bool("locked", true)));
        assert!(!locked(OptsBuilder::new().bool("locked", false)));
        assert!(!locked(OptsBuilder::new().str("locked", "yes")));
    }

    #[test]
    fn unlabelled_drives_are_mounted_by_uuid_or_as_disk() {
        let by_uuid = Drive::from_wire(&wire_drive("", b"", OptsBuilder::new().str("uuid", "1234-ABCD")));
        assert_eq!(by_uuid.mount_name(), "1234-ABCD");
        let bare = Drive::from_wire(&wire_drive("", b"", OptsBuilder::new()));
        assert_eq!(bare.mount_name(), "disk");
    }
}
