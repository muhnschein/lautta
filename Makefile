# SPDX-License-Identifier: LGPL-2.1-or-later
#
#   make check      format, lint (every crate, Qt glue included) and host tests
#   make test       host tests only
#   make coverage   host tests with coverage -> coverage/lcov.info (needs cargo-llvm-cov)
#   make mutants    mutation check: tests must catch changed code (needs cargo-mutants)
#   make clippy-report  clippy JSON for SonarCloud -> coverage/clippy.json
#   make vendor     vendor dependencies for the offline SDK build
#   make rpm        aarch64 RPM in the Sailfish SDK container (tools/ci/build-rpm.sh)
#
# The Qt crates need Qt 5 development files on the host (qmake in PATH).

CARGO ?= cargo
HOST_CRATES = -p lautta-core -p lautta-bridge-proto
SDK_IMAGE ?= mirror.gcr.io/coderus/sailfishos-platform-sdk-aarch64:5.2.0.15@sha256:e5f7596d14502746b308c3dde36f65fc25d7f76c45a01acaed56a1f55b76f592
TARGET ?= SailfishOS-5.2.0.15-aarch64
export SDK_IMAGE TARGET

.PHONY: check fmt-check clippy test coverage mutants clippy-report vendor rpm check-rpm clean

check: fmt-check clippy test

fmt-check:
	$(CARGO) fmt --all -- --check

clippy:
	$(CARGO) clippy --workspace --all-targets --all-features -- -D warnings

test:
	QT_QPA_PLATFORM=offscreen $(CARGO) test --workspace --all-features

coverage:
	mkdir -p coverage
	QT_QPA_PLATFORM=offscreen $(CARGO) llvm-cov --workspace --all-features --lcov --output-path coverage/lcov.info

clippy-report:
	mkdir -p coverage
	$(CARGO) clippy --workspace --all-targets --all-features --message-format=json > coverage/clippy.json

mutants:
	$(CARGO) mutants --no-shuffle -j 2 --timeout 120 \
		-p lautta-core -f 'crates/lautta-core/src/ops/*.rs' -f 'crates/lautta-core/src/transfer/*.rs' \
		-f crates/lautta-core/src/sort.rs -f crates/lautta-core/src/listing.rs -f crates/lautta-core/src/vpath.rs \
		-f crates/lautta-core/src/uri.rs

vendor:
	$(CARGO) +stable vendor --locked --versioned-dirs vendor > /dev/null

rpm:
	./tools/ci/build-rpm.sh

check-rpm:
	./tools/ci/check-rpm.sh

clean:
	$(CARGO) clean
	rm -rf coverage mutants.out* RPMS vendor .cargo-home
