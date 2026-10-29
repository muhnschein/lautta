// SPDX-License-Identifier: LGPL-2.1-or-later
//! Wire types of `org.netvfs.Bridge1` (netvfs SPEC-v2 XB-9). Paths and names
//! are `ay` (raw bytes), never `s` (SPEC NVB-8).

use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use zbus::zvariant::{OwnedValue, Type, Value};

/// Object path and interface of the bridge (XB-8).
pub const OBJECT_PATH: &str = "/org/netvfs/Bridge";
pub const INTERFACE: &str = "org.netvfs.Bridge1";
/// Largest `Read` the bridge accepts (XB-17).
pub const MAX_READ_BYTES: u32 = 1 << 20;
/// Largest `ListBatch` size the bridge accepts (XB-17).
pub const MAX_LIST_BATCH: u32 = 512;

/// Entry flags (XB-9).
pub mod entry_flags {
    pub const HIDDEN: u16 = 0x1;
    pub const READ_ONLY: u16 = 0x2;
    pub const SYSTEM: u16 = 0x4;
    pub const NAME_NOT_UTF8: u16 = 0x8;
    pub const TARGET_UNKNOWN: u16 = 0x10;
}

/// `(ayyyxxxxixxssqays)`; -1 or empty means unknown.
#[derive(Debug, Clone, PartialEq, Eq, Default, Serialize, Deserialize, Type)]
pub struct WireEntry {
    pub name: Vec<u8>,
    pub kind: u8,
    pub target_kind: u8,
    pub size: i64,
    pub mtime_ms: i64,
    pub ctime_ms: i64,
    pub atime_ms: i64,
    pub mode: i32,
    pub uid: i64,
    pub gid: i64,
    pub owner: String,
    pub group: String,
    pub flags: u16,
    pub etag: Vec<u8>,
    pub content_type: String,
}

impl WireEntry {
    /// An entry with every optional field unknown.
    pub fn unknown(name: &[u8], kind: u8) -> WireEntry {
        WireEntry {
            name: name.to_vec(),
            kind,
            target_kind: 0,
            size: -1,
            mtime_ms: -1,
            ctime_ms: -1,
            atime_ms: -1,
            mode: -1,
            uid: -1,
            gid: -1,
            ..WireEntry::default()
        }
    }
}

/// `(asasx)`
#[derive(Debug, Clone, PartialEq, Eq, Default, Serialize, Deserialize, Type)]
pub struct WireCapabilities {
    pub flags: Vec<String>,
    pub checksum_algorithms: Vec<String>,
    pub max_name_bytes: i64,
}

/// The connection state of a location: info key `state` of
/// `ListLocations` (netvfs SPEC-v2 XB-10, XB-12), with `stateError` (the
/// XC-21 name) when it is [`FAILED`](location_state::FAILED).
pub mod location_state {
    /// The info key of the state.
    pub const KEY: &str = "state";
    /// The info key of the error of a failed state.
    pub const ERROR_KEY: &str = "stateError";
    /// No connection of the consumer's pool is established or being
    /// established.
    pub const DISCONNECTED: &str = "disconnected";
    /// None is established, at least one is being established.
    pub const CONNECTING: &str = "connecting";
    /// At least one is established (connected and authenticated).
    pub const CONNECTED: &str = "connected";
    /// As [`DISCONNECTED`], but the last attempt to connect failed.
    pub const FAILED: &str = "failed";
}

/// The names `GetFeatures` answers (netvfs SPEC-v2 XB-19): the optional parts
/// of the interface a bridge offers. Names this client does not know are
/// ignored; a bridge without `GetFeatures` offers none.
pub mod feature {
    /// `OpenStorageSettings` opens Settings › Storage (XB-15).
    pub const STORAGE_SETTINGS: &str = "storage-settings";
    /// `ListDrives`, `Mount`, `Unmount` and `DrivesChanged` (XB-20).
    pub const DRIVES: &str = "drives";
}

/// `(ssaya{sv})`: a memory card or USB drive of `ListDrives`, mounted or not
/// (netvfs SPEC-v2 XB-20).
#[derive(Debug, Serialize, Deserialize, Type)]
pub struct WireDrive {
    /// Opaque to consumers (`mmcblk1p1`, `sda1`).
    pub id: String,
    /// The filesystem's label (a locked drive's: its LUKS header's); empty
    /// without one.
    pub label: String,
    /// Where it is mounted; empty when it is not.
    pub mount_point: Vec<u8>,
    /// [`drive_info`] keys; unknown ones are ignored.
    pub info: HashMap<String, OwnedValue>,
}

/// The info keys of a [`WireDrive`] (XB-20).
pub mod drive_info {
    /// `s`: [`KIND_SD`] or [`KIND_USB`].
    pub const KIND: &str = "kind";
    /// `x`: bytes.
    pub const SIZE: &str = "size";
    /// `s`: `vfat`, `exfat`, `ext4`, …
    pub const FS_TYPE: &str = "fsType";
    /// `b`.
    pub const READ_ONLY: &str = "readOnly";
    /// `s`, when known.
    pub const UUID: &str = "uuid";
    /// `s`: the drive's vendor and model, when known.
    pub const MODEL: &str = "model";
    /// `b`, true: a LUKS device not unlocked. Settings › Storage unlocks it;
    /// `Mount` answers `Unsupported`.
    pub const LOCKED: &str = "locked";
    /// A memory card.
    pub const KIND_SD: &str = "sd";
    /// A USB drive.
    pub const KIND_USB: &str = "usb";
}

/// `(sssa{sv})`: id, provider, display name, info (`kind`, `host`, `path`,
/// `attention`, `state`, …).
#[derive(Debug, Serialize, Deserialize, Type)]
pub struct WireLocation {
    pub id: String,
    pub provider: String,
    pub name: String,
    pub info: HashMap<String, OwnedValue>,
}

/// `(sssq)`: a discovered server; port 0 is the provider's default.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Type)]
pub struct WireNearby {
    pub name: String,
    pub provider: String,
    pub host: String,
    pub port: u16,
}

/// `(ay(ayyyxxxxixxssqays))` of `WalkBatch`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Type)]
pub struct WireWalkItem {
    pub path: Vec<u8>,
    pub entry: WireEntry,
}

/// An `a{sv}` argument built from plain Rust values.
pub type Opts = HashMap<String, Value<'static>>;

/// Builder for option dictionaries (`opts`, `changes`, `answer`).
#[derive(Debug, Default)]
pub struct OptsBuilder(Opts);

impl OptsBuilder {
    pub fn new() -> OptsBuilder {
        OptsBuilder::default()
    }

    pub fn bool(mut self, key: &str, v: bool) -> OptsBuilder {
        self.0.insert(key.to_owned(), Value::from(v));
        self
    }

    pub fn int(mut self, key: &str, v: i64) -> OptsBuilder {
        self.0.insert(key.to_owned(), Value::from(v));
        self
    }

    pub fn str(mut self, key: &str, v: &str) -> OptsBuilder {
        self.0.insert(key.to_owned(), Value::from(v.to_owned()));
        self
    }

    /// A byte array (`ay`), as `info.path` carries a path.
    pub fn bytes(mut self, key: &str, v: &[u8]) -> OptsBuilder {
        self.0.insert(key.to_owned(), Value::from(v.to_vec()));
        self
    }

    /// An array of byte arrays (`aay`), as `Answer` carries keyboard answers.
    pub fn byte_arrays(mut self, key: &str, v: &[Vec<u8>]) -> OptsBuilder {
        let items: Vec<Vec<u8>> = v.to_vec();
        self.0.insert(key.to_owned(), Value::from(items));
        self
    }

    pub fn build(self) -> Opts {
        self.0
    }

    /// The dictionary as the bridge sends it (`a{sv}` with owned values), for
    /// building server-side messages and test fixtures.
    pub fn build_owned(self) -> HashMap<String, OwnedValue> {
        self.0
            .into_iter()
            // Plain values hold no file descriptors, the only case that fails.
            .filter_map(|(k, v)| OwnedValue::try_from(v).ok().map(|o| (k, o)))
            .collect()
    }
}

/// Reads a string out of an `a{sv}` value.
pub fn value_str(v: &Value<'_>) -> Option<String> {
    match v {
        Value::Str(s) => Some(s.as_str().to_owned()),
        _ => None,
    }
}

/// Reads any integer out of an `a{sv}` value.
pub fn value_i64(v: &Value<'_>) -> Option<i64> {
    match v {
        Value::U8(n) => Some(i64::from(*n)),
        Value::I16(n) => Some(i64::from(*n)),
        Value::U16(n) => Some(i64::from(*n)),
        Value::I32(n) => Some(i64::from(*n)),
        Value::U32(n) => Some(i64::from(*n)),
        Value::I64(n) => Some(*n),
        Value::U64(n) => i64::try_from(*n).ok(),
        _ => None,
    }
}

/// Reads a boolean out of an `a{sv}` value.
pub fn value_bool(v: &Value<'_>) -> Option<bool> {
    match v {
        Value::Bool(b) => Some(*b),
        _ => None,
    }
}

/// Reads a byte array out of an `a{sv}` value (`ay`, as `info.path`).
pub fn value_bytes(v: &Value<'_>) -> Option<Vec<u8>> {
    match v {
        Value::Array(a) => a
            .iter()
            .map(|item| match item {
                Value::U8(b) => Some(*b),
                _ => None,
            })
            .collect(),
        _ => None,
    }
}

/// Reads an array of strings out of an `a{sv}` value.
pub fn value_strings(v: &Value<'_>) -> Option<Vec<String>> {
    match v {
        Value::Array(a) => a
            .iter()
            .map(|item| match item {
                Value::Str(s) => Some(s.as_str().to_owned()),
                _ => None,
            })
            .collect(),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn signatures_match_the_contract() {
        assert_eq!(WireEntry::signature().as_str(), "(ayyyxxxxixxssqays)");
        assert_eq!(WireCapabilities::signature().as_str(), "(asasx)");
        assert_eq!(WireLocation::signature().as_str(), "(sssa{sv})");
        assert_eq!(WireNearby::signature().as_str(), "(sssq)");
        assert_eq!(WireWalkItem::signature().as_str(), "(ay(ayyyxxxxixxssqays))");
        assert_eq!(WireDrive::signature().as_str(), "(ssaya{sv})");
    }

    #[test]
    fn unknown_entry_uses_minus_one() {
        let e = WireEntry::unknown(b"a", 1);
        assert_eq!((e.size, e.mtime_ms, e.mode, e.uid), (-1, -1, -1, -1));
        assert!(e.owner.is_empty() && e.etag.is_empty());
    }

    #[test]
    fn opts_builder_values_read_back() {
        let o = OptsBuilder::new()
            .bool("b", true)
            .int("n", -5)
            .str("s", "x")
            .byte_arrays("a", &[b"p".to_vec()])
            .build();
        assert_eq!(o.len(), 4);
        assert_eq!(value_bool(&o["b"]), Some(true));
        assert_eq!(value_i64(&o["n"]), Some(-5));
        assert_eq!(value_str(&o["s"]).as_deref(), Some("x"));
        assert_eq!(value_i64(&o["s"]), None);
        assert_eq!(o["a"].value_signature().as_str(), "aay");
    }

    #[test]
    fn value_helpers_decode_arrays_and_reject_other_types() {
        let s = Value::from("x");
        assert_eq!(value_bool(&s), None);
        assert_eq!(value_strings(&s), None);
        assert_eq!(value_bytes(&s), None);
        assert_eq!(value_bytes(&Value::from(vec![1u8, 2])), Some(vec![1, 2]));
        assert_eq!(
            value_strings(&Value::from(vec!["a".to_owned()])),
            Some(vec!["a".to_owned()])
        );
        assert_eq!(value_i64(&Value::from(7u16)), Some(7));
        assert_eq!(value_i64(&Value::from(u64::MAX)), None);
    }
}
