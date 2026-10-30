// SPDX-License-Identifier: LGPL-2.1-or-later
//! Runs host QML tests (tests/qml/host/tst_*.qml) against the real Lautta types with a temporary
//! home. No Silica on host.

use std::path::PathBuf;
use std::process::Command;

// Child: the one QML file it checks, and the file it creates once that passed (a child whose
// libtest filter matched nothing exits 0 too).
const FILE_VAR: &str = "LAUTTA_QML_HOST_FILE";
const PASSED_VAR: &str = "LAUTTA_QML_HOST_PASSED";

/// Each file in its own child process, all at once: one Qt app per process, files mostly wait on
/// timers.
#[test]
fn host_qml_tests_pass() {
    if let (Ok(file), Ok(passed)) = (std::env::var(FILE_VAR), std::env::var(PASSED_VAR)) {
        check_one(&file);
        std::fs::write(passed, "").unwrap();
        return;
    }
    let dir = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../tests/qml/host");
    let mut files: Vec<String> = std::fs::read_dir(dir)
        .unwrap()
        .filter_map(|e| e.ok().map(|e| e.path()))
        .filter(|p| {
            p.file_name()
                .and_then(|n| n.to_str())
                .is_some_and(|n| n.starts_with("tst_"))
        })
        .map(|p| p.canonicalize().unwrap().display().to_string())
        .collect();
    files.sort();
    assert!(!files.is_empty());
    let exe = std::env::current_exe().unwrap();
    let marks = tempfile::tempdir().unwrap();
    let children: Vec<_> = files
        .iter()
        .enumerate()
        .map(|(i, file)| {
            let passed = marks.path().join(i.to_string());
            let child = Command::new(&exe)
                .args([
                    "--exact",
                    "host_qml_tests_pass",
                    "--nocapture",
                    "--test-threads=1",
                ])
                .env(FILE_VAR, file)
                .env(PASSED_VAR, &passed)
                .spawn()
                .unwrap();
            (file, passed, child)
        })
        .collect();
    let failed: Vec<&String> = children
        .into_iter()
        .filter_map(|(file, passed, mut child)| {
            let ok = child.wait().unwrap().success() && passed.exists();
            (!ok).then_some(file)
        })
        .collect();
    assert!(
        failed.is_empty(),
        "QML host tests failed (see output): {failed:?}"
    );
}

fn check_one(file: &str) {
    let home = tempfile::tempdir().unwrap();
    for d in ["Documents", "Downloads", "Pictures"] {
        std::fs::create_dir_all(home.path().join(d)).unwrap();
    }
    std::env::set_var("HOME", home.path());
    std::env::set_var("QT_QPA_PLATFORM", "offscreen");
    assert_eq!(
        lautta_qt::qml_check(&[file.to_string()]),
        0,
        "QML host test failed: {file}"
    );
}
