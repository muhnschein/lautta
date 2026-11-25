#!/bin/sh
# SPDX-License-Identifier: LGPL-2.1-or-later
set -eu

classify() {
    seen=0
    while IFS= read -r file; do
        seen=1
        case $file in
            tests/* | qml/* | third_party/* | doc/harbour-warnings.md) echo code=true; return ;;
            *.md) ;;
            *) echo code=true; return ;;
        esac
    done
    if [ "$seen" -eq 0 ]; then echo code=true; else echo code=false; fi
}

if [ "${1:-}" = --classify ]; then
    classify
    exit 0
fi

case ${EVENT:-} in
    pull_request) base=HEAD^1 ;;
    push) base=${BEFORE:-} ;;
    *)
        echo code=true
        exit 0
        ;;
esac

if ! files=$(git diff --name-only "$base" HEAD); then
    echo "changes: diff unreadable; running everything" >&2
    echo code=true
    exit 0
fi
printf '%s\n' "$files" | classify
