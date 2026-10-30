// SPDX-License-Identifier: LGPL-2.1-or-later
//! `main` exported as C symbol (Harbour validator); started directly (`no-invoker`), not via
//! booster.
#![no_main]

use std::os::raw::{c_char, c_int};

/// libhybris: Android GL driver writes bionic's TLS slots (thread pointer + 0..72 bytes). glibc
/// aarch64 puts executable TLS right after 16-byte header, so `thread_local!`s got overwritten.
/// 128-byte aligned member moves the block past the slots. `tools/ci/check-rpm.sh` checks alignment
/// of shipped binary.
#[repr(align(128))]
struct HybrisTlsGap(#[allow(dead_code)] u8);

thread_local! {
    static HYBRIS_TLS_GAP: HybrisTlsGap = const { HybrisTlsGap(0) };
}

/// # Safety
/// Called by C runtime with process arguments.
#[no_mangle]
pub unsafe extern "C" fn main(argc: c_int, argv: *mut *mut c_char) -> c_int {
    // Address escapes: LTO cannot fold the constant away and drop the gap.
    HYBRIS_TLS_GAP.with(|g| {
        std::hint::black_box(g as *const HybrisTlsGap);
    });
    lautta_qt::run(argc, argv)
}
