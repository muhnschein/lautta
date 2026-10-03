# SPDX-License-Identifier: LGPL-2.1-or-later
#
#   make check     format, lint and host tests (what CI's "check" job runs)
#   make test      host tests only
#   make coverage  host tests with coverage (lcov at coverage/lcov.info)

CARGO ?= cargo
HOST_CRATES = -p lautta-core -p lautta-bridge-proto

.PHONY: check fmt-check clippy test coverage clean

check: fmt-check clippy test

fmt-check:
	$(CARGO) fmt --all -- --check

clippy:
	$(CARGO) clippy $(HOST_CRATES) --all-targets --all-features -- -D warnings

test:
	$(CARGO) test $(HOST_CRATES) --all-features

clean:
	$(CARGO) clean
