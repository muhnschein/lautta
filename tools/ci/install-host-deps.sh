#!/bin/sh
# SPDX-License-Identifier: LGPL-2.1-or-later
# Host build dependencies (Ubuntu 24.04): Qt 5, pinned Rust toolchain, optional tools:
#   install-host-deps.sh [nextest] [coverage] [mutants] [cross]
# Cargo tool already at pinned version is kept: CI installs prebuilt ones first.
set -eu
sudo apt-get update -q
sudo apt-get install -y -q --no-install-recommends \
    qtbase5-dev qttools5-dev-tools qtdeclarative5-dev qtmultimedia5-dev qml-module-qtquick2 qml-module-qtqml-models2 qml-module-qtmultimedia libsqlite3-dev liblzma-dev libbz2-dev pkg-config
rustup show active-toolchain || rustup toolchain install
cargo_tool() {
    crate=$1 version=$2
    if cargo "${crate#cargo-}" --version 2> /dev/null | grep -qF " $version"; then
        echo "$crate $version already installed"
    else
        cargo +stable install --locked "$crate@$version"
    fi
}
for tool in "$@"; do
    case $tool in
        nextest) cargo_tool cargo-nextest 0.9.148 ;;
        coverage) cargo_tool cargo-llvm-cov 0.6.16 ;;
        mutants) cargo_tool cargo-mutants 25.0.1 ;;
        # SDK's cross compiler is an i386 program.
        cross) sudo apt-get install -y -q --no-install-recommends libc6-i386 ;;
        *) echo "unknown tool $tool" >&2; exit 2 ;;
    esac
done
