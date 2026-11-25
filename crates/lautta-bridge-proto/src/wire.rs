// SPDX-License-Identifier: LGPL-2.1-or-later

use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use zbus::zvariant::{OwnedValue, Type, Value};

pub const OBJECT_PATH: &str = "/org/netvfs/Bridge";
pub const INTERFACE: &str = "org.netvfs.Bridge1";
pub const MAX_READ_BYTES: u32 = 1 << 20;
pub const MAX_LIST_BATCH: u32 = 512;

#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct Bytes(pub Vec<u8>);

impl Type for Bytes {
    fn signature() -> zbus::zvariant::Signature<'static> {
        <Vec<u8>>::signature()
    }
}

impl<'de> Deserialize<'de> for Bytes {
    fn deserialize<D: serde::Deserializer<'de>>(d: D) -> Result<Bytes, D::Error> {
        struct Visit;
        impl serde::de::Visitor<'_> for Visit {
            type Value = Bytes;
            fn expecting(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
                f.write_str("a byte array")
            }
            fn visit_bytes<E: serde::de::Error>(self, v: &[u8]) -> Result<Bytes, E> {
                Ok(Bytes(v.to_vec()))
            }
        }
        d.deserialize_bytes(Visit)
    }
}

pub mod entry_flags {
    pub const HIDDEN: u16 = 0x1;
    pub const READ_ONLY: u16 = 0x2;
    pub const SYSTEM: u16 = 0x4;
    pub const NAME_NOT_UTF8: u16 = 0x8;
    pub const TARGET_UNKNOWN: u16 = 0x10;
}

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

#[derive(Debug, Clone, PartialEq, Eq, Default, Serialize, Deserialize, Type)]
pub struct WireCapabilities {
    pub flags: Vec<String>,
    pub checksum_algorithms: Vec<String>,
    pub max_name_bytes: i64,
}

pub mod location_state {
    pub const KEY: &str = "state";
    pub const ERROR_KEY: &str = "stateError";
    pub const DISCONNECTED: &str = "disconnected";
    pub const CONNECTING: &str = "connecting";
    pub const CONNECTED: &str = "connected";
    pub const FAILED: &str = "failed";
}

pub mod feature {
    pub const STORAGE_SETTINGS: &str = "storage-settings";
    pub const DRIVES: &str = "drives";
}

#[derive(Debug, Serialize, Deserialize, Type)]
pub struct WireDrive {
    pub id: String,
    pub label: String,
    pub mount_point: Vec<u8>,
    pub info: HashMap<String, OwnedValue>,
}

pub mod drive_info {
    pub const KIND: &str = "kind";
    pub const SIZE: &str = "size";
    pub const FS_TYPE: &str = "fsType";
    pub const READ_ONLY: &str = "readOnly";
    pub const UUID: &str = "uuid";
    pub const MODEL: &str = "model";
    pub const LOCKED: &str = "locked";
    pub const KIND_SD: &str = "sd";
    pub const KIND_USB: &str = "usb";
}

#[derive(Debug, Serialize, Deserialize, Type)]
pub struct WireLocation {
    pub id: String,
    pub provider: String,
    pub name: String,
    pub info: HashMap<String, OwnedValue>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Type)]
pub struct WireNearby {
    pub name: String,
    pub provider: String,
    pub host: String,
    pub port: u16,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Type)]
pub struct WireWalkItem {
    pub path: Vec<u8>,
    pub entry: WireEntry,
}

pub type Opts = HashMap<String, Value<'static>>;

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

    pub fn bytes(mut self, key: &str, v: &[u8]) -> OptsBuilder {
        self.0.insert(key.to_owned(), Value::from(v.to_vec()));
        self
    }

    pub fn build(self) -> Opts {
        self.0
    }

    pub fn build_owned(self) -> HashMap<String, OwnedValue> {
        self.0
            .into_iter()
            .filter_map(|(k, v)| OwnedValue::try_from(v).ok().map(|o| (k, o)))
            .collect()
    }
}

pub fn value_str(v: &Value<'_>) -> Option<String> {
    match v {
        Value::Str(s) => Some(s.as_str().to_owned()),
        _ => None,
    }
}

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

pub fn value_bool(v: &Value<'_>) -> Option<bool> {
    match v {
        Value::Bool(b) => Some(*b),
        _ => None,
    }
}

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
    fn read_bytes_decode_from_an_ay_reply() {
        assert_eq!(Bytes::signature().as_str(), "ay");
        for data in [Vec::new(), (0..=255u8).cycle().take(70_000).collect::<Vec<u8>>()] {
            let msg = zbus::Message::method("/a", "Read")
                .unwrap()
                .build(&(7u32, data.clone()))
                .unwrap();
            let (n, got): (u32, Bytes) = msg.body().deserialize().unwrap();
            assert_eq!((n, got.0), (7, data));
        }
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
            .build();
        assert_eq!(o.len(), 3);
        assert_eq!(value_bool(&o["b"]), Some(true));
        assert_eq!(value_i64(&o["n"]), Some(-5));
        assert_eq!(value_str(&o["s"]).as_deref(), Some("x"));
        assert_eq!(value_i64(&o["s"]), None);
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
