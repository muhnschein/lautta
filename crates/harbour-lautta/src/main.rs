// SPDX-License-Identifier: LGPL-2.1-or-later
#![no_main]

use std::os::raw::{c_char, c_int};

#[repr(align(128))]
struct HybrisTlsGap(#[allow(dead_code)] u8);

thread_local! {
    static HYBRIS_TLS_GAP: HybrisTlsGap = const { HybrisTlsGap(0) };
}

/// # Safety
/// Called by C runtime with process arguments.
#[no_mangle]
pub unsafe extern "C" fn main(argc: c_int, argv: *mut *mut c_char) -> c_int {
    HYBRIS_TLS_GAP.with(|g| {
        std::hint::black_box(g as *const HybrisTlsGap);
    });
    lautta_qt::run(argc, argv)
}
