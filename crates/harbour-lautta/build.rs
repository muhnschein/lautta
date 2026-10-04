// SPDX-License-Identifier: LGPL-2.1-or-later
//! Exports `main` from the PIE (SPEC RS-3).
fn main() {
    println!("cargo:rustc-link-arg-bins=-Wl,--export-dynamic-symbol=main");
}
