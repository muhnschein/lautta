// SPDX-License-Identifier: LGPL-2.1-or-later
use super::*;
use std::ffi::OsStr;
use std::os::unix::fs::symlink;
use std::time::Duration;

struct Env {
    root: tempfile::TempDir,
    /// The root of a location ("Documents").
    loc: PathBuf,
    trash: Trash,
}

fn env() -> Env {
    let root = tempfile::tempdir().unwrap();
    let loc = root.path().join("home/Documents");
    std::fs::create_dir_all(&loc).unwrap();
    let trash = Trash::new(root.path().join("data/trash"), Db::open_in_memory().unwrap());
    Env { root, loc, trash }
}

/// The same setup twice: renames work, and as in the sandbox.
fn both() -> [Env; 2] {
    let plain = env();
    let mut sandboxed = env();
    sandboxed.trash = sandboxed.trash.clone().across_mounts();
    [plain, sandboxed]
}

impl Trash {
    /// As in the sandbox (§3.3): every rename between two folders fails
    /// with `EXDEV`, as between two mounts.
    fn across_mounts(self) -> Trash {
        Trash {
            rename: |_, _| Err(Error::kind(ErrorKind::CrossesDevice)),
            ..self
        }
    }
}

impl Env {
    fn put(&self, name: &str, data: &str) -> PathBuf {
        let f = self.loc.join(name);
        std::fs::create_dir_all(f.parent().unwrap()).unwrap();
        std::fs::write(&f, data).unwrap();
        f
    }

    fn trash(&self, path: &Path, name: &str) -> Result<TrashItem> {
        self.trash.trash(path, &uri(name))
    }

    /// The same trash as it works in the sandbox.
    fn sandboxed(&self) -> Trash {
        self.trash.clone().across_mounts()
    }

    fn resolve(&self) -> impl Fn(&Uri) -> Option<PathBuf> + '_ {
        |u: &Uri| Some(self.loc.join(OsStr::from_bytes(u.path.as_bytes())))
    }

    /// A row as the bin-keeping builds (schemas 4 to 6) wrote it: it names
    /// a bin in a location, though the code does not use that column now.
    fn row_with_a_bin(&self, stored: &str, name: &str) -> i64 {
        let conn = self.trash.db.lock();
        conn.execute(
            "INSERT INTO trash_items(original_uri, trashed_at, stored_name, is_dir, size, bin) \
             VALUES (?1, 1000, ?2, 0, 1, ?3)",
            params![
                uri(name).to_string(),
                stored,
                self.loc.join(".lautta-trash").as_os_str().as_bytes()
            ],
        )
        .unwrap();
        conn.last_insert_rowid()
    }
}

fn uri(path: &str) -> Uri {
    Uri::parse(&format!("lautta://user-documents/{path}")).unwrap()
}

fn count(dir: &Path) -> usize {
    std::fs::read_dir(dir).unwrap().count()
}

fn names(dir: &Path) -> Vec<String> {
    let mut v: Vec<String> = std::fs::read_dir(dir)
        .unwrap()
        .map(|e| e.unwrap().file_name().to_string_lossy().into_owned())
        .collect();
    v.sort();
    v
}

fn mode(path: &Path) -> u32 {
    std::fs::symlink_metadata(path).unwrap().permissions().mode() & 0o7777
}

fn mtime(path: &Path) -> SystemTime {
    std::fs::symlink_metadata(path).unwrap().modified().unwrap()
}

fn is_root() -> bool {
    sys::effective_ids().0 == 0
}

/// A folder tree with what a move must keep: content, modes, times, links.
fn project(dir: &Path) -> SystemTime {
    std::fs::create_dir_all(dir.join("sub")).unwrap();
    std::fs::write(dir.join("sub/f"), "content").unwrap();
    std::fs::write(dir.join("one"), "123").unwrap();
    std::fs::set_permissions(dir.join("sub/f"), std::fs::Permissions::from_mode(0o640)).unwrap();
    std::fs::set_permissions(dir.join("sub"), std::fs::Permissions::from_mode(0o750)).unwrap();
    symlink("sub/f", dir.join("link")).unwrap();
    let when = UNIX_EPOCH + Duration::from_secs(1_000_000_000);
    sys::set_mtime(&dir.join("sub/f"), when).unwrap();
    sys::set_mtime(&dir.join("sub"), when + Duration::from_secs(60)).unwrap();
    sys::set_mtime(dir, when + Duration::from_secs(120)).unwrap();
    when
}

/// What [`project`] made, checked at `dir`.
fn assert_project(dir: &Path, when: SystemTime) {
    assert_eq!(std::fs::read(dir.join("sub/f")).unwrap(), b"content");
    assert_eq!(std::fs::read(dir.join("one")).unwrap(), b"123");
    assert_eq!(mode(&dir.join("sub/f")), 0o640);
    assert_eq!(mode(&dir.join("sub")), 0o750);
    assert_eq!(mtime(&dir.join("sub/f")), when);
    assert_eq!(mtime(&dir.join("sub")), when + Duration::from_secs(60));
    assert_eq!(
        mtime(dir),
        when + Duration::from_secs(120),
        "folder times are kept"
    );
    assert_eq!(std::fs::read_link(dir.join("link")).unwrap(), Path::new("sub/f"));
}

/// A named pipe: a kind of file that is not copied.
fn fifo(path: &Path) {
    rustix::fs::mknodat(
        rustix::fs::CWD,
        path,
        rustix::fs::FileType::Fifo,
        rustix::fs::Mode::from_raw_mode(0o600),
        0,
    )
    .unwrap();
}

/// A temp folder on another file system than `near`, if the host has one.
fn other_device(near: &Path) -> Option<tempfile::TempDir> {
    let dir = tempfile::tempdir_in("/dev/shm").ok()?;
    (!sys::same_device(dir.path(), near).unwrap_or(true)).then_some(dir)
}

// ------------------------------------------------------------ trashing

#[test]
fn trashing_moves_the_item_into_the_trash_folder() {
    let e = env();
    let f = e.put("report.txt", "twelve bytes");
    let item = e.trash(&f, "report.txt").unwrap();
    assert!(!f.exists());
    assert_eq!(item.dir, e.trash.dir());
    assert_eq!(item.stored_path(), e.trash.dir().join(&item.stored_name));
    assert_eq!(std::fs::read(item.stored_path()).unwrap(), b"twelve bytes");
    assert_eq!(item.original_uri, uri("report.txt"));
    assert_eq!(item.size, Some(12));
    assert!(!item.is_dir);
    assert!((now_secs() - item.trashed_at).abs() < 60);
    assert_eq!(e.trash.list().unwrap(), vec![item.clone()]);
    assert_eq!(e.trash.get(item.id).unwrap(), item);
    assert_eq!(mode(e.trash.dir()), 0o700, "the trash folder is private (SEC-5)");
    assert_eq!(
        names(&e.loc),
        Vec::<String>::new(),
        "nothing is left in the location"
    );
}

#[test]
fn trashing_across_mounts_copies_the_item_then_removes_it() {
    let e = env();
    let trash = e.sandboxed();
    let d = e.loc.join("Project");
    let when = project(&d);
    let item = trash.trash(&d, &uri("Project")).unwrap();
    assert!(!d.exists(), "moved, not copied");
    assert!(item.is_dir);
    assert_eq!(
        item.size,
        Some(10 + "sub/f".len() as u64),
        "file bytes and the link's own size"
    );
    assert_project(&item.stored_path(), when);
    assert_eq!(
        names(trash.dir()),
        [item.stored_name.as_str()],
        "no temporary copy is left"
    );
    assert_eq!(trash.list().unwrap(), vec![item]);
}

#[test]
fn trashing_across_mounts_keeps_a_link_a_link() {
    let e = env();
    let trash = e.sandboxed();
    let target = e.loc.join("target-dir");
    std::fs::create_dir(&target).unwrap();
    std::fs::write(target.join("keep"), "x").unwrap();
    let link = e.loc.join("link");
    symlink(&target, &link).unwrap();
    let item = trash.trash(&link, &uri("link")).unwrap();
    assert!(!item.is_dir, "a link to a folder is not a folder");
    assert!(target.join("keep").exists(), "the link's target stays");
    assert!(std::fs::symlink_metadata(&link).is_err());
    assert_eq!(std::fs::read_link(item.stored_path()).unwrap(), target);
}

#[test]
fn a_failed_copy_leaves_the_original_and_no_partial_copy() {
    let e = env();
    let trash = e.sandboxed();
    let d = e.loc.join("d");
    let when = project(&d);
    std::fs::create_dir(d.join("z")).unwrap();
    std::fs::write(d.join("z/a"), "a").unwrap();
    fifo(&d.join("z/pipe"));
    sys::set_mtime(&d, when + Duration::from_secs(120)).unwrap();
    let err = trash.trash(&d, &uri("d")).unwrap_err();
    assert_eq!(err.kind, ErrorKind::Unsupported, "special files are not copied");
    assert_project(&d, when);
    assert!(d.join("z/pipe").exists() && d.join("z/a").exists());
    assert_eq!(count(trash.dir()), 0, "the partial copy is removed");
    assert!(trash.list().unwrap().is_empty());
}

#[test]
fn an_item_that_could_not_be_removed_after_the_copy_is_not_copied() {
    if is_root() {
        // Root may remove anything: nothing to refuse.
        return;
    }
    let e = env();
    let trash = e.sandboxed();
    // A read-only folder with something in it: its content can't be removed.
    let d = e.loc.join("d");
    std::fs::create_dir_all(d.join("ro")).unwrap();
    std::fs::write(d.join("ro/f"), "x").unwrap();
    std::fs::set_permissions(d.join("ro"), std::fs::Permissions::from_mode(0o555)).unwrap();
    let err = trash.trash(&d, &uri("d")).unwrap_err();
    assert_eq!(err.kind, ErrorKind::PermissionDenied);
    assert_eq!(std::fs::read(d.join("ro/f")).unwrap(), b"x");
    assert_eq!(count(trash.dir()), 0);
    // A file in a read-only folder: the file can't be removed.
    let f = d.join("ro/f");
    assert_eq!(
        trash.trash(&f, &uri("d/ro/f")).unwrap_err().kind,
        ErrorKind::PermissionDenied
    );
    assert!(f.exists());
    // An empty read-only folder can go.
    std::fs::create_dir(d.join("empty")).unwrap();
    std::fs::set_permissions(d.join("empty"), std::fs::Permissions::from_mode(0o555)).unwrap();
    trash.trash(&d.join("empty"), &uri("d/empty")).unwrap();
    std::fs::set_permissions(d.join("ro"), std::fs::Permissions::from_mode(0o755)).unwrap();
    assert_eq!(trash.list().unwrap().len(), 1);
}

#[test]
fn same_names_get_distinct_stored_names() {
    let e = env();
    let mut names = Vec::new();
    for i in 0..5 {
        let f = e.put("same.txt", &format!("v{i}"));
        names.push(e.trash(&f, "same.txt").unwrap().stored_name);
    }
    names.sort();
    names.dedup();
    assert_eq!(names.len(), 5);
    assert_eq!(e.trash.list().unwrap().len(), 5);
}

#[test]
fn non_utf8_names_are_fine() {
    let e = env();
    let f = e.loc.join(OsStr::from_bytes(b"\xff\xfe"));
    std::fs::write(&f, "x").unwrap();
    let u = Uri::new("user-documents", crate::vpath::VPath::parse(b"\xff\xfe").unwrap());
    let item = e.sandboxed().trash(&f, &u).unwrap();
    assert_eq!(e.trash.list().unwrap()[0].original_uri, u);
    assert_eq!(e.trash.restore(item.id, &e.resolve()).unwrap(), f);
    assert!(f.exists());
}

#[test]
fn missing_source_is_not_found_and_leaves_no_row() {
    let e = env();
    let err = e.trash(&e.loc.join("ghost"), "ghost").unwrap_err();
    assert_eq!(err.kind, ErrorKind::NotFound);
    assert!(e.trash.list().unwrap().is_empty());
}

#[test]
fn the_trash_cannot_swallow_itself() {
    let e = env();
    e.trash(&e.put("first", "x"), "first").unwrap();
    let inside = e.trash.dir().join("x");
    std::fs::write(&inside, "x").unwrap();
    for p in [
        e.trash.dir().to_path_buf(),
        inside,
        e.trash.dir().parent().unwrap().to_path_buf(),
    ] {
        let err = e.trash(&p, "x").unwrap_err();
        assert_eq!(err.kind, ErrorKind::InvalidArgument, "{p:?}");
        assert!(p.exists());
    }
    assert_eq!(e.trash.list().unwrap().len(), 1);
}

#[test]
fn row_failure_puts_the_item_back() {
    for e in both() {
        e.trash
            .db
            .lock()
            .execute_batch(
                "CREATE TRIGGER refuse BEFORE INSERT ON trash_items BEGIN SELECT RAISE(ABORT, 'full'); END;",
            )
            .unwrap();
        let f = e.put("f", "x");
        assert!(e.trash(&f, "f").is_err());
        assert_eq!(std::fs::read(&f).unwrap(), b"x");
        assert_eq!(count(e.trash.dir()), 0, "no copy without a row");
    }
}

#[test]
fn list_is_newest_first() {
    let e = env();
    let mut ids = Vec::new();
    for (i, name) in ["a", "b", "c"].iter().enumerate() {
        let item = e.trash(&e.put(name, "x"), name).unwrap();
        e.trash
            .db
            .lock()
            .execute(
                "UPDATE trash_items SET trashed_at = ?1 WHERE id = ?2",
                params![1000 + i as i64, item.id],
            )
            .unwrap();
        ids.push(item.id);
    }
    let listed: Vec<i64> = e.trash.list().unwrap().iter().map(|i| i.id).collect();
    assert_eq!(listed, [ids[2], ids[1], ids[0]]);
}

#[test]
fn unreadable_rows_are_skipped_in_the_listing() {
    let e = env();
    e.trash(&e.put("f", "x"), "f").unwrap();
    e.trash
        .db
        .lock()
        .execute(
            "INSERT INTO trash_items(original_uri, trashed_at, stored_name, is_dir) VALUES ('not a uri', 1, 'zz', 0)",
            [],
        )
        .unwrap();
    assert_eq!(e.trash.list().unwrap().len(), 1);
}

// ------------------------------------------------------------ restore

#[test]
fn restore_puts_the_item_back() {
    let e = env();
    let f = e.put("doc.txt", "precious");
    let item = e.trash(&f, "doc.txt").unwrap();
    let dest = e.trash.restore(item.id, &e.resolve()).unwrap();
    assert_eq!(dest, f);
    assert_eq!(std::fs::read(&f).unwrap(), b"precious");
    assert!(e.trash.list().unwrap().is_empty());
    assert_eq!(count(e.trash.dir()), 0);
}

#[test]
fn restore_across_mounts_copies_the_tree_back_then_removes_it() {
    let e = env();
    let trash = e.sandboxed();
    let d = e.loc.join("Project");
    let when = project(&d);
    let item = trash.trash(&d, &uri("Project")).unwrap();
    assert_eq!(trash.restore(item.id, &e.resolve()).unwrap(), d);
    assert_project(&d, when);
    assert!(!item.stored_path().exists(), "moved, not copied");
    assert!(trash.list().unwrap().is_empty());
    assert_eq!(names(&e.loc), ["Project"], "no temporary copy is left");
}

#[test]
fn restore_never_overwrites() {
    for e in both() {
        let f = e.put("doc.txt", "old");
        let item = e.trash(&f, "doc.txt").unwrap();
        std::fs::write(&f, "new occupant").unwrap();
        let err = e.trash.restore_to(item.id, &f).unwrap_err();
        assert_eq!(err.kind, ErrorKind::AlreadyExists);
        assert_eq!(std::fs::read(&f).unwrap(), b"new occupant");
        assert_eq!(e.trash.list().unwrap().len(), 1, "still in the trash");
        assert!(item.stored_path().exists());
        let other = e.loc.join("doc 2.txt");
        e.trash.restore_to(item.id, &other).unwrap();
        assert_eq!(std::fs::read(&other).unwrap(), b"old");
        assert_eq!(names(&e.loc), ["doc 2.txt", "doc.txt"]);
    }
}

#[test]
fn restore_recreates_missing_folders_and_reports_missing_locations() {
    for e in both() {
        let f = e.put("gone-dir/deeper/f", "x");
        let item = e.trash(&f, "gone-dir/deeper/f").unwrap();
        std::fs::remove_dir_all(e.loc.join("gone-dir")).unwrap();
        let none = e.trash.restore(item.id, &|_| None).unwrap_err();
        assert_eq!(none.kind, ErrorKind::NotFound);
        assert_eq!(e.trash.list().unwrap().len(), 1);
        assert_eq!(e.trash.restore(item.id, &e.resolve()).unwrap(), f);
        assert_eq!(std::fs::read(&f).unwrap(), b"x");
    }
}

#[test]
fn a_failed_restore_copy_keeps_the_item_in_the_trash() {
    let e = env();
    let trash = e.sandboxed();
    let d = e.loc.join("d");
    std::fs::create_dir(&d).unwrap();
    std::fs::write(d.join("a"), "a").unwrap();
    let item = trash.trash(&d, &uri("d")).unwrap();
    fifo(&item.stored_path().join("pipe"));
    let err = trash.restore(item.id, &e.resolve()).unwrap_err();
    assert_eq!(err.kind, ErrorKind::Unsupported);
    assert_eq!(count(&e.loc), 0, "the failed copy is cleaned up");
    assert_eq!(trash.list().unwrap(), vec![item.clone()]);
    assert_eq!(std::fs::read(item.stored_path().join("a")).unwrap(), b"a");
}

#[test]
fn restore_of_a_vanished_item_drops_the_stale_row() {
    let e = env();
    let f = e.put("f", "x");
    let item = e.trash(&f, "f").unwrap();
    std::fs::remove_file(item.stored_path()).unwrap();
    let err = e.trash.restore_to(item.id, &f).unwrap_err();
    assert_eq!(err.kind, ErrorKind::NotFound);
    assert!(e.trash.list().unwrap().is_empty());
    assert_eq!(e.trash.get(item.id).unwrap_err().kind, ErrorKind::NotFound);
}

#[test]
fn restore_across_real_mounts() {
    // Sailjail: the app data folder and ~/Documents are different mounts,
    // so a rename between them fails with EXDEV (tests/sandbox.rs too).
    let e = env();
    let Some(shm) = other_device(&e.loc) else {
        return;
    };
    let trash = Trash::new(shm.path().join("trash"), e.trash.db.clone());
    let d = e.loc.join("Project");
    let when = project(&d);
    let item = trash.trash(&d, &uri("Project")).unwrap();
    assert!(!d.exists());
    assert_project(&item.stored_path(), when);
    trash.restore(item.id, &e.resolve()).unwrap();
    assert_project(&d, when);
    assert_eq!(count(trash.dir()), 0);
}

// ------------------------------------------------------------ delete, empty, purge

#[test]
fn delete_removes_content_and_row() {
    let e = env();
    let d = e.loc.join("d");
    std::fs::create_dir_all(d.join("sub")).unwrap();
    std::fs::write(d.join("sub/f"), "x").unwrap();
    let item = e.trash(&d, "d").unwrap();
    e.trash.delete(item.id).unwrap();
    assert_eq!(count(e.trash.dir()), 0);
    assert!(e.trash.list().unwrap().is_empty());
    assert_eq!(e.trash.delete(item.id).unwrap_err().kind, ErrorKind::NotFound);
}

#[test]
fn deleting_never_follows_symlinks_out_of_the_trash() {
    let e = env();
    let outside = e.loc.join("precious");
    std::fs::create_dir(&outside).unwrap();
    std::fs::write(outside.join("file"), "keep me").unwrap();
    let d = e.loc.join("d");
    std::fs::create_dir(&d).unwrap();
    symlink(&outside, d.join("escape")).unwrap();
    symlink(outside.join("file"), d.join("filelink")).unwrap();
    let item = e.sandboxed().trash(&d, &uri("d")).unwrap();
    e.trash.delete(item.id).unwrap();
    assert_eq!(std::fs::read(outside.join("file")).unwrap(), b"keep me");
    assert!(!item.stored_path().exists());
}

#[test]
fn remove_tree_handles_all_kinds() {
    let d = tempfile::tempdir().unwrap();
    let f = d.path().join("f");
    std::fs::write(&f, "x").unwrap();
    remove_tree(&f).unwrap();
    remove_tree(&f).unwrap();
    let l = d.path().join("l");
    symlink("/nonexistent", &l).unwrap();
    remove_tree(&l).unwrap();
    assert!(std::fs::symlink_metadata(&l).is_err());
    std::fs::create_dir_all(d.path().join("t/u")).unwrap();
    remove_tree(&d.path().join("t")).unwrap();
    assert!(!d.path().join("t").exists());
}

#[test]
fn empty_clears_items_and_strays() {
    let e = env();
    e.trash(&e.put("a", "x"), "a").unwrap();
    e.sandboxed().trash(&e.put("b", "x"), &uri("b")).unwrap();
    std::fs::write(e.trash.dir().join("stray-from-a-crash"), "x").unwrap();
    std::fs::create_dir(e.trash.dir().join(format!("{PARTIAL_PREFIX}1"))).unwrap();
    assert_eq!(e.trash.empty().unwrap(), 2);
    assert!(e.trash.list().unwrap().is_empty());
    assert_eq!(count(e.trash.dir()), 0);
    assert!(e.loc.is_dir());
    assert_eq!(e.trash.empty().unwrap(), 0);
}

#[test]
fn empty_never_sweeps_through_a_planted_link() {
    let e = env();
    let elsewhere = e.root.path().join("elsewhere");
    std::fs::create_dir(&elsewhere).unwrap();
    let mine = elsewhere.join("mine.txt");
    std::fs::write(&mine, "keep").unwrap();
    std::fs::create_dir_all(e.trash.dir().parent().unwrap()).unwrap();
    symlink(&elsewhere, e.trash.dir()).unwrap();
    assert_eq!(e.trash.empty().unwrap(), 0);
    assert_eq!(std::fs::read(&mine).unwrap(), b"keep");
}

fn age(e: &Env, id: i64, days: i64) {
    e.trash
        .db
        .lock()
        .execute(
            "UPDATE trash_items SET trashed_at = ?1 WHERE id = ?2",
            params![now_secs() - days * SECS_PER_DAY, id],
        )
        .unwrap();
}

#[test]
fn purge_removes_only_expired_items() {
    let e = env();
    let old = e.trash(&e.put("old", "x"), "old").unwrap();
    let old2 = e.sandboxed().trash(&e.put("old2", "x"), &uri("old2")).unwrap();
    let borderline = e.trash(&e.put("borderline", "x"), "borderline").unwrap();
    e.trash(&e.put("fresh", "x"), "fresh").unwrap();
    for id in [old.id, old2.id] {
        age(&e, id, 31);
    }
    age(&e, borderline.id, 29);
    assert_eq!(e.trash.purge_expired().unwrap(), 2);
    let left: Vec<String> = e
        .trash
        .list()
        .unwrap()
        .iter()
        .map(|i| i.original_uri.to_string())
        .collect();
    assert_eq!(left.len(), 2);
    assert!(!left.iter().any(|u| u.contains("/old")));
    assert!(!old.stored_path().exists());
    assert!(!old2.stored_path().exists());
    assert!(borderline.stored_path().exists());
    assert_eq!(e.trash.purge_older_than(7).unwrap(), 1);
    assert_eq!(
        e.trash.purge_older_than(0).unwrap(),
        0,
        "items from this second are not older"
    );
    assert_eq!(e.trash.purge_before(now_secs() + 10).unwrap(), 1);
}

#[test]
fn total_size_adds_up() {
    let e = env();
    assert_eq!(e.trash.total_size().unwrap(), 0);
    e.trash(&e.put("f", "1234"), "f").unwrap();
    let d = e.loc.join("d");
    std::fs::create_dir(&d).unwrap();
    std::fs::write(d.join("x"), "123456").unwrap();
    e.sandboxed().trash(&d, &uri("d")).unwrap();
    assert_eq!(e.trash.total_size().unwrap(), 10);
}

// ------------------------------------------------------------ strays and old rows

#[test]
fn tidy_removes_half_made_copies_only() {
    let e = env();
    let kept = e.trash(&e.put("kept", "x"), "kept").unwrap();
    std::fs::create_dir_all(e.trash.dir().join(format!("{PARTIAL_PREFIX}1/sub"))).unwrap();
    std::fs::write(e.trash.dir().join("no-row"), "x").unwrap();
    e.trash.tidy().unwrap();
    let mut expected = vec![kept.stored_name, "no-row".to_owned()];
    expected.sort();
    assert_eq!(names(e.trash.dir()), expected, "only half-made copies go");
    e.trash.tidy().unwrap();
    assert_eq!(names(e.trash.dir()), expected, "a second pass changes nothing");
}

#[test]
fn tidy_of_a_trash_folder_that_does_not_exist_is_fine() {
    let e = env();
    e.trash.tidy().unwrap();
    assert!(!e.trash.dir().exists());
}

#[test]
fn a_row_that_names_a_bin_is_an_item_of_the_trash_folder() {
    // Nothing moves such an item into the trash folder any more: its row
    // lists as one of the trash folder and finds nothing there.
    let e = env();
    let id = e.row_with_a_bin("s-old", "old");
    let item = e.trash.get(id).unwrap();
    assert_eq!(item.dir, e.trash.dir());
    assert_eq!(item.stored_path(), e.trash.dir().join("s-old"));
    assert_eq!(e.trash.list().unwrap(), vec![item]);
    let err = e.trash.restore(id, &e.resolve()).unwrap_err();
    assert_eq!(err.kind, ErrorKind::NotFound);
    assert!(e.trash.list().unwrap().is_empty(), "the stale row went");
    let other = e.row_with_a_bin("s-old2", "old2");
    e.trash.delete(other).unwrap();
    assert!(e.trash.list().unwrap().is_empty());
}

#[test]
fn what_a_bin_left_in_a_location_is_not_touched() {
    let e = env();
    let bin = e.loc.join(".lautta-trash");
    std::fs::create_dir_all(&bin).unwrap();
    std::fs::write(bin.join("s-old"), "keep").unwrap();
    e.row_with_a_bin("s-old", "old");
    e.trash(&e.put("new", "x"), "new").unwrap();
    e.trash.tidy().unwrap();
    e.trash.empty().unwrap();
    assert_eq!(std::fs::read(bin.join("s-old")).unwrap(), b"keep");
}
