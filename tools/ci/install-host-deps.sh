#!/bin/sh
# SPDX-License-Identifier: LGPL-2.1-or-later
# Host build dependencies for CI (Ubuntu 24.04): Qt 5 for the Qt glue crates,
# the pinned Rust toolchain (rust-toolchain.toml), and optional tools:
#   install-host-deps.sh [coverage] [mutants]
set -eu
sudo apt-get update -q
sudo apt-get install -y -q --no-install-recommends \
    qtbase5-dev qttools5-dev-tools qtdeclarative5-dev qtmultimedia5-dev libsqlite3-dev liblzma-dev libbz2-dev pkg-config
rustup show active-toolchain || rustup toolchain install
for tool in "$@"; do
    case $tool in
        coverage) cargo +stable install --locked cargo-llvm-cov@0.6.16 ;;
        mutants) cargo +stable install --locked cargo-mutants@25.0.1 ;;
        *) echo "unknown tool $tool" >&2; exit 2 ;;
    esac
done
