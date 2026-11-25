// SPDX-License-Identifier: LGPL-2.1-or-later

use std::os::unix::process::ExitStatusExt;
use std::process::Command;

const DIR_VAR: &str = "LAUTTA_CRASH_REPORT_DIR";

fn crash_after_a_passing_check(dir: &str) {
    std::env::set_var("HOME", dir);
    std::env::set_var("QT_QPA_PLATFORM", "offscreen");
    lautta_qt::report_crashes();
    let qml = std::path::Path::new(dir).join("tst_done.qml");
    std::fs::write(
        &qml,
        "import QtQuick 2.6\nItem { property bool reachedEnd: true }\n",
    )
    .unwrap();
    assert_eq!(lautta_qt::qml_check(&[qml.display().to_string()]), 0);
    std::thread::Builder::new()
        .name("crasher".into())
        .spawn(|| unsafe { libc::raise(libc::SIGSEGV) })
        .unwrap()
        .join()
        .unwrap();
    panic!("survived SIGSEGV");
}

#[test]
fn a_crash_in_qml_check_names_its_thread_and_frames_and_keeps_the_results() {
    if let Ok(dir) = std::env::var(DIR_VAR) {
        crash_after_a_passing_check(&dir);
        return;
    }
    let dir = tempfile::tempdir().unwrap();
    let exe = std::env::current_exe().unwrap();
    let out = Command::new(&exe)
        .args([
            "--exact",
            "a_crash_in_qml_check_names_its_thread_and_frames_and_keeps_the_results",
            "--nocapture",
            "--test-threads=1",
        ])
        .env(DIR_VAR, dir.path())
        .output()
        .unwrap();
    let stdout = String::from_utf8_lossy(&out.stdout);
    let stderr = String::from_utf8_lossy(&out.stderr);
    assert_eq!(
        out.status.signal(),
        Some(libc::SIGSEGV),
        "dies by the signal: {out:?}"
    );
    assert!(
        stderr.contains("CRASH   signal 11 in thread crasher\n"),
        "names signal and thread: {stderr}"
    );
    let name = exe.file_name().unwrap().to_string_lossy().into_owned();
    assert!(
        stderr
            .lines()
            .any(|l| l.contains(&format!("{name}(")) && l.contains("[0x")),
        "prints frames of the binary: {stderr}"
    );
    assert!(
        stdout.contains(&format!("ok      {}", dir.path().join("tst_done.qml").display())),
        "results printed before the crash are kept: {stdout}"
    );
}
