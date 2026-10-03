#!/bin/bash
# SPDX-License-Identifier: LGPL-2.1-or-later
# Developer tool: builds harbour-lautta for the Sailfish OS target in a
# running SDK container and runs `--qml-check` there on the given QML files
# (real Silica, real Lautta types, Qt 5.6). Faster than a full RPM build:
# the container keeps a shared vendor folder and cargo target folder.
#
#   tools/sdk-qml-check.sh [--no-build] <file.qml|dir>...
#
# Env: SDK_CONTAINER (default "sfos", started from the SDK image),
#      TARGET (default SailfishOS-5.2.0.15-aarch64).
set -euo pipefail
container=${SDK_CONTAINER:-sfos}
target=${TARGET:-SailfishOS-5.2.0.15-aarch64}
root=$(cd "$(dirname "$0")/.." && pwd)
name=$(basename "$root")
build=yes
if [[ "${1:-}" == --no-build ]]; then
    build=no
    shift
fi
[[ $# -gt 0 ]] || { echo "usage: $0 [--no-build] <file.qml|dir>..." >&2; exit 2; }

work=/home/mersdk/w/$name
docker exec "$container" mkdir -p "$work" /home/mersdk/shared
# Sources (tracked and new files, no build output).
(cd "$root" && git ls-files -z --cached --others --exclude-standard | grep -zv '^design/' | tar --null -T - -cf -) |
    docker exec -i -u 0 "$container" bash -c "rm -rf '$work/src' && mkdir -p '$work/src' && tar -C '$work/src' -xf - && chown -R mersdk '$work'"

if [[ "$build" == yes ]]; then
    # One vendor folder for every work tree (same Cargo.lock => same content).
    lock_sum=$(sha256sum "$root/Cargo.lock" | cut -c1-16)
    if ! docker exec "$container" test -f "/home/mersdk/shared/vendor-$lock_sum.ok"; then
        tmp=$(mktemp -d)
        (cd "$root" && cargo +stable vendor --locked --versioned-dirs -q "$tmp/vendor" > /dev/null)
        tar -C "$tmp" -cf - vendor | docker exec -i -u 0 "$container" bash -c \
            "rm -rf /home/mersdk/shared/vendor && tar -C /home/mersdk/shared -xf - && chown -R mersdk /home/mersdk/shared && touch /home/mersdk/shared/vendor-$lock_sum.ok"
        rm -rf "$tmp"
    fi
    docker exec "$container" bash -lc "
        set -euo pipefail
        cd '$work/src'
        mkdir -p .cargo-home
        printf '[source.crates-io]\nreplace-with = \"v\"\n[source.v]\ndirectory = \"/home/mersdk/shared/vendor\"\n' > .cargo-home/config.toml
        export CARGO_HOME=\$PWD/.cargo-home CARGO_TARGET_DIR=/home/mersdk/shared/target QT_INCLUDE_PATH=/usr/include/qt5 QT_LIBRARY_PATH=/usr/lib64 CARGO_INCREMENTAL=0
        export RUSTFLAGS='-C link-arg=-Wl,--as-needed'
        for attempt in 1 2 3 4; do
            if timeout 1200 sb2 -t '$target' cargo build --frozen --offline --target aarch64-unknown-linux-gnu -p harbour-lautta --features sailfish 2>&1 | grep -E '^(error|warning: unused)|^\s+-->|Finished' ; then
                break
            fi
            echo \"build attempt \$attempt stalled or failed; retrying\"
        done
        cp /home/mersdk/shared/target/aarch64-unknown-linux-gnu/debug/harbour-lautta '$work/harbour-lautta'
    "
fi

files=()
for arg in "$@"; do
    if [[ -d "$root/$arg" || -d "$arg" ]]; then
        rel=$(realpath --relative-to="$root" "$arg")
        while IFS= read -r f; do files+=("$work/src/$f"); done < <(cd "$root" && find "$rel" -name '*.qml' | sort)
    else
        files+=("$work/src/$(realpath --relative-to="$root" "$arg")")
    fi
done
docker exec "$container" bash -lc "
    home=\$(mktemp -d) && mkdir -p \$home/Documents \$home/Downloads \$home/Pictures
    sb2 -t '$target' env HOME=\$home QT_QPA_PLATFORM=minimal '$work/harbour-lautta' --qml-check -I '$work/src/qml' ${files[*]}
"
