// SPDX-License-Identifier: LGPL-2.1-or-later
//! Removable volumes: SD cards and USB drives mounted below `/run/media/$USER/<label-or-uuid>`.
//!
//! No UDisks2 listener: folder scan plus an inotify watch on the media root sees the same mounts
//! and unmounts (udisks creates and removes mount point directories), needs no D-Bus dependency in
//! core, and is trivial to test. Core keeps a [`VolumeWatcher`] for its lifetime; the watch can
//! miss a mount inside the sandbox's mount namespace, so Home also rescans on refresh. The
//! sandbox mounts nothing itself: the bridge mounts and unmounts drives
//! ([`crate::app::Core::mount_drive`]).

use crate::bridge::DriveKind;
use crate::error::Result;
use crate::paths::AppPaths;
use crate::watch::{lock, map_notify_error, raw_watcher, spawn_worker};
use notify::{EventKind, RecommendedWatcher, RecursiveMode, Watcher};
use std::ffi::OsStr;
use std::os::unix::ffi::OsStrExt;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex, Weak};
use std::time::Duration;
use tokio::sync::mpsc::{unbounded_channel, UnboundedReceiver, UnboundedSender};

/// Quiet time after a mount change before the media root is rescanned
/// (udisks creates the directory a moment before the mount completes).
pub const VOLUME_DEBOUNCE: Duration = Duration::from_millis(300);

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Volume {
    pub id: String,
    /// Mount folder name: label, or UUID when none ([`is_fallback_name`]).
    pub name: String,
    pub mount_path: PathBuf,
    pub kind: DriveKind,
}

/// Shown for a volume without label.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct VolumeWords {
    pub card: String,
    pub usb: String,
}

impl Default for VolumeWords {
    fn default() -> VolumeWords {
        VolumeWords {
            card: "Memory card".to_owned(),
            usb: "USB drive".to_owned(),
        }
    }
}

impl VolumeWords {
    pub fn of(&self, kind: DriveKind) -> &str {
        match kind {
            DriveKind::Card => &self.card,
            DriveKind::Usb => &self.usb,
        }
    }

    /// Label, or word for its kind when the mount folder carries none.
    pub fn name_of(&self, v: &Volume) -> String {
        if is_fallback_name(&v.name) {
            self.of(v.kind).to_owned()
        } else {
            v.name.clone()
        }
    }
}

/// Mount folder name udisks makes for a filesystem without label: its UUID (FAT and exFAT
/// `XXXX-XXXX`, NTFS 16 hex digits, others RFC 4122), or `disk` without UUID either. Label
/// that looks the same counts as none.
pub fn is_fallback_name(name: &str) -> bool {
    let hex = |s: &str, n: usize| s.len() == n && s.bytes().all(|b| b.is_ascii_hexdigit());
    let groups: Vec<&str> = name.split('-').collect();
    name == "disk"
        || hex(name, 16)
        || matches!(groups.as_slice(), [a, b] if hex(a, 4) && hex(b, 4))
        || matches!(groups.as_slice(), [a, b, c, d, e]
            if hex(a, 8) && hex(b, 4) && hex(c, 4) && hex(d, 4) && hex(e, 12))
}

/// Kind of drive mounted on `path`, from `mountinfo` (`/proc/self/mountinfo` text): memory card
/// when its block device, or a device below it (unlocked LUKS), is `mmcblk*`. `sys` = `/sys`;
/// unreadable there, mount source's name decides. Not found: USB, as bridge counts unknown kinds.
pub fn kind_of(path: &Path, mountinfo: &str, sys: &Path) -> DriveKind {
    let Some((dev, source)) = mountinfo.lines().rev().find_map(|line| {
        let (fields, tail) = line.split_once(" - ")?;
        let mut f = fields.split(' ');
        let dev = f.nth(2)?;
        (unescape_mount(f.nth(1)?) == path.as_os_str().as_bytes())
            .then(|| (dev, tail.split(' ').nth(1).unwrap_or_default()))
    }) else {
        return DriveKind::Usb;
    };
    let block = sys.join("dev/block").join(dev);
    let mut names: Vec<Vec<u8>> = std::fs::read_dir(block.join("slaves"))
        .map(|rd| rd.flatten().map(|e| e.file_name().as_bytes().to_vec()).collect())
        .unwrap_or_default();
    if let Ok(real) = std::fs::canonicalize(&block) {
        names.extend(real.file_name().map(|n| n.as_bytes().to_vec()));
    }
    if names.is_empty() {
        names.extend(Path::new(source).file_name().map(|n| n.as_bytes().to_vec()));
    }
    if names.iter().any(|n| n.starts_with(b"mmcblk")) {
        DriveKind::Card
    } else {
        DriveKind::Usb
    }
}

/// Mount point field of `mountinfo`: space, tab, newline and backslash as `\ooo`.
fn unescape_mount(field: &str) -> Vec<u8> {
    let mut out = Vec::with_capacity(field.len());
    let mut rest = field.as_bytes();
    while let Some((&b, tail)) = rest.split_first() {
        let code = tail
            .get(..3)
            .filter(|o| b == b'\\' && o.iter().all(u8::is_ascii_digit))
            .and_then(|o| u8::from_str_radix(std::str::from_utf8(o).ok()?, 8).ok());
        match code {
            Some(c) => {
                out.push(c);
                rest = &tail[3..];
            }
            None => {
                out.push(b);
                rest = tail;
            }
        }
    }
    out
}

impl Volume {
    /// `vol-<uuid-or-dirname>`.
    pub fn location_id(&self) -> String {
        format!("vol-{}", self.id)
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum VolumeEvent {
    Appeared(Volume),
    Disappeared(Volume),
}

/// Keeps ASCII letters, digits and `-.:_`; every other byte becomes `_xx`
/// (hex), so any directory name, including non-UTF-8 ones, gives a valid
/// `lautta://` location id.
pub fn sanitize_id(name: &OsStr) -> String {
    let mut out = String::with_capacity(name.len());
    for &b in name.as_bytes() {
        if b.is_ascii_alphanumeric() || matches!(b, b'-' | b'.' | b':' | b'_') {
            out.push(char::from(b));
        } else {
            out.push_str(&format!("_{b:02x}"));
        }
    }
    out
}

/// Location id of a volume mounted on folder `name` of the media root, as [`Volume::location_id`]
/// makes it.
pub fn location_id_of(name: &OsStr) -> String {
    format!("vol-{}", sanitize_id(name))
}

/// `/run/media/$USER`, with the home folder's name standing in when `$USER` is unset.
pub fn media_root(paths: &AppPaths) -> PathBuf {
    let user = std::env::var("USER")
        .ok()
        .filter(|u| !u.is_empty())
        .unwrap_or_else(|| {
            paths
                .home
                .file_name()
                .map_or_else(|| "defaultuser".to_owned(), |n| n.to_string_lossy().into_owned())
        });
    paths.media_root(&user)
}

/// Lists the mounted volumes below `root`: readable folders, not hidden,
/// sorted by name. A missing root means no volumes.
pub fn scan(root: &Path) -> Vec<Volume> {
    let mountinfo = std::fs::read_to_string("/proc/self/mountinfo").unwrap_or_default();
    scan_with(root, &mountinfo, Path::new("/sys"))
}

fn scan_with(root: &Path, mountinfo: &str, sys: &Path) -> Vec<Volume> {
    let Ok(rd) = std::fs::read_dir(root) else {
        return Vec::new();
    };
    let mut out: Vec<Volume> = rd
        .flatten()
        .filter_map(|entry| {
            let name = entry.file_name();
            let path = entry.path();
            // follow links: some setups bind-mount through a symlink
            let is_dir = std::fs::metadata(&path).map(|m| m.is_dir()).unwrap_or(false);
            let usable = name.as_bytes().first() != Some(&b'.') && is_dir && std::fs::read_dir(&path).is_ok();
            usable.then(|| Volume {
                id: sanitize_id(&name),
                name: name.to_string_lossy().into_owned(),
                kind: kind_of(&path, mountinfo, sys),
                mount_path: path,
            })
        })
        .collect();
    out.sort_by(|a, b| a.name.cmp(&b.name).then_with(|| a.id.cmp(&b.id)));
    out
}

pub fn diff(old: &[Volume], new: &[Volume]) -> Vec<VolumeEvent> {
    let gone = old
        .iter()
        .filter(|o| !new.iter().any(|n| n.id == o.id))
        .cloned()
        .map(VolumeEvent::Disappeared);
    let added = new
        .iter()
        .filter(|n| !old.iter().any(|o| o.id == n.id))
        .cloned()
        .map(VolumeEvent::Appeared);
    gone.chain(added).collect()
}

fn nearest_existing(path: &Path) -> PathBuf {
    path.ancestors()
        .find(|p| p.is_dir())
        .map_or_else(|| PathBuf::from("/"), Path::to_path_buf)
}

pub struct VolumeWatcher {
    known: Arc<Mutex<Vec<Volume>>>,
    _watcher: Arc<Mutex<RecommendedWatcher>>,
}

struct Rescanner {
    root: PathBuf,
    known: Arc<Mutex<Vec<Volume>>>,
    tx: UnboundedSender<VolumeEvent>,
    /// Weak: dropping [`VolumeWatcher`] drops the watcher, which ends the worker.
    watcher: Weak<Mutex<RecommendedWatcher>>,
    watching: PathBuf,
}

impl Rescanner {
    /// The root may be created or removed at any time; keep the watch on it,
    /// or on its nearest existing ancestor while it is missing.
    fn retarget(&mut self) {
        let want = nearest_existing(&self.root);
        if want == self.watching {
            return;
        }
        let Some(watcher) = self.watcher.upgrade() else {
            return;
        };
        let mut w = lock(&watcher);
        let _ = w.unwatch(&self.watching);
        if w.watch(&want, RecursiveMode::NonRecursive).is_ok() {
            self.watching = want;
        }
    }

    fn rescan(&mut self) -> bool {
        self.retarget();
        let now = scan(&self.root);
        let events = {
            let mut known = lock(&self.known);
            let events = diff(&known, &now);
            *known = now;
            events
        };
        events.into_iter().all(|e| self.tx.send(e).is_ok())
    }
}

impl VolumeWatcher {
    /// Starts watching `root` (which need not exist yet). Events arrive on
    /// the receiver; the volumes present now are in [`VolumeWatcher::current`].
    pub fn start(
        root: PathBuf,
        debounce: Duration,
    ) -> Result<(VolumeWatcher, UnboundedReceiver<VolumeEvent>)> {
        let (mut watcher, raw) = raw_watcher()?;
        let watching = nearest_existing(&root);
        watcher
            .watch(&watching, RecursiveMode::NonRecursive)
            .map_err(map_notify_error)?;
        let watcher = Arc::new(Mutex::new(watcher));
        let known = Arc::new(Mutex::new(scan(&root)));
        let (tx, rx) = unbounded_channel();
        let mut rescanner = Rescanner {
            root,
            known: known.clone(),
            tx,
            watcher: Arc::downgrade(&watcher),
            watching,
        };
        spawn_worker(
            raw,
            debounce,
            |ev| {
                if matches!(ev.kind, EventKind::Access(_)) {
                    Vec::new()
                } else {
                    vec![()]
                }
            },
            move |()| rescanner.rescan(),
        );
        Ok((
            VolumeWatcher {
                known,
                _watcher: watcher,
            },
            rx,
        ))
    }

    pub fn current(&self) -> Vec<Volume> {
        lock(&self.known).clone()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const SHORT: Duration = Duration::from_millis(60);

    async fn next(rx: &mut UnboundedReceiver<VolumeEvent>) -> Option<VolumeEvent> {
        tokio::time::timeout(Duration::from_secs(30), rx.recv())
            .await
            .ok()
            .flatten()
    }

    fn vol(id: &str) -> Volume {
        Volume {
            id: id.into(),
            name: id.into(),
            mount_path: PathBuf::from("/m").join(id),
            kind: DriveKind::Usb,
        }
    }

    #[test]
    fn scan_lists_readable_non_hidden_folders_sorted() {
        let d = tempfile::tempdir().unwrap();
        std::fs::create_dir(d.path().join("SDCARD")).unwrap();
        std::fs::create_dir(d.path().join("0123-4567")).unwrap();
        std::fs::create_dir(d.path().join(".hidden")).unwrap();
        std::fs::write(d.path().join("not-a-volume"), "x").unwrap();
        std::os::unix::fs::symlink(d.path().join("SDCARD"), d.path().join("linked")).unwrap();
        std::os::unix::fs::symlink("/nonexistent", d.path().join("dangling")).unwrap();
        let vols = scan(d.path());
        let names: Vec<&str> = vols.iter().map(|v| v.name.as_str()).collect();
        assert_eq!(names, ["0123-4567", "SDCARD", "linked"]);
        assert_eq!(vols[1].mount_path, d.path().join("SDCARD"));
        assert_eq!(vols[1].location_id(), "vol-SDCARD");
    }

    #[test]
    fn scan_of_a_missing_root_is_empty() {
        let d = tempfile::tempdir().unwrap();
        assert!(scan(&d.path().join("nope")).is_empty());
    }

    #[test]
    fn ids_are_safe_for_uris_even_for_odd_names() {
        assert_eq!(sanitize_id(OsStr::new("SD-CARD_1.x:y")), "SD-CARD_1.x:y");
        assert_eq!(sanitize_id(OsStr::new("My Card")), "My_20Card");
        assert_eq!(sanitize_id(OsStr::from_bytes(b"a/b\xff")), "a_2fb_ff");
        assert_eq!(sanitize_id(OsStr::new("\u{e9}")), "_c3_a9");
        for name in ["My Card", "caf\u{e9} #1", "100%"] {
            let v = Volume {
                id: sanitize_id(OsStr::new(name)),
                name: name.into(),
                mount_path: PathBuf::new(),
                kind: DriveKind::Usb,
            };
            let u = crate::uri::Uri::parse(&format!("lautta://{}/", v.location_id())).unwrap();
            assert_eq!(u.location, v.location_id());
        }
    }

    #[test]
    fn non_utf8_directory_names_get_valid_ids() {
        let d = tempfile::tempdir().unwrap();
        std::fs::create_dir(d.path().join(OsStr::from_bytes(b"caf\xe9"))).unwrap();
        let vols = scan(d.path());
        assert_eq!(vols.len(), 1);
        assert_eq!(vols[0].id, "caf_e9");
        assert!(vols[0].name.contains('\u{fffd}'));
        assert!(vols[0].mount_path.ends_with(OsStr::from_bytes(b"caf\xe9")));
    }

    #[test]
    fn udisks_fallback_names_are_told_from_labels() {
        for name in [
            "0123-4567",
            "ABCD-ef01",
            "0123456789ABCDEF",
            "5c1e0a4f-1b2c-4d3e-8f90-a1b2c3d4e5f6",
            "disk",
        ] {
            assert!(is_fallback_name(name), "{name}");
        }
        for name in [
            "SDCARD",
            "Disk",
            "disk1",
            "0123-456",
            "0123-45678",
            "0123-456G",
            "0123-4567-89AB",
            "0123456789ABCDE",
            "0123456789ABCDEFA",
            "0123456789ABCDEG",
            "5c1e0a4f-1b2c-4d3e-8f90-a1b2c3d4e5f",
            "5c1e0a4f-1b2c-4d3e-8f9-a1b2c3d4e5f60",
            "5c1e0a4f-1b2c-4d3-e8f90-a1b2c3d4e5f6",
            "5c1e0a4-f1b2c-4d3e-8f90-a1b2c3d4e5f6",
            "5c1e0a4f-1b2-c4d3e-8f90-a1b2c3d4e5f6",
            "5c1e0a4f-1b2c-4d3e-8f90-a1b2c3d4e5fg",
            "",
        ] {
            assert!(!is_fallback_name(name), "{name}");
        }
    }

    #[test]
    fn unlabelled_volumes_are_named_by_kind() {
        let words = VolumeWords {
            card: "Card".into(),
            usb: "Stick".into(),
        };
        let mut v = vol("0123-4567");
        assert_eq!(words.name_of(&v), "Stick");
        v.kind = DriveKind::Card;
        assert_eq!(words.name_of(&v), "Card");
        assert_eq!(words.name_of(&vol("Holiday")), "Holiday");
        assert_eq!(VolumeWords::default().of(DriveKind::Card), "Memory card");
        assert_eq!(VolumeWords::default().of(DriveKind::Usb), "USB drive");
    }

    /// `/sys` with `dev/block/<dev>` links as the kernel makes them.
    fn fake_sys(devs: &[(&str, &str, &[&str])]) -> tempfile::TempDir {
        let d = tempfile::tempdir().unwrap();
        std::fs::create_dir_all(d.path().join("dev/block")).unwrap();
        for (dev, device, slaves) in devs {
            let real = d.path().join("devices").join(device);
            std::fs::create_dir_all(real.join("slaves")).unwrap();
            for s in *slaves {
                std::fs::create_dir(real.join("slaves").join(s)).unwrap();
            }
            std::os::unix::fs::symlink(&real, d.path().join("dev/block").join(dev)).unwrap();
        }
        d
    }

    fn mount_line(id: u32, dev: &str, point: &str, source: &str) -> String {
        format!("{id} 30 {dev} / {point} rw,nosuid shared:9 - vfat {source} rw,uid=100000\n")
    }

    #[test]
    fn drive_kind_comes_from_the_block_device() {
        let sys = fake_sys(&[
            ("179:97", "platform/mmc1/block/mmcblk1/mmcblk1p1", &[]),
            ("8:1", "platform/usb1/block/sda/sda1", &[]),
            ("253:0", "virtual/block/dm-0", &["mmcblk1p2"]),
            ("253:1", "virtual/block/dm-1", &["sdb1"]),
            ("7:0", "virtual/block/loop0", &[]),
        ]);
        let info = [
            mount_line(40, "179:97", "/run/media/u/0123-4567", "/dev/mmcblk1p1"),
            mount_line(41, "8:1", "/run/media/u/My\\040Stick", "/dev/sda1"),
            mount_line(42, "253:0", "/run/media/u/Vault", "/dev/mapper/luks-1"),
            mount_line(43, "253:1", "/run/media/u/Safe", "/dev/mapper/luks-2"),
            mount_line(44, "7:0", "/run/media/u/Loop", "/dev/loop0"),
        ]
        .concat();
        let kind = |p: &str| kind_of(Path::new(p), &info, sys.path());
        assert_eq!(kind("/run/media/u/0123-4567"), DriveKind::Card);
        assert_eq!(kind("/run/media/u/My Stick"), DriveKind::Usb);
        assert_eq!(kind("/run/media/u/Vault"), DriveKind::Card, "LUKS on a card");
        assert_eq!(kind("/run/media/u/Safe"), DriveKind::Usb);
        assert_eq!(kind("/run/media/u/Loop"), DriveKind::Usb);
        assert_eq!(kind("/run/media/u/Missing"), DriveKind::Usb);
    }

    #[test]
    fn drive_kind_without_sys_goes_by_mount_source() {
        let nosys = tempfile::tempdir().unwrap();
        let info = [
            mount_line(40, "179:97", "/m/a", "/dev/mmcblk1p1"),
            mount_line(41, "8:1", "/m/b", "/dev/sda1"),
        ]
        .concat();
        assert_eq!(kind_of(Path::new("/m/a"), &info, nosys.path()), DriveKind::Card);
        assert_eq!(kind_of(Path::new("/m/b"), &info, nosys.path()), DriveKind::Usb);
    }

    #[test]
    fn the_last_mount_on_a_point_counts() {
        let nosys = tempfile::tempdir().unwrap();
        let info = [
            mount_line(40, "8:1", "/m/a", "/dev/sda1"),
            mount_line(41, "179:97", "/m/a", "/dev/mmcblk1p1"),
        ]
        .concat();
        assert_eq!(kind_of(Path::new("/m/a"), &info, nosys.path()), DriveKind::Card);
        let info = [
            mount_line(40, "179:97", "/m/a", "/dev/mmcblk1p1"),
            mount_line(41, "8:1", "/m/a", "/dev/sda1"),
        ]
        .concat();
        assert_eq!(kind_of(Path::new("/m/a"), &info, nosys.path()), DriveKind::Usb);
    }

    #[test]
    fn mount_points_are_unescaped() {
        assert_eq!(unescape_mount("a\\040b\\011c\\012d\\134e"), b"a b\tc\nd\\e");
        assert_eq!(unescape_mount("\\04"), b"\\04");
        assert_eq!(unescape_mount("\\048x"), b"\\048x");
        assert_eq!(unescape_mount("\\377"), b"\xff");
        assert_eq!(unescape_mount("\\400"), b"\\400", "past a byte");
        assert_eq!(unescape_mount("\\+12"), b"\\+12");
        assert_eq!(unescape_mount("x\\"), b"x\\");
        assert_eq!(unescape_mount("plain"), b"plain");
    }

    #[test]
    fn scan_tells_cards_from_usb_drives() {
        let d = tempfile::tempdir().unwrap();
        std::fs::create_dir(d.path().join("0123-4567")).unwrap();
        std::fs::create_dir(d.path().join("STICK")).unwrap();
        let sys = fake_sys(&[("179:97", "mmc/mmcblk1p1", &[])]);
        let point = d.path().join("0123-4567");
        let info = mount_line(40, "179:97", point.to_str().unwrap(), "/dev/mmcblk1p1");
        let kinds: Vec<_> = scan_with(d.path(), &info, sys.path())
            .into_iter()
            .map(|v| (v.name, v.kind))
            .collect();
        assert_eq!(
            kinds,
            [
                ("0123-4567".to_owned(), DriveKind::Card),
                ("STICK".to_owned(), DriveKind::Usb)
            ]
        );
    }

    #[test]
    fn diff_reports_removals_then_additions() {
        let old = [vol("a"), vol("b")];
        let new = [vol("b"), vol("c")];
        assert_eq!(
            diff(&old, &new),
            [
                VolumeEvent::Disappeared(vol("a")),
                VolumeEvent::Appeared(vol("c"))
            ]
        );
        assert!(diff(&old, &old).is_empty());
        assert!(diff(&[], &[]).is_empty());
    }

    #[test]
    fn media_root_uses_the_user_name() {
        let paths = AppPaths::new("/home/someone");
        let root = media_root(&paths);
        let user = std::env::var("USER").ok().filter(|u| !u.is_empty());
        assert_eq!(
            root,
            Path::new("/run/media").join(user.unwrap_or_else(|| "someone".into()))
        );
    }

    #[test]
    fn nearest_existing_walks_up() {
        let d = tempfile::tempdir().unwrap();
        assert_eq!(nearest_existing(&d.path().join("a/b/c")), d.path());
        assert_eq!(nearest_existing(d.path()), d.path());
    }

    #[tokio::test]
    async fn mounts_and_unmounts_are_reported() {
        let d = tempfile::tempdir().unwrap();
        let root = d.path().join("media");
        std::fs::create_dir(&root).unwrap();
        std::fs::create_dir(root.join("PRESENT")).unwrap();
        let (w, mut rx) = VolumeWatcher::start(root.clone(), SHORT).unwrap();
        assert_eq!(
            w.current().iter().map(|v| v.name.as_str()).collect::<Vec<_>>(),
            ["PRESENT"]
        );
        std::fs::create_dir(root.join("CARD")).unwrap();
        let ev = next(&mut rx).await.unwrap();
        assert_eq!(
            ev,
            VolumeEvent::Appeared(Volume {
                id: "CARD".into(),
                name: "CARD".into(),
                mount_path: root.join("CARD"),
                kind: DriveKind::Usb,
            })
        );
        assert_eq!(w.current().len(), 2);
        std::fs::remove_dir(root.join("PRESENT")).unwrap();
        match next(&mut rx).await.unwrap() {
            VolumeEvent::Disappeared(v) => assert_eq!(v.id, "PRESENT"),
            other => panic!("unexpected {other:?}"),
        }
        assert_eq!(w.current().len(), 1);
    }

    #[tokio::test]
    async fn watching_starts_before_the_media_root_exists() {
        let d = tempfile::tempdir().unwrap();
        let root = d.path().join("run/media/user");
        let (w, mut rx) = VolumeWatcher::start(root.clone(), SHORT).unwrap();
        assert!(w.current().is_empty());
        std::fs::create_dir_all(root.join("SD")).unwrap();
        let mut seen = None;
        while let Some(ev) = next(&mut rx).await {
            if matches!(&ev, VolumeEvent::Appeared(v) if v.id == "SD") {
                seen = Some(ev);
                break;
            }
        }
        assert!(seen.is_some(), "volume below a late-created root must be found");
        // after the root exists the watch moved onto it: later mounts are seen
        std::fs::create_dir(root.join("USB")).unwrap();
        let ev = next(&mut rx).await.unwrap();
        assert!(matches!(ev, VolumeEvent::Appeared(v) if v.id == "USB"));
    }

    #[tokio::test]
    async fn removing_the_whole_root_reports_every_volume_gone() {
        let d = tempfile::tempdir().unwrap();
        let root = d.path().join("media");
        std::fs::create_dir_all(root.join("CARD")).unwrap();
        let (w, mut rx) = VolumeWatcher::start(root.clone(), SHORT).unwrap();
        std::fs::remove_dir_all(&root).unwrap();
        let mut gone = false;
        while let Some(ev) = next(&mut rx).await {
            if matches!(&ev, VolumeEvent::Disappeared(v) if v.id == "CARD") {
                gone = true;
                break;
            }
        }
        assert!(gone);
        assert!(w.current().is_empty());
        // and it comes back
        std::fs::create_dir_all(root.join("CARD2")).unwrap();
        let mut back = false;
        while let Some(ev) = next(&mut rx).await {
            if matches!(&ev, VolumeEvent::Appeared(v) if v.id == "CARD2") {
                back = true;
                break;
            }
        }
        assert!(back);
    }
}
