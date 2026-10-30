// SPDX-License-Identifier: LGPL-2.1-or-later
//! Each case runs in a child (this binary again for one test): hook is process-wide, abort takes
//! the whole process.

use lautta_core::panics::{abort_outside_catch, catch, Panicked};
use std::os::unix::process::ExitStatusExt;
use std::process::{Command, Output};

const CASE: &str = "LAUTTA_PANIC_HOOK_CASE";

fn in_child() -> bool {
    std::env::var_os(CASE).is_some()
}

fn child(name: &str) -> Output {
    Command::new(std::env::current_exe().unwrap())
        .args([name, "--exact", "--nocapture", "--test-threads=1"])
        .env(CASE, "1")
        .output()
        .unwrap()
}

#[test]
fn a_panic_inside_a_catch_is_an_error_and_the_process_goes_on() {
    if in_child() {
        abort_outside_catch();
        assert_eq!(catch(|| panic!("hayro tripped")), Err::<(), _>(Panicked));
        println!("went on");
        return;
    }
    let out = child("a_panic_inside_a_catch_is_an_error_and_the_process_goes_on");
    assert!(out.status.success(), "{out:?}");
    assert!(
        String::from_utf8_lossy(&out.stdout).contains("went on"),
        "{out:?}"
    );
    assert!(
        String::from_utf8_lossy(&out.stderr).contains("hayro tripped"),
        "the panic is still reported: {out:?}"
    );
}

#[test]
fn any_other_panic_ends_the_process() {
    if in_child() {
        abort_outside_catch();
        panic!("a bug elsewhere");
    }
    let out = child("any_other_panic_ends_the_process");
    assert_eq!(
        out.status.signal(),
        Some(6),
        "SIGABRT, not a failed test: {out:?}"
    );
    assert!(
        String::from_utf8_lossy(&out.stderr).contains("a bug elsewhere"),
        "reported before the abort: {out:?}"
    );
}
