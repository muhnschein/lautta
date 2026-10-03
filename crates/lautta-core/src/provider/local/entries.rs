// SPDX-License-Identifier: LGPL-2.1-or-later
//! `Entry` construction from `statx` results (BRW-1, BRW-10).

use super::names::Names;
use crate::entry::{Entry, EntryFlags, Kind};
use crate::sys::{self, FileStat};
use std::io;
use std::os::unix::ffi::OsStrExt;
use std::path::{Component, Path, PathBuf};
use std::sync::Arc;

/// Turns stat results into entries. Shared by all blocking tasks of one provider.
pub struct EntryBuilder {
    root: PathBuf,
    names: Arc<Names>,
    euid: u32,
    egid: u32,
}

pub fn kind_of(st: &FileStat) -> Kind {
    if st.is_symlink() {
        Kind::Symlink
    } else if st.is_dir() {
        Kind::Dir
    } else if st.is_file() {
        Kind::File
    } else {
        Kind::Special
    }
}

/// True when the process may not write to something owned by `uid:gid` with
/// `mode`. Supplementary groups are not consulted, so a group-writable file
/// that only a secondary group may write shows as read-only: the safe side.
pub fn is_readonly(mode: u32, uid: u32, gid: u32, euid: u32, egid: u32) -> bool {
    if euid == 0 {
        return false;
    }
    let bit = if uid == euid {
        0o200
    } else if gid == egid {
        0o020
    } else {
        0o002
    };
    mode & bit == 0
}

/// Normalises `.` and `..` lexically (no file system access).
pub fn lexical_normalize(path: &Path) -> PathBuf {
    let mut out = PathBuf::new();
    for comp in path.components() {
        match comp {
            Component::CurDir => {}
            Component::ParentDir => match out.components().next_back() {
                Some(Component::Normal(_)) => {
                    out.pop();
                }
                Some(Component::RootDir | Component::Prefix(_)) => {}
                _ => out.push(".."),
            },
            other => out.push(other.as_os_str()),
        }
    }
    out
}

impl EntryBuilder {
    pub fn new(root: PathBuf, names: Arc<Names>) -> EntryBuilder {
        let (euid, egid) = sys::effective_ids();
        EntryBuilder {
            root,
            names,
            euid,
            egid,
        }
    }

    /// Builds the entry for `name` at `path` from `st` (which may come from
    /// `lstat` or, for followed symlinks, `stat`).
    pub fn build(&self, name: &[u8], path: &Path, st: &FileStat) -> Entry {
        let kind = kind_of(st);
        let mut e = Entry::new(name, kind);
        e.modified = Some(st.mtime);
        e.created = st.btime;
        e.mode = Some(st.permissions());
        e.owner = Some(self.names.user(st.uid));
        e.group = Some(self.names.group(st.gid));
        if matches!(kind, Kind::File | Kind::Symlink) {
            e.size = Some(st.size);
        }
        if kind == Kind::Symlink {
            self.resolve_link(&mut e, path);
        } else if is_readonly(st.mode, st.uid, st.gid, self.euid, self.egid) {
            e.flags |= EntryFlags::READONLY;
        }
        e
    }

    /// Sets `target_kind` and the flags for a symlink (BRW-10): a target that
    /// cannot be read, or that does not exist outside the root (where the
    /// sandbox hides it), is "not accessible"; a dangling link inside the
    /// root is merely a link with an unknown target.
    fn resolve_link(&self, e: &mut Entry, path: &Path) {
        match sys::stat(path, true) {
            Ok(t) => e.target_kind = kind_of(&t),
            Err(err) => {
                e.target_kind = Kind::Unknown;
                e.flags |= EntryFlags::TARGET_UNKNOWN;
                if self.hidden_from_us(&err, path) {
                    e.flags |= EntryFlags::NOT_ACCESSIBLE;
                }
            }
        }
    }

    fn hidden_from_us(&self, err: &io::Error, link: &Path) -> bool {
        match err.kind() {
            io::ErrorKind::PermissionDenied => true,
            io::ErrorKind::NotFound => !self.target_inside_root(link),
            _ => false,
        }
    }

    fn target_inside_root(&self, link: &Path) -> bool {
        let Ok(target) = std::fs::read_link(link) else {
            return false;
        };
        let base = link.parent().unwrap_or(&self.root);
        let resolved = lexical_normalize(&base.join(target));
        resolved.starts_with(lexical_normalize(&self.root))
    }

    /// One `read_dir` item; `None` when it vanished meanwhile.
    pub fn entry_from_dirent(&self, item: &std::fs::DirEntry) -> Option<Entry> {
        let name = item.file_name();
        let path = item.path();
        match sys::stat(&path, false) {
            Ok(st) => Some(self.build(name.as_bytes(), &path, &st)),
            Err(e) if e.kind() == io::ErrorKind::NotFound => None,
            Err(_) => Some(Entry::new(name.as_bytes(), dirent_kind(item))),
        }
    }

    /// `stat`/`lstat` of one path as an entry named `name`.
    pub fn stat_entry(&self, path: &Path, name: &[u8], follow: bool) -> io::Result<Entry> {
        let lst = sys::stat(path, false)?;
        if follow && lst.is_symlink() {
            if let Ok(target) = sys::stat(path, true) {
                return Ok(self.build(name, path, &target));
            }
        }
        Ok(self.build(name, path, &lst))
    }
}

fn dirent_kind(item: &std::fs::DirEntry) -> Kind {
    match item.file_type() {
        Ok(t) if t.is_symlink() => Kind::Symlink,
        Ok(t) if t.is_dir() => Kind::Dir,
        Ok(t) if t.is_file() => Kind::File,
        Ok(_) => Kind::Special,
        Err(_) => Kind::Unknown,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::os::unix::fs::symlink;

    fn builder(root: &Path) -> EntryBuilder {
        EntryBuilder::new(root.to_path_buf(), Arc::new(Names::default()))
    }

    #[test]
    fn readonly_follows_the_permission_class_of_the_caller() {
        // owner, group and other each decide when they are the matching class
        assert!(is_readonly(0o444, 1000, 1000, 1000, 1000));
        assert!(!is_readonly(0o644, 1000, 1000, 1000, 1000));
        assert!(is_readonly(0o644, 0, 1000, 1000 + 1, 1000));
        assert!(!is_readonly(0o664, 0, 1000, 1001, 1000));
        assert!(is_readonly(0o644, 0, 0, 1001, 1001));
        assert!(!is_readonly(0o666, 0, 0, 1001, 1001));
        // the owner class wins even if the group would allow
        assert!(is_readonly(0o464, 1000, 1000, 1000, 1000));
        // root can write anything
        assert!(!is_readonly(0o000, 5, 5, 0, 0));
    }

    #[test]
    fn lexical_normalisation() {
        assert_eq!(lexical_normalize(Path::new("/a/b/../c/./d")), Path::new("/a/c/d"));
        assert_eq!(lexical_normalize(Path::new("/a/../../b")), Path::new("/b"));
        assert_eq!(lexical_normalize(Path::new("a/../../b")), Path::new("../b"));
    }

    #[test]
    fn builds_file_entry_with_all_attributes() {
        let d = tempfile::tempdir().unwrap();
        let f = d.path().join(".secret.txt");
        std::fs::write(&f, "hello").unwrap();
        std::fs::set_permissions(&f, std::os::unix::fs::PermissionsExt::from_mode(0o640)).unwrap();
        let b = builder(d.path());
        let e = b.stat_entry(&f, b".secret.txt", false).unwrap();
        assert_eq!(e.kind, Kind::File);
        assert_eq!(e.size, Some(5));
        assert!(e.is_hidden());
        assert_eq!(e.mode, Some(0o640));
        assert!(e.modified.is_some());
        assert!(e.owner.is_some() && e.group.is_some());
        assert_eq!(
            e.owner.as_deref(),
            Some(Names::default().user(sys::stat(&f, true).unwrap().uid).as_str())
        );
    }

    #[test]
    fn dirs_have_no_size() {
        let d = tempfile::tempdir().unwrap();
        let e = builder(d.path()).stat_entry(d.path(), b"x", false).unwrap();
        assert_eq!(e.kind, Kind::Dir);
        assert_eq!(e.size, None);
    }

    #[test]
    fn symlink_to_dir_reports_target_kind() {
        let d = tempfile::tempdir().unwrap();
        std::fs::create_dir(d.path().join("sub")).unwrap();
        symlink("sub", d.path().join("l")).unwrap();
        let b = builder(d.path());
        let e = b.stat_entry(&d.path().join("l"), b"l", false).unwrap();
        assert_eq!(e.kind, Kind::Symlink);
        assert_eq!(e.target_kind, Kind::Dir);
        assert!(e.is_dir());
        assert!(!e.flags.contains(EntryFlags::NOT_ACCESSIBLE));
        let followed = b.stat_entry(&d.path().join("l"), b"l", true).unwrap();
        assert_eq!(followed.kind, Kind::Dir);
        assert_eq!(followed.name, b"l");
    }

    #[test]
    fn dangling_link_inside_root_is_not_flagged_inaccessible() {
        let d = tempfile::tempdir().unwrap();
        symlink("gone", d.path().join("l")).unwrap();
        symlink("sub/../gone2", d.path().join("l2")).unwrap();
        let b = builder(d.path());
        for name in ["l", "l2"] {
            let e = b
                .stat_entry(&d.path().join(name), name.as_bytes(), false)
                .unwrap();
            assert_eq!(e.target_kind, Kind::Unknown);
            assert!(e.flags.contains(EntryFlags::TARGET_UNKNOWN));
            assert!(!e.flags.contains(EntryFlags::NOT_ACCESSIBLE), "{name}");
        }
    }

    #[test]
    fn link_to_unreadable_outside_target_is_not_accessible() {
        let d = tempfile::tempdir().unwrap();
        symlink("/nonexistent-outside-sandbox/file", d.path().join("abs")).unwrap();
        symlink("../../../outside-nothing", d.path().join("rel")).unwrap();
        let b = builder(d.path());
        for name in ["abs", "rel"] {
            let e = b
                .stat_entry(&d.path().join(name), name.as_bytes(), false)
                .unwrap();
            assert_eq!(e.kind, Kind::Symlink, "{name}");
            assert_eq!(e.target_kind, Kind::Unknown, "{name}");
            assert!(e.flags.contains(EntryFlags::NOT_ACCESSIBLE), "{name}");
            assert!(!e.is_dir());
        }
        // following a link that cannot be resolved falls back to the link entry
        let e = b.stat_entry(&d.path().join("abs"), b"abs", true).unwrap();
        assert_eq!(e.kind, Kind::Symlink);
    }

    #[test]
    fn readable_link_to_outside_target_is_fine() {
        let outside = tempfile::tempdir().unwrap();
        std::fs::write(outside.path().join("t"), "x").unwrap();
        let d = tempfile::tempdir().unwrap();
        symlink(outside.path().join("t"), d.path().join("l")).unwrap();
        let e = builder(d.path())
            .stat_entry(&d.path().join("l"), b"l", false)
            .unwrap();
        assert_eq!(e.target_kind, Kind::File);
        assert!(!e.flags.contains(EntryFlags::NOT_ACCESSIBLE));
    }

    #[test]
    fn special_files_are_special() {
        let b = builder(Path::new("/"));
        let e = b.stat_entry(Path::new("/dev/null"), b"null", false).unwrap();
        assert_eq!(e.kind, Kind::Special);
        assert_eq!(e.size, None);
    }

    #[test]
    fn non_utf8_names_are_flagged() {
        let d = tempfile::tempdir().unwrap();
        let name = b"caf\xe9.txt";
        let p = d.path().join(std::ffi::OsStr::from_bytes(name));
        std::fs::write(p, "x").unwrap();
        let item = std::fs::read_dir(d.path()).unwrap().next().unwrap().unwrap();
        let e = builder(d.path()).entry_from_dirent(&item).unwrap();
        assert_eq!(e.name, name);
        assert!(e.flags.contains(EntryFlags::NAME_NOT_UTF8));
        assert!(e.display_name().contains('\u{fffd}'));
    }

    #[test]
    fn vanished_dirent_is_skipped() {
        let d = tempfile::tempdir().unwrap();
        std::fs::write(d.path().join("f"), "x").unwrap();
        let item = std::fs::read_dir(d.path()).unwrap().next().unwrap().unwrap();
        std::fs::remove_file(d.path().join("f")).unwrap();
        assert!(builder(d.path()).entry_from_dirent(&item).is_none());
    }

    #[test]
    fn dirent_kind_mapping() {
        let d = tempfile::tempdir().unwrap();
        std::fs::create_dir(d.path().join("dir")).unwrap();
        std::fs::write(d.path().join("file"), "").unwrap();
        symlink("file", d.path().join("link")).unwrap();
        for item in std::fs::read_dir(d.path()).unwrap() {
            let item = item.unwrap();
            let want = match item.file_name().to_str().unwrap() {
                "dir" => Kind::Dir,
                "file" => Kind::File,
                _ => Kind::Symlink,
            };
            assert_eq!(dirent_kind(&item), want);
        }
    }

    #[test]
    fn readonly_flag_uses_real_permissions() {
        use std::os::unix::fs::PermissionsExt;
        let d = tempfile::tempdir().unwrap();
        let f = d.path().join("f");
        std::fs::write(&f, "x").unwrap();
        std::fs::set_permissions(&f, std::fs::Permissions::from_mode(0o444)).unwrap();
        let e = builder(d.path()).stat_entry(&f, b"f", false).unwrap();
        let root = sys::effective_ids().0 == 0;
        assert_eq!(e.flags.contains(EntryFlags::READONLY), !root);
    }
}
