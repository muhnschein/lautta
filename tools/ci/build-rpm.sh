#!/bin/sh
# SPDX-License-Identifier: LGPL-2.1-or-later
# Builds the aarch64 RPM in the Sailfish OS Platform SDK container from a
# copy of the tracked sources (the SDK runs as mersdk, uid 100000, and must
# not change ownership in the working tree). The RPM lands in RPMS/.
# Env: SDK_IMAGE (container image), TARGET (sb2 target name),
#      SDK_CROSS_DIR (lifted SDK for the cross build, default build-sdk-cross),
#      DOCKER_ARGS (extra docker run arguments), SDK_PREPARE (command run first
#      in the container, e.g. trusting a proxy CA).
set -eu
: "${SDK_IMAGE:?}" "${TARGET:?}"
root=$(cd "$(dirname "$0")/../.." && pwd)
work=$(mktemp -d)
trap 'rm -rf "$work" 2>/dev/null || sudo rm -rf "$work"' EXIT
src=$work/src
mkdir -p "$src"
(cd "$root" && git ls-files -z --cached --others --exclude-standard | xargs -0 cp --parents -t "$src")
# The binary is cross-built with the host's cargo (SPEC RS-6): the SDK
# target's rustc is too old. lift-sdk.sh needs the i386 C library on the host.
sdk=${SDK_CROSS_DIR:-$root/build-sdk-cross}
[ -x "$sdk/bin/aarch64-meego-linux-gnu-gcc" ] || "$root/tools/ci/lift-sdk.sh" "$SDK_IMAGE" "$TARGET" "$sdk"
"$root/tools/ci/cross-build.sh" "$sdk" "$root/prebuilt"
mkdir -p "$src/prebuilt"
cp "$root/prebuilt/harbour-lautta" "$src/prebuilt/"
chmod -R a+rwX "$work"
# shellcheck disable=SC2086
docker run --rm ${DOCKER_ARGS:-} -v "$src:/home/mersdk/src" -w /home/mersdk/src "$SDK_IMAGE" \
    bash -euc "${SDK_PREPARE:-true}; ./tools/ci/sdk-build.sh '$TARGET'"
mkdir -p "$root/RPMS"
cp "$src"/RPMS/*.rpm "$root/RPMS/"
ls -l "$root"/RPMS
