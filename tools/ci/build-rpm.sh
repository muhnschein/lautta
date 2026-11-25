#!/bin/sh
# SPDX-License-Identifier: LGPL-2.1-or-later
set -eu
: "${SDK_IMAGE:?}" "${TARGET:?}"
root=$(cd "$(dirname "$0")/../.." && pwd)
work=$(mktemp -d)
trap 'rm -rf "$work" 2>/dev/null || sudo rm -rf "$work"' EXIT
src=$work/src
mkdir -p "$src"
(cd "$root" && git ls-files -z --cached --others --exclude-standard | xargs -0 cp --parents -t "$src")
sdk=${SDK_CROSS_DIR:-$root/build-sdk-cross}
[ -x "$sdk/bin/aarch64-meego-linux-gnu-gcc" ] || "$root/tools/ci/lift-sdk.sh" "$SDK_IMAGE" "$TARGET" "$sdk"
"$root/tools/ci/cross-build.sh" "$sdk" "$root/prebuilt"
mkdir -p "$src/prebuilt"
cp "$root/prebuilt/harbour-lautta" "$src/prebuilt/"
chmod -R a+rwX "$work"
docker run --rm ${DOCKER_ARGS:-} -v "$src:/home/mersdk/src" -w /home/mersdk/src "$SDK_IMAGE" \
    bash -euc "${SDK_PREPARE:-true}; ./tools/ci/sdk-build.sh '$TARGET'"
mkdir -p "$root/RPMS"
cp "$src"/RPMS/*.rpm "$root/RPMS/"
ls -l "$root"/RPMS
