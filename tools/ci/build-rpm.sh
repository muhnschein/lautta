#!/bin/sh
# SPDX-License-Identifier: LGPL-2.1-or-later
# Builds the aarch64 RPM in the Sailfish OS Platform SDK container from a
# copy of the tracked sources (the SDK runs as mersdk, uid 100000, and must
# not change ownership in the working tree). The RPM lands in RPMS/.
# Env: SDK_IMAGE (container image), TARGET (sb2 target name),
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
# Dependencies are vendored on the host so the SDK build is offline (RS-6).
(cd "$root" && cargo +stable vendor --locked --versioned-dirs -q "$src/vendor" > /dev/null)
mkdir -p "$src/.cargo-home"
cat > "$src/.cargo-home/config.toml" <<CFG
[source.crates-io]
replace-with = "vendored-sources"
[source.vendored-sources]
directory = "vendor"
CFG
chmod -R a+rwX "$work"
# shellcheck disable=SC2086
docker run --rm ${DOCKER_ARGS:-} -v "$src:/home/mersdk/src" -w /home/mersdk/src "$SDK_IMAGE" \
    bash -euc "${SDK_PREPARE:-true}; ./tools/ci/sdk-build.sh '$TARGET'"
mkdir -p "$root/RPMS"
cp "$src"/RPMS/*.rpm "$root/RPMS/"
ls -l "$root"/RPMS
