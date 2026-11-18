#!/bin/sh
# SPDX-License-Identifier: LGPL-2.1-or-later
# Does this change need the build and test jobs? Prints `code=true` or
# `code=false` for $GITHUB_OUTPUT (`changes` job in ci.yml, build.yml, codeql.yml).
#
# false only for a pull request or push touching nothing but `*.md` outside what
# builds, tests or packaging read: tests/ (markdown fixtures), qml/ and
# third_party/ (shipped, built), doc/harbour-warnings.md (check-rpm.sh).
# Dispatch, schedule, new branch, unreadable diff: true. Running too much costs
# minutes; skipping wrongly costs a red main.
#
# Usage:  changes.sh             (pull request: merge commit against its base;
#                                 push: BEFORE against HEAD)
#         changes.sh --classify  (file names on stdin; tools/ci/changes-test.sh)
# Env: EVENT = github.event_name, BEFORE = github.event.before (push).
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

case ${EVENT:-} in
    # Pull request checkout = merge commit: first parent is the base.
    pull_request) base=HEAD^1 ;;
    # New branch: all zeros, diff unreadable.
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
