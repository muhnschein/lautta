#!/bin/sh
# SPDX-License-Identifier: LGPL-2.1-or-later
set -u
script=$(cd "$(dirname "$0")/.." && pwd)/verify-rpm.sh
status=0

prints_usage() {
    why=$1
    shift
    got=$(sh "$script" "$@" 2>&1 > /dev/null)
    code=$?
    first=$(printf '%s\n' "$got" | head -n 1)
    if [ "$code" -eq 3 ] && [ "$first" = "usage: tools/verify-rpm.sh --ref <ref> <rpm>..." ] &&
        printf '%s\n' "$got" | grep -q -- '--ref <ref>  commit' &&
        printf '%s\n' "$got" | grep -q '<rpm>        packages' &&
        ! printf '%s\n' "$got" | grep -qF '$'; then
        echo "ok   $why"
    else
        echo "FAIL $why: exit $code, printed:" >&2
        printf '%s\n' "$got" >&2
        status=1
    fi
}

prints_usage "--help prints usage" --help
prints_usage "no arguments print usage"
prints_usage "--ref without RPMs prints usage" --ref main
prints_usage "--ref without value prints usage" --ref

exit "$status"
