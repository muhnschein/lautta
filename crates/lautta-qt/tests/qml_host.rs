// SPDX-License-Identifier: LGPL-2.1-or-later
//! Runs the host QML tests (tests/qml/host/tst_*.qml) against the real
//! Lautta types with a temporary home folder. Silica is not available on the
//! host; pages are checked on the target (tools/ci/check-rpm.sh).

use std::path::PathBuf;

#[test]
fn host_qml_tests_pass() {
    let home = tempfile::tempdir().unwrap();
    for d in ["Documents", "Downloads", "Pictures"] {
        std::fs::create_dir_all(home.path().join(d)).unwrap();
    }
    std::env::set_var("HOME", home.path());
    std::env::set_var("QT_QPA_PLATFORM", "offscreen");
    let dir = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../tests/qml/host");
    let mut files: Vec<String> = std::fs::read_dir(&dir)
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
    assert_eq!(
        lautta_qt::qml_check(&files),
        0,
        "QML host tests failed (see output)"
    );
}
