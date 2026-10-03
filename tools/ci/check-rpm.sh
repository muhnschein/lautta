#!/bin/sh
# SPDX-License-Identifier: LGPL-2.1-or-later
# Checks the built RPM (RPMS/*.rpm) in the Sailfish OS Platform SDK:
#   - Harbour intake: sdk-harbour-rpmvalidator at the commit SPEC §3.2 names
#     (TST-7); any error fails, warnings must be listed in doc/harbour-warnings.md;
#   - the binary exports `main` and links __libc_start_main@GLIBC_2.34 (HBR-4),
#     carries no RPATH and needs only HBR-2 libraries;
#   - the package installs into the target and every library resolves.
# Env: SDK_IMAGE (container image), TARGET (sb2 target name), DOCKER_ARGS.
set -eu
: "${SDK_IMAGE:?}" "${TARGET:?}"
VALIDATOR_COMMIT=7dd7dd5

if [ "${1:-}" != --in-sdk ]; then
    root=$(cd "$(dirname "$0")/../.." && pwd)
    if [ ! -d "$root/.validator" ]; then
        git clone -q https://github.com/sailfishos/sdk-harbour-rpmvalidator.git "$root/.validator"
    fi
    git -C "$root/.validator" checkout -q "$VALIDATOR_COMMIT"
    sudo=""; [ "$(id -u)" = 0 ] || sudo=sudo
    $sudo chown -R 100000:100000 "$root"
    trap '$sudo chown -R "$(id -u):$(id -g)" "$root"' EXIT
    # shellcheck disable=SC2086
    docker run --rm ${DOCKER_ARGS:-} -e TARGET -e SDK_IMAGE -v "$root:/home/mersdk/src" -w /home/mersdk/src \
        "$SDK_IMAGE" bash -euc "${SDK_PREPARE:-true}; sh tools/ci/check-rpm.sh --in-sdk"
    exit 0
fi

status=0
fail() {
    echo "FAIL: $1"
    status=1
}

rpm=$(ls RPMS/harbour-lautta-*.aarch64.rpm | grep -v -e debuginfo -e debugsource | head -n 1)
echo "== $rpm"
rpm -qp --requires "$rpm" | sed 's/^/  requires: /'

# Harbour validator. Output lines starting with ERROR fail the build;
# WARNING lines must be documented.
out=$(cd .validator && ./rpmvalidation.sh -t "$TARGET" "../$rpm" 2>&1) || true
echo "$out"
if echo "$out" | grep -q '^ERROR'; then
    fail "rpmvalidator reported errors"
fi
echo "$out" | sed -n 's/^WARNING *\[[^]]*\] *//p' | while IFS= read -r warning; do
    if ! grep -qF -- "$warning" doc/harbour-warnings.md; then
        echo "FAIL: undocumented validator warning: $warning"
        echo undocumented >> /tmp/lautta-undocumented
    fi
done
[ ! -f /tmp/lautta-undocumented ] || fail "undocumented validator warnings (doc/harbour-warnings.md)"

work=$(mktemp -d)
(cd "$work" && rpm2cpio "$OLDPWD/$rpm" | cpio -idm --quiet)
bin=$work/usr/bin/harbour-lautta
nm -D "$bin" | grep -qE ' T main$' || fail "main is not exported (HBR-4)"
nm -D "$bin" | grep -q '__libc_start_main@GLIBC_2.34' || fail "__libc_start_main@GLIBC_2.34 not linked (HBR-4)"
if readelf -d "$bin" | grep -E 'RPATH|RUNPATH'; then
    fail "binary carries a library search path"
fi
allowed="libc.so.6 libm.so.6 libpthread.so.0 libdl.so.2 librt.so.1 libgcc_s.so.1 libstdc++.so.6 \
libQt5Core.so.5 libQt5Gui.so.5 libQt5Qml.so.5 libQt5Quick.so.5 libQt5Multimedia.so.5 \
libsailfishapp.so.1 libsqlite3.so.0 libz.so.1 liblzma.so.5 libbz2.so.1 ld-linux-aarch64.so.1"
for needed in $(readelf -d "$bin" | sed -n 's/.*Shared library: \[\(.*\)\]/\1/p'); do
    case " $allowed " in
        *" $needed "*) echo "  needs $needed" ;;
        *) fail "links $needed, not in the HBR-2 list" ;;
    esac
done
size=$(stat -c %s "$bin")
[ "$size" -le 20971520 ] || fail "binary is $size bytes, over the 20 MB budget (RS-7)"

sb2 -t "$TARGET" -m sdk-install -R zypper --non-interactive in --allow-unsigned-rpm "$rpm"
if sb2 -t "$TARGET" ldd /usr/bin/harbour-lautta | grep 'not found'; then
    fail "unresolved libraries"
fi
rm -rf "$work"
exit $status
