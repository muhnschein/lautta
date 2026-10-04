// SPDX-License-Identifier: LGPL-2.1-or-later
//! The `harbour-lautta` binary. `main` is exported as a C symbol, as the
//! Harbour validator requires of Silica apps (SPEC RS-3, HBR-4); the app is
//! started directly (`no-invoker`), not through the booster.
#![no_main]

use std::os::raw::{c_char, c_int};

/// Moves this executable's thread-local block clear of the Android TLS slots.
///
/// On libhybris devices the Android GL driver writes bionic's fixed slots
/// (thread pointer + 0..72 bytes: OpenGL, stack guard, ...). glibc on aarch64
/// puts the executable's TLS right after its 16-byte header, so Rust
/// `thread_local!`s there were overwritten as soon as GL started (a
/// `RefCell` reading as "already borrowed" on the first launch). The TLS
/// block starts at the thread pointer rounded up to its alignment, so a
/// 128-byte aligned member moves all of it past the slots.
/// `tools/ci/check-rpm.sh` checks the alignment of the shipped binary.
#[repr(align(128))]
struct HybrisTlsGap(u8);

thread_local! {
    static HYBRIS_TLS_GAP: HybrisTlsGap = const { HybrisTlsGap(0) };
}

/// # Safety
/// Called by the C runtime (or the booster) with the process arguments.
#[no_mangle]
pub unsafe extern "C" fn main(argc: c_int, argv: *mut *mut c_char) -> c_int {
    // Keeps the gap (and so the alignment) in the binary: its address escapes,
    // so not even LTO can fold the constant away and drop the variable.
    HYBRIS_TLS_GAP.with(|g| {
        std::hint::black_box(g as *const HybrisTlsGap);
    });
    lautta_qt::run(argc, argv)
}
