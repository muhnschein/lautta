// SPDX-License-Identifier: LGPL-2.1-or-later
//! Conversions between bridge wire types and core types.

use crate::entry::{cap, ms_to_system_time, Capabilities, Entry, EntryFlags, Kind};
use crate::error::{Error, ErrorKind};
use lautta_bridge_proto::{BridgeError, WireCapabilities, WireEntry};
use std::collections::{BTreeSet, HashMap};

/// Maps a bridge failure to the app's one error model (SPEC RS-8): netvfs
/// names map one-to-one (`ErrorKind::from_name`), a lost socket is
/// `ConnectionLost`, malformed calls cannot be the user's doing.
pub fn map_error(e: BridgeError) -> Error {
    match e {
        BridgeError::Remote {
            name,
            message,
            detail,
            retry_after_ms,
        } => Error {
            kind: ErrorKind::from_name(&name),
            message,
            detail,
            retry_after_ms,
        },
        BridgeError::InvalidArgs(m) => Error::new(ErrorKind::InvalidArgument, m),
        BridgeError::UnknownMethod(m) => Error::new(ErrorKind::Unsupported, m),
        BridgeError::Disconnected(m) => Error::new(ErrorKind::ConnectionLost, m),
        BridgeError::Protocol(m) => Error::new(ErrorKind::ProtocolError, m),
    }
}

/// The error of `ListDone` / `JobFinished` (a bare name, empty on success).
pub fn finished_error(
    name: &str,
    message: &str,
    extra: &HashMap<String, lautta_bridge_proto::zvariant::OwnedValue>,
) -> Option<Error> {
    BridgeError::from_bare(name, message, extra).map(map_error)
}

fn opt_time(ms: i64) -> Option<std::time::SystemTime> {
    (ms != -1).then(|| ms_to_system_time(ms))
}

fn non_empty(s: &str) -> Option<String> {
    (!s.is_empty()).then(|| s.to_owned())
}

/// Wire entry to [`Entry`]; `-1` and empty values are unknown (XB-9).
pub fn entry_from_wire(w: &WireEntry) -> Entry {
    let kind = Kind::from_wire(w.kind);
    let mut e = Entry::new(&w.name, kind);
    e.target_kind = match Kind::from_wire(w.target_kind) {
        Kind::Unknown => kind,
        t => t,
    };
    e.size = u64::try_from(w.size).ok();
    e.modified = opt_time(w.mtime_ms);
    e.created = opt_time(w.ctime_ms);
    e.mode = u32::try_from(w.mode).ok();
    e.owner = non_empty(&w.owner);
    e.group = non_empty(&w.group);
    e.flags |= EntryFlags::from_bits_truncate(w.flags);
    e.etag = (!w.etag.is_empty()).then(|| w.etag.clone());
    e.content_type = non_empty(&w.content_type);
    e
}

/// netvfs capability names that the app's `cap::*` names spell differently.
const RENAMED: [(&str, &str); 6] = [
    ("PosixModes", cap::PERMISSIONS),
    ("SetModified", cap::SET_MTIME),
    ("ReadHandles", cap::RANDOM_READ),
    ("WriteResume", cap::RESUME_UPLOAD),
    ("NativeNoReplace", cap::ATOMIC_RENAME_NOREPLACE),
    ("WindowsNames", cap::RESTRICTED_NAMES),
];

/// `Capabilities(loc)` to [`Capabilities`]. The bridge's own flag names stay in
/// `raw` next to the app's names; the bridge has no read-only flag, a bridge
/// location is writable unless an operation says otherwise.
pub fn capabilities_from_wire(w: &WireCapabilities) -> Capabilities {
    let mut raw: BTreeSet<String> = w.flags.iter().cloned().collect();
    for (netvfs, app) in RENAMED {
        if raw.contains(netvfs) {
            raw.insert(app.to_owned());
        }
    }
    raw.insert(cap::WRITE.to_owned());
    Capabilities {
        raw,
        checksum_algorithms: w.checksum_algorithms.clone(),
        max_name_bytes: u64::try_from(w.max_name_bytes).ok().filter(|n| *n > 0),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::Duration;

    #[test]
    fn error_names_map_one_to_one() {
        let e = map_error(BridgeError::Remote {
            name: "org.netvfs.Error.RateLimited".into(),
            message: "slow".into(),
            detail: Some("429".into()),
            retry_after_ms: Some(900),
        });
        assert_eq!(e.kind, ErrorKind::RateLimited);
        assert_eq!(e.message, "slow");
        assert_eq!(e.detail.as_deref(), Some("429"));
        assert_eq!(e.retry_after_ms, Some(900));
        assert_eq!(
            map_error(BridgeError::remote("Brandnew", "")).kind,
            ErrorKind::ProtocolError
        );
        assert_eq!(
            map_error(BridgeError::InvalidArgs("x".into())).kind,
            ErrorKind::InvalidArgument
        );
        assert_eq!(
            map_error(BridgeError::UnknownMethod("x".into())).kind,
            ErrorKind::Unsupported
        );
        assert_eq!(
            map_error(BridgeError::Disconnected("x".into())).kind,
            ErrorKind::ConnectionLost
        );
        assert_eq!(
            map_error(BridgeError::Protocol("x".into())).kind,
            ErrorKind::ProtocolError
        );
    }

    #[test]
    fn finished_errors() {
        let none = HashMap::new();
        assert!(finished_error("", "", &none).is_none());
        let e = finished_error("NoSpace", "full", &none).unwrap();
        assert_eq!((e.kind, e.message.as_str()), (ErrorKind::NoSpace, "full"));
    }

    #[test]
    fn entries_with_unknowns() {
        let mut w = WireEntry::unknown(b"a.txt", 1);
        w.size = 5;
        let e = entry_from_wire(&w);
        assert_eq!(e.kind, Kind::File);
        assert_eq!(e.size, Some(5));
        assert_eq!(
            (e.modified, e.created, e.mode, e.owner.clone()),
            (None, None, None, None)
        );
        assert_eq!(e.target_kind, Kind::File);
        assert!(e.etag.is_none() && e.content_type.is_none());
    }

    #[test]
    fn entries_with_everything() {
        let w = WireEntry {
            name: b"link".to_vec(),
            kind: 3,
            target_kind: 2,
            size: 0,
            mtime_ms: 1_700_000_000_500,
            ctime_ms: 1_700_000_000_000,
            atime_ms: -1,
            mode: 0o755,
            uid: 1,
            gid: 2,
            owner: "me".into(),
            group: "users".into(),
            flags: 0x2 | 0x10,
            etag: b"e1".to_vec(),
            content_type: "text/plain".into(),
        };
        let e = entry_from_wire(&w);
        assert_eq!((e.kind, e.target_kind), (Kind::Symlink, Kind::Dir));
        assert!(e.is_dir());
        assert_eq!(e.size, Some(0));
        assert_eq!(
            e.modified,
            Some(std::time::UNIX_EPOCH + Duration::from_millis(1_700_000_000_500))
        );
        assert_eq!(e.mode, Some(0o755));
        assert_eq!(e.owner.as_deref(), Some("me"));
        assert_eq!(e.group.as_deref(), Some("users"));
        assert!(e
            .flags
            .contains(EntryFlags::READONLY | EntryFlags::TARGET_UNKNOWN));
        assert_eq!(e.etag.as_deref(), Some(&b"e1"[..]));
        assert_eq!(e.content_type.as_deref(), Some("text/plain"));
    }

    #[test]
    fn odd_names_are_flagged() {
        let e = entry_from_wire(&WireEntry::unknown(b"caf\xe9", 1));
        assert!(e.name_is_lossy());
        assert_eq!(e.name, b"caf\xe9");
        let e = entry_from_wire(&WireEntry::unknown(b".rc", 1));
        assert!(e.is_hidden());
    }

    #[test]
    fn capability_names_are_mapped_and_kept() {
        let c = capabilities_from_wire(&WireCapabilities {
            flags: vec![
                "Symlinks".into(),
                "PosixModes".into(),
                "WriteResume".into(),
                "ReadHandles".into(),
                "ShellExec".into(),
            ],
            checksum_algorithms: vec!["sha256".into()],
            max_name_bytes: 255,
        });
        for flag in [
            cap::SYMLINKS,
            cap::PERMISSIONS,
            cap::RESUME_UPLOAD,
            cap::RANDOM_READ,
            cap::WRITE,
        ] {
            assert!(c.has(flag), "{flag}");
        }
        assert!(c.has("PosixModes") && c.has("ShellExec"));
        assert!(!c.has(cap::SET_MTIME));
        assert_eq!(c.checksum_algorithms, vec!["sha256".to_owned()]);
        assert_eq!(c.max_name_bytes, Some(255));
        assert!(c.writable());
        let none = capabilities_from_wire(&WireCapabilities::default());
        assert_eq!(none.max_name_bytes, None);
    }
}
