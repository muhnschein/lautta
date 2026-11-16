// SPDX-License-Identifier: LGPL-2.1-or-later

use lautta_core::app::Core;
use lautta_core::entry::{cap, Capabilities};
use lautta_core::locations::{Location, LocationKind, LocationRegistry};
use lautta_core::paths::AppPaths;
use lautta_core::provider::memory::MemoryProvider;
use lautta_core::provider::{no_progress, ProgressSink};
use lautta_core::{Error, ErrorKind, Uri};
use std::path::PathBuf;
use std::sync::{Arc, Mutex};

const PDF: &[u8] = b"%PDF-1.4 scanned pages";

struct Home {
    _dir: tempfile::TempDir,
    root: PathBuf,
    core: Arc<Core>,
    pdf: PathBuf,
}

async fn home() -> Home {
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path().to_path_buf();
    std::fs::create_dir_all(root.join("Documents/Letters")).unwrap();
    let paths = AppPaths::new(&root);
    let registry = LocationRegistry::with_media_root(paths.clone(), root.join("media"));
    let core = Core::open_with(paths, registry).await.unwrap();
    let pdf = root.join("built.pdf");
    std::fs::write(&pdf, PDF).unwrap();
    Home {
        _dir: dir,
        root,
        core,
        pdf,
    }
}

fn uri(s: &str) -> Uri {
    Uri::parse(s).unwrap()
}

fn server(h: &Home, caps: &[&str]) -> Arc<MemoryProvider> {
    let mem = Arc::new(MemoryProvider::new(Capabilities::with(caps)));
    mem.add_dir("scans");
    let location = Location::remote("nas", LocationKind::AdHoc, "NAS", Some("sftp://nas.home".into()));
    h.core.locations.register(location, mem.clone());
    mem
}

fn names(dir: &std::path::Path) -> Vec<String> {
    let mut names: Vec<String> = std::fs::read_dir(dir)
        .unwrap()
        .map(|e| e.unwrap().file_name().to_string_lossy().into_owned())
        .collect();
    names.sort();
    names
}

#[tokio::test]
async fn scan_lands_in_the_device_folder_under_a_free_name() {
    let h = home().await;
    let dest = uri("lautta://user-documents/Letters");
    let saved = h
        .core
        .save_scan(&h.pdf, &dest, " Scan 2026-11-12 ", no_progress())
        .await
        .unwrap();
    assert_eq!(
        saved,
        uri("lautta://user-documents/Letters/Scan%202026-11-12.pdf")
    );
    let folder = h.root.join("Documents/Letters");
    assert_eq!(std::fs::read(folder.join("Scan 2026-11-12.pdf")).unwrap(), PDF);
    let again = h
        .core
        .save_scan(&h.pdf, &dest, "Scan 2026-11-12", no_progress())
        .await
        .unwrap();
    assert_eq!(
        again,
        uri("lautta://user-documents/Letters/Scan%202026-11-12%202.pdf")
    );
    assert_eq!(names(&folder), ["Scan 2026-11-12 2.pdf", "Scan 2026-11-12.pdf"]);
    assert!(h.pdf.exists(), "built file is left to its owner");
}

#[tokio::test]
async fn scan_is_uploaded_to_a_server_folder_with_progress() {
    let h = home().await;
    let mem = server(&h, &[cap::WRITE]);
    let seen = Arc::new(Mutex::new(Vec::new()));
    let sink: ProgressSink = {
        let seen = seen.clone();
        Arc::new(move |done, total| seen.lock().unwrap().push((done, total)))
    };
    let saved = h
        .core
        .save_scan(&h.pdf, &uri("lautta://nas/scans"), "contract", sink)
        .await
        .unwrap();
    assert_eq!(saved, uri("lautta://nas/scans/contract.pdf"));
    assert_eq!(mem.read_file("scans/contract.pdf").as_deref(), Some(PDF));
    assert_eq!(mem.paths(), ["scans", "scans/contract.pdf"]);
    let size = PDF.len() as u64;
    assert_eq!(seen.lock().unwrap().last(), Some(&(size, Some(size))));
}

#[tokio::test]
async fn cut_upload_leaves_no_file_behind() {
    let h = home().await;
    let mem = server(&h, &[cap::WRITE]);
    let dest = uri("lautta://nas/scans");
    mem.fail_next(
        "upload_from",
        "scans/.contract.pdf.lautta-part",
        Error::kind(ErrorKind::ConnectionLost),
    );
    let e = h
        .core
        .save_scan(&h.pdf, &dest, "contract", no_progress())
        .await
        .unwrap_err();
    assert_eq!(e.kind, ErrorKind::ConnectionLost);
    assert_eq!(mem.paths(), ["scans"]);
    // Upload done, last step fails: the hidden file goes too.
    mem.fail_next(
        "rename",
        "scans/.contract.pdf.lautta-part",
        Error::kind(ErrorKind::NoSpace),
    );
    let e = h
        .core
        .save_scan(&h.pdf, &dest, "contract", no_progress())
        .await
        .unwrap_err();
    assert_eq!(e.kind, ErrorKind::NoSpace);
    assert_eq!(mem.paths(), ["scans"]);
    h.core
        .save_scan(&h.pdf, &dest, "contract", no_progress())
        .await
        .unwrap();
    assert_eq!(mem.paths(), ["scans", "scans/contract.pdf"]);
}

#[tokio::test]
async fn leftover_of_an_earlier_try_does_not_block_saving() {
    let h = home().await;
    let folder = h.root.join("Documents/Letters");
    std::fs::write(folder.join(".bill.pdf.lautta-part"), b"half").unwrap();
    h.core
        .save_scan(
            &h.pdf,
            &uri("lautta://user-documents/Letters"),
            "bill",
            no_progress(),
        )
        .await
        .unwrap();
    assert_eq!(names(&folder), ["bill.pdf"]);
    assert_eq!(std::fs::read(folder.join("bill.pdf")).unwrap(), PDF);
}

#[tokio::test]
async fn bad_destinations_names_and_sources_are_refused() {
    let h = home().await;
    let dest = uri("lautta://user-documents/Letters");
    let folder = h.root.join("Documents/Letters");
    let e = h
        .core
        .save_scan(&h.pdf, &dest, "a/b", no_progress())
        .await
        .unwrap_err();
    assert_eq!(e.kind, ErrorKind::InvalidName);
    let gone = h.root.join("gone.pdf");
    let e = h
        .core
        .save_scan(&gone, &dest, "x", no_progress())
        .await
        .unwrap_err();
    assert_eq!(e.kind, ErrorKind::NotFound);
    let e = h
        .core
        .save_scan(&h.pdf, &uri("lautta://nowhere/"), "x", no_progress())
        .await
        .unwrap_err();
    assert_eq!(e.kind, ErrorKind::NotFound);
    let e = h
        .core
        .save_scan(
            &h.pdf,
            &uri("lautta://user-documents/Missing"),
            "x",
            no_progress(),
        )
        .await
        .unwrap_err();
    assert_eq!(e.kind, ErrorKind::NotFound);
    assert!(names(&folder).is_empty());
    let mem = server(&h, &[cap::WRITE, cap::READ_ONLY]);
    let e = h
        .core
        .save_scan(&h.pdf, &uri("lautta://nas/scans"), "x", no_progress())
        .await
        .unwrap_err();
    assert_eq!(e.kind, ErrorKind::ReadOnlyFilesystem);
    assert_eq!(mem.calls(), Vec::<String>::new());
}

#[tokio::test]
async fn saved_scan_shows_up_in_a_cached_folder_listing() {
    let h = home().await;
    let dest = uri("lautta://user-documents/Letters");
    h.core.dircache.put(&dest, &[]).unwrap();
    h.core
        .save_scan(&h.pdf, &dest, "bill", no_progress())
        .await
        .unwrap();
    assert!(h.core.dircache.get(&dest).unwrap().is_none());
}
