#!/bin/sh
# SPDX-License-Identifier: LGPL-2.1-or-later
# Does this change need the build and test jobs? Prints `code=true` or
# `code=false` for $GITHUB_OUTPUT (`changes` job in ci.yml, build.yml).
#
# false only for a pull request touching nothing but `*.md` outside what
# builds, tests or packaging read: tests/ (markdown fixtures), qml/ and
# third_party/ (shipped, built), doc/harbour-warnings.md (check-rpm.sh).
# Push to main, dispatch, unreadable diff: true. Running too much costs
# minutes; skipping wrongly costs a red main.
#
# Usage:  changes.sh             (pull request: merge commit against its base)
#         changes.sh --classify  (file names on stdin; tools/ci/changes-test.sh)
# Env: EVENT = github.event_name.
set -eu

# Nothing named = true: an empty diff proves nothing.
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

if [ "${EVENT:-}" != pull_request ]; then
    echo code=true
    exit 0
fi

# Pull request checkout = merge commit: first parent is the base.
if ! files=$(git diff --name-only HEAD^1 HEAD); then
    echo "changes: diff unreadable; running everything" >&2
    echo code=true
    exit 0
fi
printf '%s\n' "$files" | classify
