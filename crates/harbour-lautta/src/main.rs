// SPDX-License-Identifier: LGPL-2.1-or-later
//! The `harbour-lautta` binary. `main` is exported as a C symbol so the
//! silica-qt5 booster can load the PIE and call it (SPEC RS-3, HBR-4).
#![no_main]

use std::os::raw::{c_char, c_int};

/// # Safety
/// Called by the C runtime (or the booster) with the process arguments.
#[no_mangle]
pub unsafe extern "C" fn main(argc: c_int, argv: *mut *mut c_char) -> c_int {
    lautta_qt::run(argc, argv)
}
