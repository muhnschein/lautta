#!/bin/sh
# SPDX-License-Identifier: LGPL-2.1-or-later
# Host build dependencies for CI (Ubuntu 24.04): Qt 5 for the Qt glue crates,
# the pinned Rust toolchain (rust-toolchain.toml), and optional tools:
#   install-host-deps.sh [coverage] [mutants] [cross]
# A cargo tool already at its pinned version is kept: CI puts prebuilt ones
# there first (taiki-e/install-action, same versions), building them takes
# minutes.
set -eu
sudo apt-get update -q
sudo apt-get install -y -q --no-install-recommends \
    qtbase5-dev qttools5-dev-tools qtdeclarative5-dev qtmultimedia5-dev qml-module-qtquick2 qml-module-qtqml-models2 qml-module-qtmultimedia libsqlite3-dev liblzma-dev libbz2-dev pkg-config
rustup show active-toolchain || rustup toolchain install
# cargo_tool CRATE VERSION
cargo_tool() {
    if cargo "${1#cargo-}" --version 2> /dev/null | grep -qF " $2"; then
        echo "$1 $2 already installed"
    else
        cargo +stable install --locked "$1@$2"
    fi
}
for tool in "$@"; do
    case $tool in
        coverage) cargo_tool cargo-llvm-cov 0.6.16 ;;
        mutants) cargo_tool cargo-mutants 25.0.1 ;;
        # The SDK's cross compiler is an i386 program (tools/ci/lift-sdk.sh).
        cross) sudo apt-get install -y -q --no-install-recommends libc6-i386 ;;
        *) echo "unknown tool $tool" >&2; exit 2 ;;
    esac
done
