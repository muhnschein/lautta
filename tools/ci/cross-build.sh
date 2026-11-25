#!/bin/sh
# SPDX-License-Identifier: LGPL-2.1-or-later
set -eu
sdk=$(cd "$1" && pwd)
mkdir -p "$2"
out=$(cd "$2" && pwd)
root=$(cd "$(dirname "$0")/../.." && pwd)
triple=aarch64-unknown-linux-gnu
gcc="$sdk/bin/aarch64-meego-linux-gnu-gcc"

export CARGO_TARGET_AARCH64_UNKNOWN_LINUX_GNU_LINKER="$gcc"
export TARGET_CC="$gcc"
export TARGET_CXX="$sdk/bin/aarch64-meego-linux-gnu-g++"
export TARGET_AR="$sdk/bin/aarch64-meego-linux-gnu-ar"
export QT_INCLUDE_PATH="$sdk/sysroot/usr/include/qt5"
export QT_LIBRARY_PATH="$sdk/sysroot/usr/lib64"
export SAILFISHAPP_INCLUDE_PATH="$sdk/sysroot/usr/include/sailfishapp"
export PKG_CONFIG_ALLOW_CROSS=1
export TARGET_PKG_CONFIG_LIBDIR="$sdk/linklib/pkgconfig"
export SQLITE3_LIB_DIR="$sdk/linklib"
export SQLITE3_STATIC=0
export LIBSQLITE3_SYS_USE_PKG_CONFIG=1
export LZMA_API_STATIC=0
export CARGO_TARGET_AARCH64_UNKNOWN_LINUX_GNU_RUSTFLAGS="-C link-arg=-Wl,--as-needed -C link-arg=-Wl,-z,relro,-z,now"
export CARGO_TARGET_DIR="$out/target"
export CARGO_INCREMENTAL=0

cd "$root"
rustup target add "$triple" > /dev/null
cargo build --release --locked --target "$triple" -p harbour-lautta --features sailfish
cp -f "$CARGO_TARGET_DIR/$triple/release/harbour-lautta" "$out/harbour-lautta"
ls -l "$out/harbour-lautta"
