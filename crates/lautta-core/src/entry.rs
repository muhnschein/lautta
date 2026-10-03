// SPDX-License-Identifier: LGPL-2.1-or-later
//! Directory entries and location capabilities (SPEC Appendix B, BRW-1,
//! netvfs XB-9).

use std::collections::BTreeSet;
use std::time::SystemTime;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
pub enum Kind {
    #[default]
    Unknown,
    File,
    Dir,
    Symlink,
    Special,
}

impl Kind {
    /// netvfs wire value (XB-9: 0 Unknown, 1 File, 2 Directory, 3 Symlink, 4 Special).
    pub fn from_wire(v: u8) -> Kind {
        match v {
            1 => Kind::File,
            2 => Kind::Dir,
            3 => Kind::Symlink,
            4 => Kind::Special,
            _ => Kind::Unknown,
        }
    }

    pub fn to_wire(self) -> u8 {
        match self {
            Kind::Unknown => 0,
            Kind::File => 1,
            Kind::Dir => 2,
            Kind::Symlink => 3,
            Kind::Special => 4,
        }
    }
}

bitflags::bitflags! {
    #[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
    pub struct EntryFlags: u16 {
        const HIDDEN = 0x1;
        const READONLY = 0x2;
        const SYSTEM = 0x4;
        const NAME_NOT_UTF8 = 0x8;
        const TARGET_UNKNOWN = 0x10;
        /// Local symlink pointing outside the sandbox (BRW-10).
        const NOT_ACCESSIBLE = 0x100;
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct Entry {
    pub name: Vec<u8>,
    pub kind: Kind,
    /// For symlinks the kind of the target when known, otherwise equal to `kind`.
    pub target_kind: Kind,
    pub size: Option<u64>,
    pub modified: Option<SystemTime>,
    pub created: Option<SystemTime>,
    pub mode: Option<u32>,
    pub owner: Option<String>,
    pub group: Option<String>,
    pub flags: EntryFlags,
    pub etag: Option<Vec<u8>>,
    /// MIME type if the provider knows it (bridge `contentType`).
    pub content_type: Option<String>,
}

impl Entry {
    pub fn new(name: &[u8], kind: Kind) -> Entry {
        let mut flags = EntryFlags::empty();
        if std::str::from_utf8(name).is_err() {
            flags |= EntryFlags::NAME_NOT_UTF8;
        }
        if name.first() == Some(&b'.') {
            flags |= EntryFlags::HIDDEN;
        }
        Entry {
            name: name.to_vec(),
            kind,
            target_kind: kind,
            flags,
            ..Entry::default()
        }
    }

    /// Directory, following a symlink's target kind when known (BRW-1 isDir).
    pub fn is_dir(&self) -> bool {
        self.kind == Kind::Dir || (self.kind == Kind::Symlink && self.target_kind == Kind::Dir)
    }

    pub fn is_symlink(&self) -> bool {
        self.kind == Kind::Symlink
    }

    pub fn is_hidden(&self) -> bool {
        self.flags.contains(EntryFlags::HIDDEN) || self.name.first() == Some(&b'.')
    }

    pub fn display_name(&self) -> String {
        String::from_utf8_lossy(&self.name).into_owned()
    }

    pub fn name_is_lossy(&self) -> bool {
        self.flags.contains(EntryFlags::NAME_NOT_UTF8) || std::str::from_utf8(&self.name).is_err()
    }

    pub fn modified_ms(&self) -> Option<i64> {
        self.modified.map(system_time_to_ms)
    }
}

pub fn system_time_to_ms(t: SystemTime) -> i64 {
    match t.duration_since(SystemTime::UNIX_EPOCH) {
        Ok(d) => i64::try_from(d.as_millis()).unwrap_or(i64::MAX),
        Err(e) => -i64::try_from(e.duration().as_millis()).unwrap_or(i64::MAX),
    }
}

pub fn ms_to_system_time(ms: i64) -> SystemTime {
    if ms >= 0 {
        SystemTime::UNIX_EPOCH + std::time::Duration::from_millis(ms.unsigned_abs())
    } else {
        SystemTime::UNIX_EPOCH - std::time::Duration::from_millis(ms.unsigned_abs())
    }
}

/// Capability flags. Local capabilities depend on the filesystem type
/// (`statfs`); remote ones come from the bridge (`Capabilities(loc)`), whose
/// flag strings are kept verbatim in [`Capabilities::raw`].
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct Capabilities {
    pub raw: BTreeSet<String>,
    pub checksum_algorithms: Vec<String>,
    pub max_name_bytes: Option<u64>,
}

/// Well-known capability names (netvfs flag strings where they exist).
pub mod cap {
    pub const WRITE: &str = "Write";
    pub const SYMLINKS: &str = "Symlinks";
    pub const HARDLINKS: &str = "Hardlinks";
    pub const PERMISSIONS: &str = "Permissions";
    pub const SET_MTIME: &str = "SetMtime";
    pub const SERVER_COPY: &str = "ServerCopy";
    pub const RESUME_UPLOAD: &str = "ResumeUpload";
    pub const ATOMIC_PUT: &str = "AtomicPut";
    pub const CASE_INSENSITIVE: &str = "CaseInsensitive";
    pub const RANDOM_READ: &str = "RandomRead";
    pub const CHECKSUMS: &str = "Checksums";
    pub const SPACE_INFO: &str = "SpaceInfo";
    pub const ATOMIC_RENAME_NOREPLACE: &str = "RenameNoReplace";
    /// Local-only: deletes go to Recently deleted.
    pub const TRASH: &str = "Trash";
    /// Archives and offline caches.
    pub const READ_ONLY: &str = "ReadOnly";
    /// vfat/exFAT/SMB name rules (OPS-7).
    pub const RESTRICTED_NAMES: &str = "RestrictedNames";
}

impl Capabilities {
    pub fn with(flags: &[&str]) -> Capabilities {
        Capabilities {
            raw: flags.iter().map(|s| (*s).to_owned()).collect(),
            ..Capabilities::default()
        }
    }

    pub fn has(&self, flag: &str) -> bool {
        self.raw.contains(flag)
    }

    pub fn writable(&self) -> bool {
        self.has(cap::WRITE) && !self.has(cap::READ_ONLY)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn wire_kinds_round_trip() {
        for k in [Kind::Unknown, Kind::File, Kind::Dir, Kind::Symlink, Kind::Special] {
            assert_eq!(Kind::from_wire(k.to_wire()), k);
        }
        assert_eq!(Kind::from_wire(99), Kind::Unknown);
    }

    #[test]
    fn new_entry_flags() {
        let e = Entry::new(b".hidden", Kind::File);
        assert!(e.is_hidden());
        let e = Entry::new(b"bad\xff", Kind::File);
        assert!(e.name_is_lossy());
        assert!(!e.is_hidden());
    }

    #[test]
    fn symlink_to_dir_is_dir() {
        let mut e = Entry::new(b"l", Kind::Symlink);
        assert!(!e.is_dir());
        e.target_kind = Kind::Dir;
        assert!(e.is_dir());
        assert!(e.is_symlink());
    }

    #[test]
    fn time_conversion() {
        let t = ms_to_system_time(1_700_000_000_123);
        assert_eq!(system_time_to_ms(t), 1_700_000_000_123);
        let t = ms_to_system_time(-5000);
        assert_eq!(system_time_to_ms(t), -5000);
    }

    #[test]
    fn capabilities() {
        let c = Capabilities::with(&[cap::WRITE, cap::SYMLINKS]);
        assert!(c.writable());
        assert!(c.has(cap::SYMLINKS));
        assert!(!c.has(cap::PERMISSIONS));
        let c = Capabilities::with(&[cap::WRITE, cap::READ_ONLY]);
        assert!(!c.writable());
    }
}
