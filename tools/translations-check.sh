#!/bin/sh
# SPDX-License-Identifier: LGPL-2.1-or-later
set -eu
root=$(cd "$(dirname "$0")/.." && pwd)
work=$(mktemp -d)
trap 'rm -rf "$work"' EXIT
cp "$root"/translations/*.ts "$work/"
(cd "$root" && ${LUPDATE:-lupdate -silent -locations none -no-obsolete} qml -ts "$work"/*.ts)
python3 "$root/tools/english-plurals.py" fill < "$work/harbour-lautta.ts" > "$work/filled"
mv "$work/filled" "$work/harbour-lautta.ts"
status=0
for catalogue in "$root"/translations/*.ts; do
    diff -u "$catalogue" "$work/$(basename "$catalogue")" || status=1
done
if [ "$status" -ne 0 ]; then
    echo "translations-check: catalogues out of date: run make translations" >&2
    exit 1
fi
for catalogue in "$work"/*.ts; do
    if ! out=$("${LRELEASE:-lrelease}" -idbased -silent "$catalogue" -qm "${catalogue%.ts}.qm" 2>&1) || [ -n "$out" ]; then
        echo "$(basename "$catalogue"): $out" >&2
        status=1
    fi
done
if [ "$status" -ne 0 ]; then
    echo "translations-check: lrelease fails or warns: fix the catalogue" >&2
    exit 1
fi
