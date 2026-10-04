# SPDX-License-Identifier: LGPL-2.1-or-later
#
#   make check      format, lint (every crate, Qt glue included) and host tests
#   make test       host tests only
#   make coverage   host tests with coverage -> coverage/lcov.info (needs cargo-llvm-cov)
#   make mutants    mutation check: tests must catch changed code (needs cargo-mutants)
#   make clippy-report  clippy JSON for SonarCloud -> coverage/clippy.json
#   make translations  refresh translations/harbour-lautta.ts from the QML (qsTrId ids)
#   make vendor     vendor dependencies for the offline SDK build
#   make rpm        aarch64 RPM in the Sailfish SDK container (tools/ci/build-rpm.sh)
#
# The Qt crates need Qt 5 development files on the host (qmake in PATH).

CARGO ?= cargo
HOST_FEATURES = --features lautta-bridge-proto/fake
# The "sailfish" feature needs libsailfishapp and is only built in the SDK.
SDK_IMAGE ?= mirror.gcr.io/coderus/sailfishos-platform-sdk-aarch64:5.2.0.15@sha256:e5f7596d14502746b308c3dde36f65fc25d7f76c45a01acaed56a1f55b76f592
TARGET ?= SailfishOS-5.2.0.15-aarch64
export SDK_IMAGE TARGET

LUPDATE ?= lupdate

.PHONY: translations translations-check check fmt-check clippy test coverage mutants clippy-report vendor rpm check-rpm clean

check: fmt-check translations-check clippy test

fmt-check:
	$(CARGO) fmt --all -- --check

clippy:
	$(CARGO) clippy --workspace --all-targets $(HOST_FEATURES) -- -D warnings

test:
	QT_QPA_PLATFORM=offscreen $(CARGO) test --workspace $(HOST_FEATURES)

coverage:
	mkdir -p coverage
	QT_QPA_PLATFORM=offscreen $(CARGO) llvm-cov --workspace $(HOST_FEATURES) --lcov --output-path coverage/lcov.info

clippy-report:
	mkdir -p coverage
	$(CARGO) clippy --workspace --all-targets $(HOST_FEATURES) --message-format=json > coverage/clippy.json

# SPEC TST-5: planner, queue and conflict modules. MUTANTS_SHARD=k/n splits
# the run (CI runs shards in parallel).
MUTANTS_SHARD ?= 0/1
MUTANTS_FILES = -f crates/lautta-core/src/ops/plan.rs -f crates/lautta-core/src/ops/conflict.rs \
	-f crates/lautta-core/src/transfer/scheduler.rs -f crates/lautta-core/src/transfer/model.rs \
	-f crates/lautta-core/src/transfer/store.rs -f crates/lautta-core/src/transfer/conflict.rs

mutants:
	$(CARGO) mutants --no-shuffle -j 2 --timeout 300 --shard $(MUTANTS_SHARD) -p lautta-core $(MUTANTS_FILES)

vendor:
	$(CARGO) +stable vendor --locked --versioned-dirs vendor > /dev/null

rpm:
	./tools/ci/build-rpm.sh

check-rpm:
	./tools/ci/check-rpm.sh

translations:
	$(LUPDATE) -silent -locations none -no-obsolete qml -ts translations/harbour-lautta.ts

# The engineering English catalogue must match the QML (UI-7).
translations-check:
	cp translations/harbour-lautta.ts $${TMPDIR:-/tmp}/lautta-ts-check.ts
	$(LUPDATE) -silent -locations none -no-obsolete qml -ts $${TMPDIR:-/tmp}/lautta-ts-check.ts
	diff -u translations/harbour-lautta.ts $${TMPDIR:-/tmp}/lautta-ts-check.ts

clean:
	$(CARGO) clean
	rm -rf coverage mutants.out* RPMS vendor .cargo-home
