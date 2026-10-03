// SPDX-License-Identifier: LGPL-2.1-or-later
//! Filesystem types and what they can do (SPEC §10.1: capabilities come from
//! `statfs`; OPS-3 / OPS-7: vfat and exFAT are case-insensitive with
//! restricted names and no symlinks, hard links or permissions).

use crate::entry::{cap, Capabilities};
use std::path::Path;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum FsKind {
    Vfat,
    Exfat,
    Ntfs,
    Ext,
    Btrfs,
    Xfs,
    F2fs,
    Tmpfs,
    Overlay,
    /// Anything else, with its `f_type` magic.
    Other(u64),
}

impl FsKind {
    pub fn from_magic(magic: u64) -> FsKind {
        match magic {
            0x4d44 => FsKind::Vfat,
            0x2011_BAB0 => FsKind::Exfat,
            0x5346_544e | 0x7366_746e => FsKind::Ntfs,
            0xEF53 => FsKind::Ext,
            0x9123_683E => FsKind::Btrfs,
            0x5846_5342 => FsKind::Xfs,
            0xF2F5_2010 => FsKind::F2fs,
            0x0102_1994 => FsKind::Tmpfs,
            0x794c_7630 => FsKind::Overlay,
            other => FsKind::Other(other),
        }
    }

    /// FAT family: no POSIX semantics.
    pub fn is_fat_like(self) -> bool {
        matches!(self, FsKind::Vfat | FsKind::Exfat)
    }

    pub fn case_insensitive(self) -> bool {
        self.is_fat_like()
    }

    /// Capabilities that depend on the filesystem type alone.
    pub fn capabilities(self) -> Capabilities {
        let mut flags = vec![
            cap::WRITE,
            cap::RANDOM_READ,
            cap::CHECKSUMS,
            cap::SPACE_INFO,
            cap::SET_MTIME,
            cap::SERVER_COPY,
            cap::ATOMIC_RENAME_NOREPLACE,
        ];
        if self.is_fat_like() {
            flags.extend([cap::CASE_INSENSITIVE, cap::RESTRICTED_NAMES]);
        } else {
            flags.extend([cap::SYMLINKS, cap::HARDLINKS, cap::PERMISSIONS]);
        }
        let mut caps = Capabilities::with(&flags);
        caps.checksum_algorithms = vec!["sha256".into(), "sha1".into(), "md5".into()];
        caps.max_name_bytes = Some(255);
        caps
    }
}

/// The filesystem type under `path`. `Other(0)` when `statfs` fails (callers
/// then assume POSIX behaviour).
pub fn fs_kind(path: &Path) -> FsKind {
    match crate::sys::fs_magic(path) {
        Ok(m) => FsKind::from_magic(m),
        Err(_) => FsKind::Other(0),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn magic_numbers_map_to_kinds() {
        assert_eq!(FsKind::from_magic(0x4d44), FsKind::Vfat);
        assert_eq!(FsKind::from_magic(0x2011_BAB0), FsKind::Exfat);
        assert_eq!(FsKind::from_magic(0xEF53), FsKind::Ext);
        assert_eq!(FsKind::from_magic(0x0102_1994), FsKind::Tmpfs);
        assert_eq!(FsKind::from_magic(0x1234), FsKind::Other(0x1234));
    }

    #[test]
    fn fat_has_no_posix_features_but_case_rules() {
        for k in [FsKind::Vfat, FsKind::Exfat] {
            let c = k.capabilities();
            assert!(!c.has(cap::SYMLINKS) && !c.has(cap::HARDLINKS) && !c.has(cap::PERMISSIONS));
            assert!(c.has(cap::CASE_INSENSITIVE) && c.has(cap::RESTRICTED_NAMES));
            assert!(c.has(cap::WRITE) && c.has(cap::SET_MTIME) && c.has(cap::SERVER_COPY));
            assert!(k.case_insensitive());
        }
    }

    #[test]
    fn posix_filesystems_are_full_featured() {
        for k in [FsKind::Ext, FsKind::Btrfs, FsKind::F2fs, FsKind::Other(7)] {
            let c = k.capabilities();
            assert!(c.has(cap::SYMLINKS) && c.has(cap::HARDLINKS) && c.has(cap::PERMISSIONS));
            assert!(!c.has(cap::CASE_INSENSITIVE) && !c.has(cap::RESTRICTED_NAMES));
            for f in [cap::RANDOM_READ, cap::CHECKSUMS, cap::SPACE_INFO, cap::WRITE] {
                assert!(c.has(f), "{f}");
            }
            assert_eq!(c.checksum_algorithms, ["sha256", "sha1", "md5"]);
            assert!(!k.case_insensitive());
        }
    }

    #[test]
    fn fs_kind_of_real_dir_is_a_posix_kind() {
        let d = tempfile::tempdir().unwrap();
        assert!(!fs_kind(d.path()).is_fat_like());
        assert_eq!(fs_kind(&d.path().join("missing")), FsKind::Other(0));
    }
}
