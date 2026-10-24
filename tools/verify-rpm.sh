#!/bin/sh
# SPDX-License-Identifier: LGPL-2.1-or-later
# Rebuilds a commit from a fresh clone and checks RPMs against it (PKG-4,
# doc/VERIFYING.md).
#
#     tools/verify-rpm.sh --ref <ref> <rpm>...
#
#   --ref <ref>  commit (full hash), branch or tag the RPMs claim to be
#   <rpm>        packages to check, e.g. a CI artifact or one off a phone
#
# Needs Linux on x86_64, git, Docker, the i386 C library (Ubuntu: libc6-i386,
# for the SDK's compiler) and rustup. First run: SDK image pull (~13 GB
# unpacked), ~10 min build. Builds in a fresh clone under LAUTTA_WORKDIR,
# never in this tree; needs ~4 GB free there (checks for 5).
#
# Exit, worst of the RPMs: 0 same file, byte for byte; 1 same files inside,
# different packaging; 2 files differ; 3 couldn't check (tools/compare-rpms.sh).
#
# Environment, all optional:
#   LAUTTA_REPO     where to clone from; default GitHub
#   LAUTTA_KEEP     1: keep the clone and build
#   LAUTTA_WORKDIR  where to clone and build; default ~/.cache/lautta-verify
#                   (not $TMPDIR: /tmp often a RAM disk, small)
#   DOCKER_ARGS, SDK_PREPARE pass through to tools/ci/build-rpm.sh.
set -eu

prog=verify-rpm
repo=${LAUTTA_REPO:-https://github.com/muhnschein/lautta.git}
here=$(cd "$(dirname "$0")/.." && pwd)

usage() {
    sed -n '6,9p' "$0" | sed 's/^# \{0,1\}//' >&2
    exit 3
}
fail() {
    echo "$prog: $*" >&2
    exit 3
}

ref=""
while [ $# -gt 0 ]; do
    case "$1" in
        --ref) [ $# -ge 2 ] || usage; ref=$2; shift ;;
        -h | --help) usage ;;
        -*) echo "$prog: unknown option $1" >&2; usage ;;
        *) break ;;
    esac
    shift
done
if [ -z "$ref" ] || [ $# -eq 0 ]; then
    usage
fi
for rpm in "$@"; do
    [ -f "$rpm" ] || fail "no such file: $rpm"
done

# Before ten minutes of build, not during.
command -v docker > /dev/null || fail "needs Docker"
docker info > /dev/null 2>&1 || fail "Docker is not running, or not for this user"
command -v rustup > /dev/null || fail "needs rustup"
[ -e /lib/ld-linux.so.2 ] || fail "needs the i386 C library for the SDK's compiler (Ubuntu: libc6-i386)"
need_gb=5
parent=${LAUTTA_WORKDIR:-${XDG_CACHE_HOME:-$HOME/.cache}/lautta-verify}
mkdir -p "$parent" || fail "can't create $parent"
free_gb=$(df -Pk "$parent" | awk 'NR == 2 { print int($4 / 1048576) }')
[ "${free_gb:-0}" -ge "$need_gb" ] ||
    fail "$parent: ${free_gb:-?} GB free, build needs $need_gb GB; set LAUTTA_WORKDIR to a roomier disk"
work=$(mktemp -d "$parent/run.XXXXXX")
# shellcheck disable=SC2317,SC2329 # run by the trap.
cleanup() {
    if [ "${LAUTTA_KEEP:-0}" = 1 ]; then
        echo "kept: $work"
    else
        rm -rf "$work" 2> /dev/null || echo "$prog: could not remove all of $work" >&2
    fi
}
trap cleanup EXIT
mkdir -p "$work/cmp/rebuilt"
n=0
for rpm in "$@"; do
    n=$((n + 1))
    mkdir "$work/cmp/$n"
    cp "$rpm" "$work/cmp/$n/"
done

# Fresh clone: nothing uncommitted gets in. Branch, tag or full hash in one
# shallow fetch; an abbreviated hash needs the history.
echo ">> fetching $ref from $repo"
src=$work/lautta
git init -q "$src"
if git -C "$src" fetch -q --depth 1 "$repo" "$ref" 2> /dev/null; then
    git -C "$src" checkout -q --detach FETCH_HEAD
else
    git -C "$src" fetch -q "$repo" '+refs/heads/*:refs/remotes/origin/*' '+refs/tags/*:refs/tags/*' ||
        fail "can't fetch from $repo"
    git -C "$src" checkout -q --detach "$ref^{commit}" 2> /dev/null || fail "no commit $ref in $repo"
fi
echo ">> building $(git -C "$src" log -1 --format='%h %s')"

# The commit's own recipe and pins (Makefile, rust-toolchain.toml), its own
# time: never the caller's.
unset SOURCE_DATE_EPOCH SDK_IMAGE TARGET SDK_CROSS_DIR
(cd "$src" && rustup toolchain install > /dev/null) || fail "can't install the commit's Rust toolchain"
make -C "$src" rpm || fail "the build failed"
cp "$src"/RPMS/*.rpm "$work/cmp/rebuilt/"
image=$(sed -n 's/^SDK_IMAGE ?= //p' "$src/Makefile")

echo
cp "$here/tools/compare-rpms.sh" "$work/cmp/"
chmod -R a+rX "$work/cmp"
status=0
docker run --rm --user 0 -v "$work/cmp:/cmp:ro" "$image" \
    sh -c 'sh /cmp/compare-rpms.sh /cmp/rebuilt /cmp/[0-9]*/*.rpm' || status=$?
echo
case "$status" in
    0) echo "VERIFIED: $ref rebuilds to every RPM checked, byte for byte." ;;
    1) echo "SAME FILES: contents match, packaging differs." ;;
    2) echo "DIFFERENT: see above." ;;
    *) echo "$prog: couldn't compare" >&2 ;;
esac
exit "$status"
