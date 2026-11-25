#!/bin/sh
# SPDX-License-Identifier: LGPL-2.1-or-later
set -u
script=$(cd "$(dirname "$0")" && pwd)/changes.sh
status=0
cases=0

check() {
    want=$1 got=$2 why=$3
    cases=$((cases + 1))
    if [ "$got" = "code=$want" ]; then
        echo "ok   $why"
    else
        echo "FAIL $why: got $got, wanted code=$want" >&2
        status=1
    fi
}

says() {
    want=$1 why=$2 files=$3
    check "$want" "$(printf '%s\n' "$files" | "$script" --classify)" "$why"
}

says false "docs alone skip" "doc/PROJECT.md
README.md"
says false "changelog alone skips" "CHANGELOG.md"
says false "agent rules alone skip" "AGENTS.md"
says true "Rust source runs" "crates/lautta-core/src/lib.rs"
says true "a page runs" "doc/PROJECT.md
qml/pages/DirectoryPage.qml"
says true "code after docs runs" "README.md
Makefile"
says true "a markdown fixture runs (host QML tests read it)" "tests/qml/host/fixtures/doc.md"
says true "a shipped QML folder's notes run" "qml/components/viewers/gallery/LAUTTA.md"
says true "third_party notes run" "third_party/qttypes/LAUTTA.md"
says true "validator warnings run (check-rpm.sh reads them)" "doc/harbour-warnings.md"
says true "a file only named like docs runs" "CHANGELOG.md.orig"
says true "an empty line runs" ""
check true "$("$script" --classify < /dev/null)" "no input runs"

work=$(mktemp -d)
trap 'rm -rf "$work"' EXIT
g() { git -C "$work" -c user.name=t -c user.email=t@t "$@" > /dev/null 2>&1; }
event() {
    want=$1 why=$2 name=$3 before=${4:-}
    check "$want" "$(cd "$work" && EVENT=$name BEFORE=$before "$script" 2> /dev/null)" "$why"
}
g init -q
echo a > "$work/main.rs"
g add . && g commit -qm code
event true "a first commit (no base) runs" pull_request
code=$(git -C "$work" rev-parse HEAD)
echo a > "$work/README.md"
g add . && g commit -qm docs
event false "a pull request with docs alone skips" pull_request
event false "a push with docs alone skips" push "$code"
event true "a push creating a branch runs" push 0000000000000000000000000000000000000000
event true "a push without its base runs" push
event true "a push from an unknown base runs" push 1234567890123456789012345678901234567890
event true "a manual run runs" workflow_dispatch "$code"
event true "a scheduled run runs" schedule "$code"
echo b > "$work/main.rs"
g add . && g commit -qm code
event true "a pull request with code runs" pull_request
echo b > "$work/README.md"
g add . && g commit -qm docs
event true "a push with code, then docs, runs" push "$code"

echo "$cases cases"
exit "$status"
