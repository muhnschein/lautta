# AGENTS.md

How to work on Lautta. What it is: `doc/SPEC.md` (requirement IDs such as `OPS-2`). Read `doc/CONTRIBUTING-CODE.md` before first change. User's words in chat beat this file.

## Ground rules

- Don't know how to proceed → say "I don't know", stop, ask. Mark guesses as guesses. Unknown beats confident wrong.
- Bugs: find root cause, state it. Symptom fix ≠ fix.
- Good return on cost. No gold-plating, no unasked refactors.
- Options → lay them out, recommend one with reasons. Implement nothing until user clearly picks.

## Done means

In this order; later step only once earlier ones hold:

1. Change implemented.
2. Tests added for it (regression test per bug).
3. Tests proven to bite: each fails with change reverted or mutated. Report how (e.g. "22/22 mutations caught"); name what headless tests can't prove. Change touches a mutated file (`make -s mutants-paths`) → `make mutants-diff MUTANTS_DIFF=<diff>` clean.
4. `make check` green locally.
5. PR CI green: CI (host checks, mutants on changed lines, RPM), SonarQube (quality gate, new-code coverage, zero open issues or hotspots: `tools/ci/sonar-issues.py`), CodeQL.
6. Device RPM: CI job "Sailfish OS 5.2 aarch64 RPM" builds it on every push; give user the `harbour-lautta-aarch64` artifact link of the green run. Needs newer netvfs → link those RPMs too, say install order.
7. With RPM: outline briefly what merging this PR risks and how you mitigated it.

Feature reachable from several places (folder pulley, item context menu, selection bar, viewers, cover, share target, folder picker) → all of them. Same for device folders and server locations; standalone mode shows no bridge traces (UI-8).

User tests RPM on phone; that test judges feel, lag, transitions. Say exactly what to check on device.

## Local checks

- Host deps (Ubuntu 24.04): `./tools/ci/install-host-deps.sh [coverage] [mutants] [cross]`: Qt 5 dev files, `lupdate`, pinned Rust 1.95; extras = pinned cargo-llvm-cov, cargo-mutants, `libc6-i386`.
- `make check` = rustfmt, catalogue drift, clippy (`clippy::all` denied), host tests incl. host QML (`tests/qml/host`: real Lautta types, offscreen, no Silica). Fake bridge (`lautta-bridge-proto` feature `fake`) stands in for netvfs.
- Page tests (`tests/qml/tst_*.qml`, real Silica, Qt 5.6) run only on SDK target: `make rpm && make check-rpm` (docker, `libc6-i386`; also Harbour validator, linked libs, installability) or `tools/sdk-qml-check.sh` against running SDK container. QML change → run one, or say only CI did.
- Missing tool (docker, cargo-mutants) → step skipped locally. Say which; CI runs all.
- Local failure → rerun on untouched `main` before calling it environment.
- Gates catch device-only breakage (Qt 5.6 QML, Harbour rules `HBR-*` of SPEC §3.2, linked libs, validator). Gate red → fix code, never loosen gate. New validator warning only if unavoidable, listed with reason in `doc/harbour-warnings.md`.
- Mutant exclusion (`.cargo/mutants.toml`) only with proof it changes no observable behaviour, written beside it.
- Tests wait on signals or conditions, not fixed sleeps or wall-clock asserts (those flake on loaded runners).
- New dependency must earn its keep: well maintained, widely used, more than someone's pet project, builds at Rust 1.95, pure Rust preferred, native links only to HBR-2 libraries (RS-2). Say why in PR. `Cargo.lock` via `cargo +stable update -p <crate>` (CONTRIBUTING-CODE.md "Toolchain").

## UI and words

- Silica stock components only (UI-1), built like existing pages; idioms in `design/STYLE.md` (pulley, no push-up menu, remorse, dialog pages). Jolla app has same feature (e.g. Gallery's image viewer) → match it. Feature references: SPEC Appendix A (Harbour File Browser, Transmit/Cyberduck, iOS Files).
- "Design first" / "no PR yet" → artboard on design canvas (`design/canvas/project/*.dc.html`, kit `design/STYLE.md`), wait for approval, then code.
- Every string `qsTrId()` with `//%` engineering English (UI-7); error text as SPEC §20. Changed strings → `make translations` (`make check` fails on drift).
- Host QML tests have no Silica; target page tests have no finger, no phone: looks and gestures unproven until device test. Say so.
- Phone = battery: no always-running animations, timers, polling. Idle in background = 0 % CPU, no timers (PRF-8); only a running transfer keeps device awake (XFR-5).

## Comments and docs

- Caveman-full = drop articles, filler, pleasantries, hedging. Fragments OK, short words. Identifiers, paths, code, errors exact. E.g. "`Cargo.lock` from stable cargo with fallback resolver: no crate newer than `rust-version` allows."
- This file: caveman-full.
- Code comments, `doc/*.md`: as existing ones (CONTRIBUTING-CODE.md): short plain prose, explain why, cite requirement IDs. SPDX header on every source file. Behaviour change → SPEC updated, same PR.
- No plans, research notes, scratch files in repo.

## Git and PRs

- One branch, one PR per task. Stacked PR only when asked.
- Bridge protocol (`org.netvfs.Bridge1`) change → PRs in lautta and netvfs together, lockstep. `tests/bridge-contract/` stays identical to netvfs `tests/bridge/contract/` (JSON, XML).
- Unrelated finding → tell user; separate PR or issue, not this PR.
- Commit and PR titles: Conventional Commits, plain language, say outcome: `fix(transfers): paste from SFTP to SMB no longer fails`. Types `feat` `fix` `perf` `refactor` `test` `docs` `ci` `build` `chore`; scope = app area or subsystem (`browse`, `directory`, `operations`, `transfers`, `viewers`, `search`, `bridge`, `rpm`, …). Body says why, plain prose; brief, to the point.
- PR body: the problem in a sentence or two, then how you fixed it. End with the model and harness that did the work.
- Final diff: no debug logging, no leftovers.
- Flaky test → find and fix cause. Never add retries: pass on second try = defect. Never re-run CI until green and call it done. Never skip test or loosen assert. One re-run OK only for infra failure before any test ran (runner lost, apt mirror timeout); say so.
- Merge only on user's explicit go. Merge commit, not squash/rebase.

## Releases

- Only on user's go. None cut yet: no tags, no changelog. Latest `main` commit green first, Sonar included: check main, not just PR.
- Strict semver: nothing breaking → minor.
- Harbour upload stays with user.

## Subagents

- Delegate only with tight, written scope.
- Models: only ones on cost/intelligence Pareto frontier ([Artificial Analysis](https://artificialanalysis.ai/), [Slopalytics](https://slopalytics.com/)). Dominated model = don't use. Cheapest frontier model that does the job well. If user names model for task, use that.
- Cheap-model output gets strong-model review: cheap models dropped facts, inverted conditions.

## Reporting

- Replies to user: caveman-full (see Comments and docs).
- Lead with result. Say what was verified, what wasn't, what is guess.
- Non-green step → name it and why. Never hide.
- Bug report → search GitHub issues first, link match.
