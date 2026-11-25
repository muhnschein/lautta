# SPDX-License-Identifier: LGPL-2.1-or-later

CARGO ?= cargo
HOST_FEATURES = --features lautta-bridge-proto/fake
SDK_IMAGE ?= mirror.gcr.io/coderus/sailfishos-platform-sdk-aarch64:5.2.0.15@sha256:e5f7596d14502746b308c3dde36f65fc25d7f76c45a01acaed56a1f55b76f592
TARGET ?= SailfishOS-5.2.0.15-aarch64
export SDK_IMAGE TARGET

LUPDATE ?= lupdate
LUPDATE_ARGS = -silent -locations none -no-obsolete -target-language en

.PHONY: translations translations-check comments-check check fmt-check clippy test coverage mutants mutants-diff mutants-paths clippy-report rpm check-rpm clean

check: fmt-check comments-check translations-check clippy test

comments-check:
	python3 tools/no_comments_test.py
	python3 tools/no_comments.py

fmt-check:
	$(CARGO) fmt --all -- --check

clippy:
	$(CARGO) clippy --workspace --all-targets $(HOST_FEATURES) -- -D warnings

test:
	@if command -v cargo-nextest >/dev/null 2>&1; then \
		QT_QPA_PLATFORM=offscreen $(CARGO) nextest run --workspace $(HOST_FEATURES); \
	else \
		echo "test: cargo-nextest not installed, running cargo test (install-host-deps.sh nextest)"; \
		QT_QPA_PLATFORM=offscreen $(CARGO) test --workspace $(HOST_FEATURES); \
	fi
	$(CARGO) test --workspace --doc $(HOST_FEATURES)

coverage:
	mkdir -p coverage
	QT_QPA_PLATFORM=offscreen $(CARGO) llvm-cov --workspace $(HOST_FEATURES) --lcov --output-path coverage/lcov.info
	python3 -m coverage run --data-file=coverage/python.data --include='tools/*' tools/no_comments_test.py
	python3 -m coverage xml --data-file=coverage/python.data -o coverage/python.xml

clippy-report:
	mkdir -p coverage
	$(CARGO) clippy --workspace --all-targets $(HOST_FEATURES) --message-format=json > coverage/clippy.json

MUTANTS_SHARD ?= 0/1
MUTANTS_DIFF ?= mutants.diff
MUTANTS_PATHS = crates/lautta-core/src/ops/plan.rs crates/lautta-core/src/ops/conflict.rs \
	crates/lautta-core/src/transfer/scheduler.rs crates/lautta-core/src/transfer/model.rs \
	crates/lautta-core/src/transfer/store.rs crates/lautta-core/src/transfer/conflict.rs \
	crates/lautta-core/src/activity.rs crates/lautta-core/src/app_pdf.rs \
	crates/lautta-pdf/src/lib.rs crates/lautta-pdf/src/text.rs crates/lautta-pdf/src/nav.rs
MUTANTS_ARGS = --no-shuffle -j 2 --timeout 300 -p lautta-core -p lautta-pdf $(addprefix -f ,$(MUTANTS_PATHS))

mutants:
	$(CARGO) mutants $(MUTANTS_ARGS) --shard $(MUTANTS_SHARD)

mutants-diff:
	$(CARGO) mutants $(MUTANTS_ARGS) --in-diff $(MUTANTS_DIFF)

mutants-paths:
	@echo $(MUTANTS_PATHS)

rpm:
	./tools/ci/build-rpm.sh

check-rpm:
	./tools/ci/check-rpm.sh

translations:
	$(LUPDATE) $(LUPDATE_ARGS) qml -ts translations/harbour-lautta.ts
	python3 tools/english-plurals.py fill < translations/harbour-lautta.ts > $${TMPDIR:-/tmp}/lautta-ts-filled.ts
	mv $${TMPDIR:-/tmp}/lautta-ts-filled.ts translations/harbour-lautta.ts

translations-check:
	cp translations/harbour-lautta.ts $${TMPDIR:-/tmp}/lautta-ts-check.ts
	$(LUPDATE) $(LUPDATE_ARGS) qml -ts $${TMPDIR:-/tmp}/lautta-ts-check.ts
	python3 tools/english-plurals.py fill < $${TMPDIR:-/tmp}/lautta-ts-check.ts > $${TMPDIR:-/tmp}/lautta-ts-checked.ts
	diff -u translations/harbour-lautta.ts $${TMPDIR:-/tmp}/lautta-ts-checked.ts

clean:
	$(CARGO) clean
	rm -rf coverage mutants.out* RPMS prebuilt build-sdk-cross
