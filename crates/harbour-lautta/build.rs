// SPDX-License-Identifier: LGPL-2.1-or-later
fn main() {
    println!("cargo:rustc-link-arg-bins=-Wl,--export-dynamic-symbol=main");
}
