// SPDX-License-Identifier: LGPL-2.1-or-later
use super::*;
use crate::entry::{EntryFlags, Kind};
use crate::provider::{list_all, no_progress};
use std::os::unix::fs::symlink;
use std::sync::Mutex;
use std::time::{Duration, SystemTime};

fn setup() -> (tempfile::TempDir, LocalProvider) {
    let d = tempfile::tempdir().unwrap();
    let p = LocalProvider::new(d.path().to_path_buf());
    (d, p)
}

fn vp(s: &str) -> VPath {
    VPath::parse(s.as_bytes()).unwrap()
}

fn fd_of(path: &Path) -> OwnedFd {
    OwnedFd::from(File::open(path).unwrap())
}

fn names(entries: &[Entry]) -> Vec<String> {
    let mut v: Vec<String> = entries.iter().map(Entry::display_name).collect();
    v.sort();
    v
}

async fn batches(p: &LocalProvider, dir: &VPath) -> Result<Vec<Vec<Entry>>> {
    let (tx, mut rx) = mpsc::channel(1024);
    let res = p.list(dir, Lane::Interactive, tx).await;
    let mut out = Vec::new();
    while let Some(b) = rx.recv().await {
        out.push(b);
    }
    res.map(|()| out)
}

type ProgressLog = Arc<Mutex<Vec<(u64, Option<u64>)>>>;

fn recording_sink() -> (ProgressSink, ProgressLog) {
    let log = Arc::new(Mutex::new(Vec::new()));
    let l2 = log.clone();
    (Arc::new(move |d, t| l2.lock().unwrap().push((d, t))), log)
}

fn fat_caps() -> Capabilities {
    fskind::FsKind::Vfat.capabilities()
}

fn read_only_provider(root: &Path) -> LocalProvider {
    let mut caps = Capabilities::default();
    caps.raw.insert(cap::RANDOM_READ.to_owned());
    LocalProvider::new(root.to_path_buf()).with_capabilities(caps)
}

// ---- capabilities -------------------------------------------------------

#[test]
fn capabilities_come_from_the_filesystem() {
    let (_d, p) = setup();
    let c = p.capabilities();
    for f in [
        cap::WRITE,
        cap::RANDOM_READ,
        cap::CHECKSUMS,
        cap::SPACE_INFO,
        cap::SET_MTIME,
        cap::SERVER_COPY,
        cap::SYMLINKS,
    ] {
        assert!(c.has(f), "{f}");
    }
    assert!(!c.has(cap::TRASH));
    assert!(p
        .clone()
        .with_capability(cap::TRASH)
        .capabilities()
        .has(cap::TRASH));
}

#[test]
fn real_path_stays_under_root() {
    let (d, p) = setup();
    assert_eq!(p.real_path(&VPath::root()), d.path());
    assert_eq!(p.real_path(&vp("a/b")), d.path().join("a/b"));
    // `..` never survives VPath parsing, so there is no way to build an escape
    assert!(VPath::parse(b"../etc/passwd").is_err());
    assert_eq!(p.real_path(&vp("a/./b//c")), d.path().join("a/b/c"));
}

// ---- listing ------------------------------------------------------------

#[tokio::test]
async fn listing_is_batched_to_256() {
    let (d, p) = setup();
    for i in 0..600 {
        std::fs::write(d.path().join(format!("f{i:03}")), "").unwrap();
    }
    let b = batches(&p, &VPath::root()).await.unwrap();
    let sizes: Vec<usize> = b.iter().map(Vec::len).collect();
    assert_eq!(sizes, [256, 256, 88]);
    let all: Vec<Entry> = b.into_iter().flatten().collect();
    assert_eq!(names(&all).len(), 600);
}

#[tokio::test]
async fn custom_batch_size_and_empty_dir() {
    let (d, p) = setup();
    assert!(batches(&p, &VPath::root()).await.unwrap().is_empty());
    for i in 0..5 {
        std::fs::write(d.path().join(format!("f{i}")), "").unwrap();
    }
    let sizes: Vec<usize> = batches(&p.with_batch(2), &VPath::root())
        .await
        .unwrap()
        .iter()
        .map(Vec::len)
        .collect();
    assert_eq!(sizes, [2, 2, 1]);
}

#[tokio::test]
async fn listing_reports_entry_details() {
    let (d, p) = setup();
    std::fs::write(d.path().join("a.txt"), "hello").unwrap();
    std::fs::write(d.path().join(".hidden"), "").unwrap();
    std::fs::create_dir(d.path().join("sub")).unwrap();
    symlink("sub", d.path().join("to-sub")).unwrap();
    let all = list_all(&p, &VPath::root(), Lane::Interactive).await.unwrap();
    assert_eq!(names(&all), [".hidden", "a.txt", "sub", "to-sub"]);
    let get = |n: &str| all.iter().find(|e| e.display_name() == n).unwrap();
    assert_eq!(get("a.txt").kind, Kind::File);
    assert_eq!(get("a.txt").size, Some(5));
    assert!(get(".hidden").is_hidden());
    assert_eq!(get("sub").kind, Kind::Dir);
    assert_eq!(get("to-sub").kind, Kind::Symlink);
    assert!(get("to-sub").is_dir());
    assert!(get("a.txt").modified.is_some());
}

#[tokio::test]
async fn listing_inside_a_subfolder() {
    let (d, p) = setup();
    std::fs::create_dir_all(d.path().join("x/y")).unwrap();
    std::fs::write(d.path().join("x/y/inner"), "1").unwrap();
    std::fs::write(d.path().join("outer"), "1").unwrap();
    let all = list_all(&p, &vp("x/y"), Lane::Bulk).await.unwrap();
    assert_eq!(names(&all), ["inner"]);
}

#[tokio::test]
async fn non_utf8_names_survive_listing_and_stat() {
    let (d, p) = setup();
    let raw = b"caf\xe9 \xff.txt";
    std::fs::write(d.path().join(OsStr::from_bytes(raw)), "x").unwrap();
    let all = list_all(&p, &VPath::root(), Lane::Interactive).await.unwrap();
    assert_eq!(all.len(), 1);
    assert_eq!(all[0].name, raw);
    assert!(all[0].flags.contains(EntryFlags::NAME_NOT_UTF8));
    let path = VPath::parse(raw).unwrap();
    let e = p.stat(&path, false, Lane::Interactive).await.unwrap();
    assert_eq!(e.name, raw);
    assert_eq!(e.size, Some(1));
}

#[tokio::test]
async fn listing_errors() {
    let (d, p) = setup();
    std::fs::write(d.path().join("file"), "").unwrap();
    let e = batches(&p, &vp("missing")).await.unwrap_err();
    assert_eq!(e.kind, ErrorKind::NotFound);
    let e = batches(&p, &vp("file")).await.unwrap_err();
    assert_eq!(e.kind, ErrorKind::NotADirectory);
}

#[tokio::test]
async fn listing_stops_when_the_consumer_goes_away() {
    let (d, p) = setup();
    for i in 0..50 {
        std::fs::write(d.path().join(format!("f{i}")), "").unwrap();
    }
    let (tx, mut rx) = mpsc::channel(1);
    let p = p.with_batch(1);
    let task = tokio::spawn(async move { p.list(&VPath::root(), Lane::Interactive, tx).await });
    assert!(rx.recv().await.is_some());
    drop(rx);
    let res = task.await.unwrap();
    assert_eq!(res.unwrap_err().kind, ErrorKind::Canceled);
}

#[tokio::test]
async fn permission_denied_is_reported_when_not_root() {
    use std::os::unix::fs::PermissionsExt;
    if sys::effective_ids().0 == 0 {
        return; // root ignores permission bits; nothing to observe
    }
    let (d, p) = setup();
    let locked = d.path().join("locked");
    std::fs::create_dir(&locked).unwrap();
    std::fs::set_permissions(&locked, std::fs::Permissions::from_mode(0o000)).unwrap();
    let e = batches(&p, &vp("locked")).await.unwrap_err();
    std::fs::set_permissions(&locked, std::fs::Permissions::from_mode(0o755)).unwrap();
    assert_eq!(e.kind, ErrorKind::PermissionDenied);
}

#[tokio::test]
async fn listing_waits_for_a_shared_permit() {
    let sem = new_io_semaphore();
    let d = tempfile::tempdir().unwrap();
    std::fs::write(d.path().join("f"), "").unwrap();
    let p = LocalProvider::new(d.path().to_path_buf()).with_semaphore(sem.clone());
    let held = sem.clone().acquire_many_owned(IO_PERMITS as u32).await.unwrap();
    let (tx, mut rx) = mpsc::channel(4);
    let pc = p.clone();
    let task = tokio::spawn(async move { pc.list(&VPath::root(), Lane::Interactive, tx).await });
    let early = tokio::time::timeout(Duration::from_millis(150), rx.recv()).await;
    assert!(early.is_err(), "listing ran without a permit");
    drop(held);
    let batch = tokio::time::timeout(Duration::from_secs(5), rx.recv())
        .await
        .unwrap();
    assert_eq!(batch.unwrap().len(), 1);
    task.await.unwrap().unwrap();
    // the permit is returned when the listing is done
    assert_eq!(sem.available_permits(), IO_PERMITS);
}

#[tokio::test]
async fn symlinks_leaving_the_sandbox_are_flagged() {
    let (d, p) = setup();
    symlink("/nonexistent-outside/secret", d.path().join("escape")).unwrap();
    symlink("nothing-here", d.path().join("broken")).unwrap();
    let all = list_all(&p, &VPath::root(), Lane::Interactive).await.unwrap();
    let escape = all.iter().find(|e| e.name == b"escape").unwrap();
    assert!(escape.flags.contains(EntryFlags::NOT_ACCESSIBLE));
    assert_eq!(escape.target_kind, Kind::Unknown);
    assert!(!escape.is_dir());
    let broken = all.iter().find(|e| e.name == b"broken").unwrap();
    assert!(!broken.flags.contains(EntryFlags::NOT_ACCESSIBLE));
    assert!(broken.flags.contains(EntryFlags::TARGET_UNKNOWN));
}

#[tokio::test]
async fn owner_names_come_from_the_injected_tables() {
    let d = tempfile::tempdir().unwrap();
    let f = d.path().join("f");
    std::fs::write(&f, "x").unwrap();
    let st = sys::stat(&f, true).unwrap();
    let tables = tempfile::tempdir().unwrap();
    std::fs::write(
        tables.path().join("passwd"),
        format!("tester:x:{}:{}\n", st.uid, st.gid),
    )
    .unwrap();
    std::fs::write(tables.path().join("group"), format!("testers:x:{}:\n", st.gid)).unwrap();
    let names = Arc::new(Names::from_files(
        tables.path().join("passwd"),
        tables.path().join("group"),
    ));
    let p = LocalProvider::new(d.path().to_path_buf()).with_names(names);
    let e = p.stat(&vp("f"), false, Lane::Interactive).await.unwrap();
    assert_eq!(e.owner.as_deref(), Some("tester"));
    assert_eq!(e.group.as_deref(), Some("testers"));
}

// ---- stat ---------------------------------------------------------------

#[tokio::test]
async fn stat_follow_and_nofollow() {
    let (d, p) = setup();
    std::fs::write(d.path().join("t"), "12345").unwrap();
    symlink("t", d.path().join("l")).unwrap();
    let l = p.stat(&vp("l"), false, Lane::Interactive).await.unwrap();
    assert_eq!(l.kind, Kind::Symlink);
    assert_eq!(l.target_kind, Kind::File);
    let t = p.stat(&vp("l"), true, Lane::Interactive).await.unwrap();
    assert_eq!(t.kind, Kind::File);
    assert_eq!(t.size, Some(5));
    assert_eq!(t.name, b"l");
    let root = p.stat(&VPath::root(), false, Lane::Interactive).await.unwrap();
    assert_eq!(root.kind, Kind::Dir);
    let e = p.stat(&vp("nope"), false, Lane::Interactive).await.unwrap_err();
    assert_eq!(e.kind, ErrorKind::NotFound);
}

#[tokio::test]
async fn read_link_returns_raw_bytes() {
    let (d, p) = setup();
    let target = OsStr::from_bytes(b"../caf\xe9");
    symlink(target, d.path().join("l")).unwrap();
    assert_eq!(p.read_link(&vp("l")).await.unwrap(), b"../caf\xe9");
    std::fs::write(d.path().join("f"), "").unwrap();
    assert_eq!(
        p.read_link(&vp("f")).await.unwrap_err().kind,
        ErrorKind::InvalidArgument
    );
    assert_eq!(
        p.read_link(&vp("zz")).await.unwrap_err().kind,
        ErrorKind::NotFound
    );
}

// ---- create and remove --------------------------------------------------

#[tokio::test]
async fn make_dir_exclusive_and_idempotent() {
    let (d, p) = setup();
    p.make_dir(&vp("a"), true).await.unwrap();
    assert!(d.path().join("a").is_dir());
    assert_eq!(
        p.make_dir(&vp("a"), true).await.unwrap_err().kind,
        ErrorKind::AlreadyExists
    );
    p.make_dir(&vp("a"), false).await.unwrap();
    std::fs::write(d.path().join("f"), "").unwrap();
    assert_eq!(
        p.make_dir(&vp("f"), false).await.unwrap_err().kind,
        ErrorKind::AlreadyExists
    );
    assert_eq!(
        p.make_dir(&vp("x/y"), true).await.unwrap_err().kind,
        ErrorKind::NotFound
    );
}

#[tokio::test]
async fn make_file_is_exclusive() {
    let (d, p) = setup();
    p.make_file(&vp("new.txt")).await.unwrap();
    assert_eq!(std::fs::metadata(d.path().join("new.txt")).unwrap().len(), 0);
    std::fs::write(d.path().join("new.txt"), "keep").unwrap();
    assert_eq!(
        p.make_file(&vp("new.txt")).await.unwrap_err().kind,
        ErrorKind::AlreadyExists
    );
    assert_eq!(std::fs::read(d.path().join("new.txt")).unwrap(), b"keep");
    assert_eq!(
        p.make_file(&vp("nodir/f")).await.unwrap_err().kind,
        ErrorKind::NotFound
    );
}

#[tokio::test]
async fn remove_file_and_dir_errors() {
    let (d, p) = setup();
    std::fs::create_dir_all(d.path().join("dir/sub")).unwrap();
    std::fs::write(d.path().join("f"), "").unwrap();
    assert_eq!(
        p.remove_file(&vp("dir")).await.unwrap_err().kind,
        ErrorKind::IsADirectory
    );
    assert_eq!(
        p.remove_file(&VPath::root()).await.unwrap_err().kind,
        ErrorKind::IsADirectory
    );
    assert_eq!(
        p.remove_dir(&vp("dir")).await.unwrap_err().kind,
        ErrorKind::DirectoryNotEmpty
    );
    assert_eq!(
        p.remove_dir(&vp("f")).await.unwrap_err().kind,
        ErrorKind::NotADirectory
    );
    assert_eq!(
        p.remove_dir(&VPath::root()).await.unwrap_err().kind,
        ErrorKind::PermissionDenied
    );
    assert_eq!(
        p.remove_file(&vp("gone")).await.unwrap_err().kind,
        ErrorKind::NotFound
    );
    p.remove_file(&vp("f")).await.unwrap();
    p.remove_dir(&vp("dir/sub")).await.unwrap();
    p.remove_dir(&vp("dir")).await.unwrap();
    assert!(!d.path().join("dir").exists() && !d.path().join("f").exists());
}

#[tokio::test]
async fn removing_a_link_leaves_its_target() {
    let (d, p) = setup();
    std::fs::create_dir(d.path().join("real")).unwrap();
    std::fs::write(d.path().join("real/keep"), "x").unwrap();
    symlink("real", d.path().join("l")).unwrap();
    p.remove_file(&vp("l")).await.unwrap();
    assert!(d.path().join("real/keep").exists());
    assert!(!d.path().join("l").exists());
}

#[tokio::test]
async fn read_only_provider_refuses_every_write() {
    let d = tempfile::tempdir().unwrap();
    std::fs::write(d.path().join("f"), "x").unwrap();
    let p = read_only_provider(d.path());
    let ro = |r: Result<()>| assert_eq!(r.unwrap_err().kind, ErrorKind::ReadOnlyFilesystem);
    ro(p.make_dir(&vp("d"), true).await);
    ro(p.make_file(&vp("n")).await);
    ro(p.remove_file(&vp("f")).await);
    ro(p.remove_dir(&vp("f")).await);
    ro(p.rename(&vp("f"), &vp("g"), RenameMode::NoReplace).await);
    ro(p.set_attributes(&vp("f"), AttributeChanges::default()).await);
    ro(p.make_symlink(b"f", &vp("l")).await);
    ro(p.make_hardlink(&vp("f"), &vp("h")).await);
    ro(p.upload_from(
        fd_of(&d.path().join("f")),
        &vp("u"),
        WriteOptions::default(),
        no_progress(),
    )
    .await);
    ro(p.server_copy(&vp("f"), &vp("c"), CopyOptions::default()).await);
    assert!(d.path().join("f").exists());
    assert_eq!(std::fs::read_dir(d.path()).unwrap().count(), 1);
    // reading is still fine
    assert_eq!(
        p.stat(&vp("f"), false, Lane::Interactive).await.unwrap().size,
        Some(1)
    );
}

// ---- rename -------------------------------------------------------------

#[tokio::test]
async fn rename_no_replace_refuses_collision_and_keeps_both() {
    let (d, p) = setup();
    std::fs::write(d.path().join("a"), "A").unwrap();
    std::fs::write(d.path().join("b"), "B").unwrap();
    let e = p
        .rename(&vp("a"), &vp("b"), RenameMode::NoReplace)
        .await
        .unwrap_err();
    assert_eq!(e.kind, ErrorKind::AlreadyExists);
    assert_eq!(std::fs::read(d.path().join("a")).unwrap(), b"A");
    assert_eq!(std::fs::read(d.path().join("b")).unwrap(), b"B");
    p.rename(&vp("a"), &vp("c"), RenameMode::NoReplace).await.unwrap();
    assert_eq!(std::fs::read(d.path().join("c")).unwrap(), b"A");
    assert!(!d.path().join("a").exists());
}

#[tokio::test]
async fn rename_replace_overwrites() {
    let (d, p) = setup();
    std::fs::write(d.path().join("a"), "A").unwrap();
    std::fs::write(d.path().join("b"), "B").unwrap();
    p.rename(&vp("a"), &vp("b"), RenameMode::Replace).await.unwrap();
    assert_eq!(std::fs::read(d.path().join("b")).unwrap(), b"A");
    assert!(!d.path().join("a").exists());
}

#[tokio::test]
async fn rename_moves_directories_and_checks_errors() {
    let (d, p) = setup();
    std::fs::create_dir_all(d.path().join("src/inner")).unwrap();
    std::fs::write(d.path().join("src/inner/f"), "x").unwrap();
    std::fs::create_dir(d.path().join("dest")).unwrap();
    p.rename(&vp("src"), &vp("dest/moved"), RenameMode::NoReplace)
        .await
        .unwrap();
    assert!(d.path().join("dest/moved/inner/f").exists());
    let e = p
        .rename(&vp("dest"), &vp("dest/moved/inner/dest"), RenameMode::NoReplace)
        .await
        .unwrap_err();
    assert_eq!(e.kind, ErrorKind::InvalidArgument);
    let e = p
        .rename(&vp("nope"), &vp("x"), RenameMode::NoReplace)
        .await
        .unwrap_err();
    assert_eq!(e.kind, ErrorKind::NotFound);
    let e = p
        .rename(&VPath::root(), &vp("x"), RenameMode::NoReplace)
        .await
        .unwrap_err();
    assert_eq!(e.kind, ErrorKind::InvalidArgument);
}

#[tokio::test]
async fn rename_with_non_utf8_names() {
    let (d, p) = setup();
    std::fs::write(d.path().join(OsStr::from_bytes(b"caf\xe9")), "x").unwrap();
    let to = VPath::parse(b"\xff\xfe").unwrap();
    p.rename(&VPath::parse(b"caf\xe9").unwrap(), &to, RenameMode::NoReplace)
        .await
        .unwrap();
    let all = list_all(&p, &VPath::root(), Lane::Interactive).await.unwrap();
    assert_eq!(all[0].name, b"\xff\xfe");
}

fn ci_provider(root: &Path) -> LocalProvider {
    LocalProvider::new(root.to_path_buf()).with_capabilities(fat_caps())
}

#[tokio::test]
async fn case_only_rename_uses_an_intermediate_name() {
    let d = tempfile::tempdir().unwrap();
    std::fs::write(d.path().join("photo.jpg"), "data").unwrap();
    let p = ci_provider(d.path());
    p.rename(&vp("photo.jpg"), &vp("PHOTO.jpg"), RenameMode::NoReplace)
        .await
        .unwrap();
    let all = list_all(&p, &VPath::root(), Lane::Interactive).await.unwrap();
    assert_eq!(names(&all), ["PHOTO.jpg"], "no temporary name may remain");
    assert_eq!(std::fs::read(d.path().join("PHOTO.jpg")).unwrap(), b"data");
}

#[tokio::test]
async fn case_only_rename_of_a_folder_with_unicode_letters() {
    let d = tempfile::tempdir().unwrap();
    std::fs::create_dir(d.path().join("\u{e9}t\u{e9}")).unwrap();
    std::fs::write(d.path().join("\u{e9}t\u{e9}/f"), "x").unwrap();
    let p = ci_provider(d.path());
    p.rename(&vp("\u{e9}t\u{e9}"), &vp("\u{c9}T\u{c9}"), RenameMode::NoReplace)
        .await
        .unwrap();
    assert!(d.path().join("\u{c9}T\u{c9}/f").exists());
    assert!(!d.path().join("\u{e9}t\u{e9}").exists());
}

#[tokio::test]
async fn case_only_rename_does_not_clobber_a_different_file() {
    let d = tempfile::tempdir().unwrap();
    std::fs::write(d.path().join("a.txt"), "lower").unwrap();
    std::fs::write(d.path().join("A.TXT"), "upper").unwrap();
    let p = ci_provider(d.path());
    let e = p
        .rename(&vp("a.txt"), &vp("A.TXT"), RenameMode::NoReplace)
        .await
        .unwrap_err();
    assert_eq!(e.kind, ErrorKind::AlreadyExists);
    assert_eq!(std::fs::read(d.path().join("a.txt")).unwrap(), b"lower");
    assert_eq!(std::fs::read(d.path().join("A.TXT")).unwrap(), b"upper");
    p.rename(&vp("a.txt"), &vp("A.TXT"), RenameMode::Replace)
        .await
        .unwrap();
    assert_eq!(std::fs::read(d.path().join("A.TXT")).unwrap(), b"lower");
    assert!(!d.path().join("a.txt").exists());
}

#[tokio::test]
async fn case_only_rename_with_an_unrelated_name_is_a_normal_rename() {
    let d = tempfile::tempdir().unwrap();
    std::fs::write(d.path().join("a"), "x").unwrap();
    let p = ci_provider(d.path());
    p.rename(&vp("a"), &vp("b"), RenameMode::NoReplace).await.unwrap();
    assert!(d.path().join("b").exists());
}

#[test]
fn case_only_detection() {
    let p = |s: &str| PathBuf::from(s);
    assert!(is_case_only_change(&p("/r/a.txt"), &p("/r/A.TXT")));
    assert!(!is_case_only_change(&p("/r/a.txt"), &p("/r/a.txt")));
    assert!(!is_case_only_change(&p("/r/a.txt"), &p("/r/b.txt")));
    assert!(!is_case_only_change(&p("/r/a.txt"), &p("/q/A.TXT")));
    assert!(is_case_only_change(
        Path::new(OsStr::from_bytes(b"/r/\xffa")),
        Path::new(OsStr::from_bytes(b"/r/\xffA"))
    ));
}

#[test]
fn case_rename_restores_the_name_when_the_second_step_fails() {
    let d = tempfile::tempdir().unwrap();
    let from = d.path().join("a.txt");
    std::fs::write(&from, "x").unwrap();
    // A destination in a folder that does not exist makes step two fail.
    let to = d.path().join("missing/A.TXT");
    let r = rename_case_only(&from, &to, RenameMode::NoReplace);
    assert!(r.is_err());
    assert_eq!(std::fs::read(&from).unwrap(), b"x");
    assert_eq!(std::fs::read_dir(d.path()).unwrap().count(), 1);
}

#[test]
fn noreplace_fallback_behaves_like_the_syscall() {
    let d = tempfile::tempdir().unwrap();
    let (a, b, c) = (d.path().join("a"), d.path().join("b"), d.path().join("c"));
    std::fs::write(&a, "A").unwrap();
    std::fs::write(&b, "B").unwrap();
    let e = rename_noreplace_fallback(&a, &b).unwrap_err();
    assert_eq!(e.kind, ErrorKind::AlreadyExists);
    assert_eq!(std::fs::read(&a).unwrap(), b"A");
    assert_eq!(std::fs::read(&b).unwrap(), b"B");
    rename_noreplace_fallback(&a, &c).unwrap();
    assert!(!a.exists());
    assert_eq!(std::fs::read(&c).unwrap(), b"A");
    // directories take the exists-check route
    let (x, y) = (d.path().join("x"), d.path().join("y"));
    std::fs::create_dir(&x).unwrap();
    std::fs::create_dir(&y).unwrap();
    assert_eq!(
        rename_noreplace_fallback(&x, &y).unwrap_err().kind,
        ErrorKind::AlreadyExists
    );
    let z = d.path().join("z");
    rename_noreplace_fallback(&x, &z).unwrap();
    assert!(z.is_dir() && !x.exists());
    // symlinks are moved as links
    symlink("target", d.path().join("l")).unwrap();
    rename_noreplace_fallback(&d.path().join("l"), &d.path().join("l2")).unwrap();
    assert_eq!(
        std::fs::read_link(d.path().join("l2")).unwrap(),
        Path::new("target")
    );
    assert_eq!(
        rename_noreplace_fallback(&d.path().join("none"), &d.path().join("q"))
            .unwrap_err()
            .kind,
        ErrorKind::NotFound
    );
}

// ---- attributes and links -----------------------------------------------

#[tokio::test]
async fn set_attributes_changes_mode_and_mtime() {
    let (d, p) = setup();
    std::fs::write(d.path().join("f"), "x").unwrap();
    let when = SystemTime::UNIX_EPOCH + Duration::from_secs(1_500_000_000);
    p.set_attributes(
        &vp("f"),
        AttributeChanges {
            mode: Some(0o600),
            modified: Some(when),
        },
    )
    .await
    .unwrap();
    let e = p.stat(&vp("f"), false, Lane::Interactive).await.unwrap();
    assert_eq!(e.mode, Some(0o600));
    assert_eq!(e.modified, Some(when));
    p.set_attributes(
        &vp("f"),
        AttributeChanges {
            mode: None,
            modified: Some(when + Duration::from_secs(60)),
        },
    )
    .await
    .unwrap();
    let e = p.stat(&vp("f"), false, Lane::Interactive).await.unwrap();
    assert_eq!(e.mode, Some(0o600), "mode untouched when not requested");
    assert_eq!(e.modified, Some(when + Duration::from_secs(60)));
    let missing = p
        .set_attributes(
            &vp("nope"),
            AttributeChanges {
                mode: Some(0o600),
                modified: None,
            },
        )
        .await
        .unwrap_err();
    assert_eq!(missing.kind, ErrorKind::NotFound);
}

#[tokio::test]
async fn set_attributes_respects_capabilities_and_links() {
    let d = tempfile::tempdir().unwrap();
    std::fs::write(d.path().join("f"), "x").unwrap();
    symlink("f", d.path().join("l")).unwrap();
    let fat = ci_provider(d.path());
    let e = fat
        .set_attributes(
            &vp("f"),
            AttributeChanges {
                mode: Some(0o600),
                modified: None,
            },
        )
        .await
        .unwrap_err();
    assert_eq!(e.kind, ErrorKind::Unsupported);
    let p = LocalProvider::new(d.path().to_path_buf());
    let e = p
        .set_attributes(
            &vp("l"),
            AttributeChanges {
                mode: Some(0o600),
                modified: None,
            },
        )
        .await
        .unwrap_err();
    assert_eq!(e.kind, ErrorKind::Unsupported);
    let m = std::fs::metadata(d.path().join("f")).unwrap();
    assert_ne!(
        std::os::unix::fs::PermissionsExt::mode(&m.permissions()) & 0o777,
        0o600
    );
}

#[tokio::test]
async fn symlinks_and_hardlinks() {
    let (d, p) = setup();
    std::fs::write(d.path().join("f"), "content").unwrap();
    p.make_symlink(b"f", &vp("sl")).await.unwrap();
    assert_eq!(p.read_link(&vp("sl")).await.unwrap(), b"f");
    p.make_symlink(b"caf\xe9/\xff", &vp("sl2")).await.unwrap();
    assert_eq!(p.read_link(&vp("sl2")).await.unwrap(), b"caf\xe9/\xff");
    p.make_hardlink(&vp("f"), &vp("hl")).await.unwrap();
    let a = sys::stat(&d.path().join("f"), true).unwrap();
    let b = sys::stat(&d.path().join("hl"), true).unwrap();
    assert_eq!((a.dev, a.ino), (b.dev, b.ino));
    assert_eq!(a.nlink, 2);
    assert_eq!(
        p.make_symlink(b"f", &vp("sl")).await.unwrap_err().kind,
        ErrorKind::AlreadyExists
    );
    assert_eq!(
        p.make_hardlink(&vp("f"), &vp("hl")).await.unwrap_err().kind,
        ErrorKind::AlreadyExists
    );
    assert_eq!(
        p.make_hardlink(&vp("none"), &vp("h2")).await.unwrap_err().kind,
        ErrorKind::NotFound
    );
    assert_eq!(
        p.make_symlink(b"", &vp("empty")).await.unwrap_err().kind,
        ErrorKind::InvalidArgument
    );
}

#[tokio::test]
async fn fat_has_no_links() {
    let d = tempfile::tempdir().unwrap();
    std::fs::write(d.path().join("f"), "x").unwrap();
    let p = ci_provider(d.path());
    assert_eq!(
        p.make_symlink(b"f", &vp("l")).await.unwrap_err().kind,
        ErrorKind::Unsupported
    );
    assert_eq!(
        p.make_hardlink(&vp("f"), &vp("h")).await.unwrap_err().kind,
        ErrorKind::Unsupported
    );
    assert_eq!(std::fs::read_dir(d.path()).unwrap().count(), 1);
}

// ---- reading ------------------------------------------------------------

#[tokio::test]
async fn open_read_gives_random_access() {
    let (d, p) = setup();
    std::fs::write(d.path().join("f"), b"0123456789").unwrap();
    let h = p.open_read(&vp("f"), Lane::Stream).await.unwrap();
    assert_eq!(h.size(), Some(10));
    assert_eq!(h.read_at(3, 4).await.unwrap(), b"3456");
    assert_eq!(h.read_at(8, 100).await.unwrap(), b"89");
    assert!(h.read_at(10, 4).await.unwrap().is_empty());
    h.read_ahead(0, 10).await.unwrap();
    assert_eq!(
        crate::provider::read_all(h.as_ref(), 1000).await.unwrap(),
        b"0123456789"
    );
    assert_eq!(crate::provider::read_all(h.as_ref(), 4).await.unwrap(), b"0123");
}

#[tokio::test]
async fn open_read_errors() {
    let (d, p) = setup();
    std::fs::create_dir(d.path().join("dir")).unwrap();
    assert_eq!(
        p.open_read(&vp("dir"), Lane::Stream).await.err().unwrap().kind,
        ErrorKind::IsADirectory
    );
    assert_eq!(
        p.open_read(&vp("none"), Lane::Stream).await.err().unwrap().kind,
        ErrorKind::NotFound
    );
}

// ---- upload / download --------------------------------------------------

fn big(len: usize) -> Vec<u8> {
    (0..len).map(|i| (i % 249) as u8).collect()
}

#[tokio::test]
async fn upload_creates_and_reports_progress() {
    let (d, p) = setup();
    let data = big(3_000_000);
    let srcdir = tempfile::tempdir().unwrap();
    std::fs::write(srcdir.path().join("s"), &data).unwrap();
    let (sink, log) = recording_sink();
    let when = SystemTime::UNIX_EPOCH + Duration::from_secs(1_400_000_000);
    let opts = WriteOptions {
        modified: Some(when),
        mode: Some(0o640),
        ..WriteOptions::default()
    };
    p.upload_from(fd_of(&srcdir.path().join("s")), &vp("out"), opts, sink)
        .await
        .unwrap();
    assert_eq!(std::fs::read(d.path().join("out")).unwrap(), data);
    let e = p.stat(&vp("out"), false, Lane::Interactive).await.unwrap();
    assert_eq!(e.modified, Some(when));
    assert_eq!(e.mode.map(|m| m & 0o750), Some(0o640 & 0o750));
    let log = log.lock().unwrap();
    assert_eq!(log.last().map(|l| l.0), Some(3_000_000));
    assert_eq!(log.last().and_then(|l| l.1), Some(3_000_000));
}

#[tokio::test]
async fn upload_create_refuses_existing_and_truncate_replaces() {
    let (d, p) = setup();
    let src = d.path().join("src");
    std::fs::write(&src, "new").unwrap();
    std::fs::write(d.path().join("dst"), "old content").unwrap();
    let e = p
        .upload_from(fd_of(&src), &vp("dst"), WriteOptions::default(), no_progress())
        .await
        .unwrap_err();
    assert_eq!(e.kind, ErrorKind::AlreadyExists);
    assert_eq!(std::fs::read(d.path().join("dst")).unwrap(), b"old content");
    let opts = WriteOptions {
        disposition: Disposition::Truncate,
        ..WriteOptions::default()
    };
    p.upload_from(fd_of(&src), &vp("dst"), opts, no_progress())
        .await
        .unwrap();
    assert_eq!(std::fs::read(d.path().join("dst")).unwrap(), b"new");
}

#[tokio::test]
async fn upload_to_missing_folder_or_over_a_directory_fails() {
    let (d, p) = setup();
    let src = d.path().join("src");
    std::fs::write(&src, "x").unwrap();
    std::fs::create_dir(d.path().join("dir")).unwrap();
    let e = p
        .upload_from(
            fd_of(&src),
            &vp("nodir/f"),
            WriteOptions::default(),
            no_progress(),
        )
        .await
        .unwrap_err();
    assert_eq!(e.kind, ErrorKind::NotFound);
    let opts = WriteOptions {
        disposition: Disposition::Truncate,
        ..WriteOptions::default()
    };
    let e = p
        .upload_from(fd_of(&src), &vp("dir"), opts, no_progress())
        .await
        .unwrap_err();
    assert_eq!(e.kind, ErrorKind::IsADirectory);
}

#[tokio::test]
async fn upload_resume_appends_after_the_partial_prefix() {
    let (d, p) = setup();
    let data = big(10_000);
    std::fs::write(d.path().join("src"), &data).unwrap();
    // the partial file has a prefix plus junk after the resume point
    let mut partial = data[..4000].to_vec();
    partial.extend_from_slice(b"JUNKJUNK");
    std::fs::write(d.path().join("dst"), &partial).unwrap();
    let (sink, log) = recording_sink();
    let opts = WriteOptions {
        disposition: Disposition::Resume,
        offset: 4000,
        ..WriteOptions::default()
    };
    p.upload_from(fd_of(&d.path().join("src")), &vp("dst"), opts, sink)
        .await
        .unwrap();
    assert_eq!(std::fs::read(d.path().join("dst")).unwrap(), data);
    assert_eq!(log.lock().unwrap().last(), Some(&(10_000, Some(10_000))));
}

#[tokio::test]
async fn upload_resume_with_a_too_short_partial_is_rejected() {
    let (d, p) = setup();
    std::fs::write(d.path().join("src"), big(100)).unwrap();
    std::fs::write(d.path().join("dst"), "abc").unwrap();
    let opts = WriteOptions {
        disposition: Disposition::Resume,
        offset: 50,
        ..WriteOptions::default()
    };
    let e = p
        .upload_from(fd_of(&d.path().join("src")), &vp("dst"), opts, no_progress())
        .await
        .unwrap_err();
    assert_eq!(e.kind, ErrorKind::InvalidArgument);
    assert_eq!(std::fs::read(d.path().join("dst")).unwrap(), b"abc");
}

#[tokio::test]
async fn upload_from_a_pipe() {
    let (d, p) = setup();
    let (r, w) = rustix::pipe::pipe().unwrap();
    let data = big(30_000);
    std::io::Write::write_all(&mut File::from(w), &data).unwrap();
    p.upload_from(r, &vp("piped"), WriteOptions::default(), no_progress())
        .await
        .unwrap();
    assert_eq!(std::fs::read(d.path().join("piped")).unwrap(), data);
}

#[tokio::test]
async fn download_copies_into_the_given_fd() {
    let (d, p) = setup();
    let data = big(1_234_567);
    std::fs::write(d.path().join("remote"), &data).unwrap();
    let out = d.path().join("local.part");
    let dst = OwnedFd::from(File::create(&out).unwrap());
    let (sink, log) = recording_sink();
    p.download_into(&vp("remote"), dst, ReadOptions::default(), sink)
        .await
        .unwrap();
    assert_eq!(std::fs::read(&out).unwrap(), data);
    assert_eq!(log.lock().unwrap().last(), Some(&(1_234_567, Some(1_234_567))));
}

#[tokio::test]
async fn download_resumes_at_an_offset() {
    let (d, p) = setup();
    let data = big(9000);
    std::fs::write(d.path().join("remote"), &data).unwrap();
    let out = d.path().join("local.part");
    std::fs::write(&out, &data[..3000]).unwrap();
    let f = OpenOptions::new().write(true).open(&out).unwrap();
    p.download_into(
        &vp("remote"),
        OwnedFd::from(f),
        ReadOptions { offset: 3000 },
        no_progress(),
    )
    .await
    .unwrap();
    assert_eq!(std::fs::read(&out).unwrap(), data);
}

#[tokio::test]
async fn download_errors() {
    let (d, p) = setup();
    std::fs::create_dir(d.path().join("dir")).unwrap();
    let dst = || OwnedFd::from(File::create(d.path().join("o")).unwrap());
    let e = p
        .download_into(&vp("dir"), dst(), ReadOptions::default(), no_progress())
        .await
        .unwrap_err();
    assert_eq!(e.kind, ErrorKind::IsADirectory);
    let e = p
        .download_into(&vp("none"), dst(), ReadOptions::default(), no_progress())
        .await
        .unwrap_err();
    assert_eq!(e.kind, ErrorKind::NotFound);
}

#[tokio::test]
async fn download_into_a_full_device_reports_no_space() {
    let Ok(full) = OpenOptions::new().write(true).open("/dev/full") else {
        return; // no /dev/full on this host
    };
    let (d, p) = setup();
    std::fs::write(d.path().join("remote"), big(100_000)).unwrap();
    let e = p
        .download_into(
            &vp("remote"),
            OwnedFd::from(full),
            ReadOptions::default(),
            no_progress(),
        )
        .await
        .unwrap_err();
    assert_eq!(e.kind, ErrorKind::NoSpace);
}

// ---- server copy --------------------------------------------------------

#[tokio::test]
async fn server_copy_copies_content_mode_and_mtime() {
    let (d, p) = setup();
    let data = big(500_000);
    std::fs::write(d.path().join("a"), &data).unwrap();
    let when = SystemTime::UNIX_EPOCH + Duration::from_secs(1_300_000_000);
    p.set_attributes(
        &vp("a"),
        AttributeChanges {
            mode: Some(0o640),
            modified: Some(when),
        },
    )
    .await
    .unwrap();
    p.server_copy(
        &vp("a"),
        &vp("b"),
        CopyOptions {
            replace: false,
            preserve_mtime: true,
        },
    )
    .await
    .unwrap();
    assert_eq!(std::fs::read(d.path().join("b")).unwrap(), data);
    assert_eq!(std::fs::read(d.path().join("a")).unwrap(), data);
    let e = p.stat(&vp("b"), false, Lane::Interactive).await.unwrap();
    assert_eq!(e.modified, Some(when));
    assert_eq!(e.mode.map(|m| m & 0o750), Some(0o640 & 0o750));
    p.server_copy(&vp("a"), &vp("c"), CopyOptions::default())
        .await
        .unwrap();
    let c = p.stat(&vp("c"), false, Lane::Interactive).await.unwrap();
    assert_ne!(c.modified, Some(when), "mtime is only preserved on request");
}

#[tokio::test]
async fn server_copy_collisions_and_replace() {
    let (d, p) = setup();
    std::fs::write(d.path().join("a"), "A").unwrap();
    std::fs::write(d.path().join("b"), "B").unwrap();
    let e = p
        .server_copy(&vp("a"), &vp("b"), CopyOptions::default())
        .await
        .unwrap_err();
    assert_eq!(e.kind, ErrorKind::AlreadyExists);
    assert_eq!(std::fs::read(d.path().join("b")).unwrap(), b"B");
    p.server_copy(
        &vp("a"),
        &vp("b"),
        CopyOptions {
            replace: true,
            preserve_mtime: false,
        },
    )
    .await
    .unwrap();
    assert_eq!(std::fs::read(d.path().join("b")).unwrap(), b"A");
    let leftovers: Vec<_> = std::fs::read_dir(d.path()).unwrap().collect();
    assert_eq!(leftovers.len(), 2, "no temporary files remain");
}

#[tokio::test]
async fn server_copy_errors_leave_nothing_behind() {
    let (d, p) = setup();
    std::fs::create_dir(d.path().join("dir")).unwrap();
    std::fs::write(d.path().join("f"), "x").unwrap();
    let k = |r: Result<()>| r.unwrap_err().kind;
    assert_eq!(
        k(p.server_copy(&vp("dir"), &vp("d2"), CopyOptions::default()).await),
        ErrorKind::IsADirectory
    );
    assert_eq!(
        k(p.server_copy(&vp("none"), &vp("n2"), CopyOptions::default())
            .await),
        ErrorKind::NotFound
    );
    assert_eq!(
        k(p.server_copy(&vp("f"), &vp("nodir/f"), CopyOptions::default())
            .await),
        ErrorKind::NotFound
    );
    assert_eq!(
        k(p.server_copy(&vp("f"), &vp("f"), CopyOptions::default()).await),
        ErrorKind::InvalidArgument
    );
    assert_eq!(std::fs::read_dir(d.path()).unwrap().count(), 2);
}

#[tokio::test]
async fn server_copy_of_a_symlink_makes_a_symlink() {
    let (d, p) = setup();
    symlink("target", d.path().join("l")).unwrap();
    p.server_copy(&vp("l"), &vp("l2"), CopyOptions::default())
        .await
        .unwrap();
    assert_eq!(
        std::fs::read_link(d.path().join("l2")).unwrap(),
        Path::new("target")
    );
}

#[tokio::test]
async fn server_copy_refuses_special_files() {
    let (d, p) = setup();
    let fifo = d.path().join("fifo");
    rustix::fs::mknodat(
        rustix::fs::CWD,
        &fifo,
        rustix::fs::FileType::Fifo,
        rustix::fs::Mode::from_raw_mode(0o600),
        0,
    )
    .unwrap();
    let e = p
        .server_copy(&vp("fifo"), &vp("copy"), CopyOptions::default())
        .await
        .unwrap_err();
    assert_eq!(e.kind, ErrorKind::Unsupported);
}

// ---- checksum and space -------------------------------------------------

#[tokio::test]
async fn checksums_match_known_vectors() {
    let (d, p) = setup();
    std::fs::write(d.path().join("abc"), "abc").unwrap();
    let hex = |b: Vec<u8>| hex::encode(b);
    assert_eq!(
        hex(p.checksum(&vp("abc"), "sha256").await.unwrap()),
        "ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad"
    );
    assert_eq!(
        hex(p.checksum(&vp("abc"), "sha1").await.unwrap()),
        "a9993e364706816aba3e25717850c26c9cd0d89d"
    );
    assert_eq!(
        hex(p.checksum(&vp("abc"), "md5").await.unwrap()),
        "900150983cd24fb0d6963f7d28e17f72"
    );
    assert_eq!(
        hex(p.checksum(&vp("abc"), "SHA-256").await.unwrap()),
        "ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad"
    );
}

#[tokio::test]
async fn checksum_of_a_multi_chunk_file() {
    use sha2::Digest;
    let (d, p) = setup();
    let data = big(2_500_000);
    std::fs::write(d.path().join("big"), &data).unwrap();
    assert_eq!(
        p.checksum(&vp("big"), "sha256").await.unwrap(),
        sha2::Sha256::digest(&data).to_vec()
    );
}

#[tokio::test]
async fn checksum_errors() {
    let (d, p) = setup();
    std::fs::create_dir(d.path().join("dir")).unwrap();
    std::fs::write(d.path().join("f"), "x").unwrap();
    assert_eq!(
        p.checksum(&vp("f"), "crc32").await.unwrap_err().kind,
        ErrorKind::Unsupported
    );
    assert_eq!(
        p.checksum(&vp("dir"), "md5").await.unwrap_err().kind,
        ErrorKind::IsADirectory
    );
    assert_eq!(
        p.checksum(&vp("none"), "md5").await.unwrap_err().kind,
        ErrorKind::NotFound
    );
}

#[tokio::test]
async fn space_reports_free_and_total() {
    let (_d, p) = setup();
    let s = p.space(&VPath::root()).await.unwrap();
    assert!(s.total > 0 && s.free <= s.total && s.used <= s.total);
    assert_eq!(
        p.space(&vp("missing")).await.unwrap_err().kind,
        ErrorKind::NotFound
    );
}

#[tokio::test]
async fn caller_file_offsets_are_ignored() {
    // netvfs XB-11: regular files are read and written at explicit offsets,
    // so an fd left at its end (as after a download into it) still uploads
    // the whole file, and a download fills a reused fd from the start.
    use std::io::{Read, Seek, SeekFrom, Write};
    let (d, p) = setup();
    let mut scratch = tempfile::tempfile().unwrap();
    scratch.write_all(b"payload").unwrap();
    p.upload_from(
        OwnedFd::from(scratch.try_clone().unwrap()),
        &vp("up"),
        WriteOptions::default(),
        no_progress(),
    )
    .await
    .unwrap();
    assert_eq!(std::fs::read(d.path().join("up")).unwrap(), b"payload");

    std::fs::write(d.path().join("src"), b"abc").unwrap();
    let mut target = tempfile::tempfile().unwrap();
    target.write_all(b"zzzzz").unwrap();
    p.download_into(
        &vp("src"),
        OwnedFd::from(target.try_clone().unwrap()),
        ReadOptions::default(),
        no_progress(),
    )
    .await
    .unwrap();
    target.seek(SeekFrom::Start(0)).unwrap();
    let mut got = Vec::new();
    target.read_to_end(&mut got).unwrap();
    assert_eq!(&got[..3], b"abc");
}
