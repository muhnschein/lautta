#!/bin/sh
# SPDX-License-Identifier: LGPL-2.1-or-later
# Cross-builds harbour-lautta for the Sailfish OS target with the host's
# cargo (the pinned toolchain, rust-toolchain.toml) and the SDK's own gcc,
# Qt and sysroot, as lifted by lift-sdk.sh (SPEC RS-6). The binary links
# against the target's glibc, Qt and system libraries, so its symbol
# versions are those of the SDK target. The spec installs it (prebuilt/).
# Usage: cross-build.sh <sdk dir> <out dir>
set -eu
sdk=$(cd "$1" && pwd)
mkdir -p "$2"
out=$(cd "$2" && pwd)
root=$(cd "$(dirname "$0")/../.." && pwd)
triple=aarch64-unknown-linux-gnu
gcc="$sdk/bin/aarch64-meego-linux-gnu-gcc"

export CARGO_TARGET_AARCH64_UNKNOWN_LINUX_GNU_LINKER="$gcc"
# cc and pkg-config read TARGET_* for the target only; build scripts
# themselves still build with the host's compiler.
export TARGET_CC="$gcc"
export TARGET_CXX="$sdk/bin/aarch64-meego-linux-gnu-g++"
export TARGET_AR="$sdk/bin/aarch64-meego-linux-gnu-ar"
# Qt and libsailfishapp of the target (crates/lautta-qt/build.rs).
export QT_INCLUDE_PATH="$sdk/sysroot/usr/include/qt5"
export QT_LIBRARY_PATH="$sdk/sysroot/usr/lib64"
export SAILFISHAPP_INCLUDE_PATH="$sdk/sysroot/usr/include/sailfishapp"
# The system libraries Harbour allows (HBR-2), found through lift-sdk.sh's
# pkg-config files; libsqlite3-sys uses its shipped bindings.
export PKG_CONFIG_ALLOW_CROSS=1
export TARGET_PKG_CONFIG_LIBDIR="$sdk/linklib/pkgconfig"
export SQLITE3_LIB_DIR="$sdk/linklib"
export SQLITE3_STATIC=0
export LIBSQLITE3_SYS_USE_PKG_CONFIG=1
# xz is compiled in, as before the switch (lzma-sys builds it whenever the
# variable is set): the validator refuses liblzma's versioned symbols.
export LZMA_API_STATIC=0
# Hardening as before the switch (PIE is rustc's default for this target).
export CARGO_TARGET_AARCH64_UNKNOWN_LINUX_GNU_RUSTFLAGS="-C link-arg=-Wl,--as-needed -C link-arg=-Wl,-z,relro,-z,now"
export CARGO_TARGET_DIR="$out/target"
export CARGO_INCREMENTAL=0

cd "$root"
rustup target add "$triple" > /dev/null
cargo build --release --locked --target "$triple" -p harbour-lautta --features sailfish
cp -f "$CARGO_TARGET_DIR/$triple/release/harbour-lautta" "$out/harbour-lautta"
ls -l "$out/harbour-lautta"
