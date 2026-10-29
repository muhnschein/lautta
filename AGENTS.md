# AGENTS.md

How to work on Lautta. What it is: `doc/SPEC.md` (requirement IDs such as `OPS-2`). Read `doc/CONTRIBUTING-CODE.md` before first change. User's words in chat beat this file.

## Ground rules

- Don't know how to proceed → say "I don't know", stop, ask. Mark guesses as guesses. Unknown beats confident wrong.
- Bugs: find root cause, state it. Symptom fix ≠ fix.
- Measure twice, cut once. YAGNI: fight scope creep; meet user's intent minimally, realistically. Good return on cost; no gold-plating, no unasked refactors.
- Rule here fights task → say so loudly, get user's sign-off before breaking it.
- Options → lay them out, recommend one with reasons. Implement nothing until user clearly picks.

## Done means

In this order; later step only once earlier ones hold:

1. Change implemented.
2. Tests added for it (regression test per bug).
3. Tests proven to bite: each fails with change reverted or mutated. Report how (e.g. "22/22 mutations caught"); name what headless tests can't prove. Change touches a mutated file (`make -s mutants-paths`) → `make mutants-diff MUTANTS_DIFF=<diff>` clean.
4. `make check` green locally.
5. PR CI green: CI (host checks, mutants on changed lines, RPM), SonarQube (quality gate, new-code coverage, zero open issues or hotspots: `tools/ci/sonar-issues.py`), CodeQL.
6. Device: CI job "Sailfish OS 5.2 aarch64 RPM" builds it on every push; run that RPM on test phone (see "Device tests"); give user the `harbour-lautta-aarch64` artifact link of the green run and the evidence. Needs newer netvfs → link those RPMs too, say install order.
7. With RPM: outline briefly what merging this PR risks and how you mitigated it.

PR opens as draft on first push. Mark ready for review only once steps 1–7 hold and their evidence is in PR body.

Feature reachable from several places (folder pulley, item context menu, selection bar, viewers, cover, share target, folder picker) → all of them. Same for device folders and server locations; standalone mode shows no bridge traces (UI-8).

User tests RPM on phone; that test judges feel, lag, transitions. Say exactly what to check on device.

## Device tests

- Test phone: OnePlus 6, SFOS 5.1, via `muhnschein-sfos-harness` workflow `do`; lautta's caller notes: its `apps/lautta.md`. Its AGENTS.md rules every phone run (baseline, no blind taps, reboot, language, `clean_slate`); helpers: its README, `scripts/lib.sh`. Test accounts and test servers only. Never run fork or others' PR code there: any job can root phone.
- PR build onto phone: `install_artifact muhnschein/lautta RUN_ID harbour-lautta`, RUN_ID of this PR's green CI run.
- Every run starts from named state; `state.txt` records it plus RPM name and sha256:
  - `clean`: `reset`: quit app, drop sailjail grant, `rpm -e`, reset dconf `/apps/harbour-lautta/`, delete `~/.local/share/org.netvfs/lautta`, `~/.config/org.netvfs/lautta`, `~/.cache/org.netvfs/lautta` (and unsandboxed `harbour-lautta` dirs); install. Standalone: no netvfs on phone (UI-8).
  - `bridge`: `clean` plus netvfs RPMs (netvfs first), for server locations and bridge paths.
  - `fixture:NAME`: `clean`, then restore data snapshot (dirs plus dconf) saved with `fixture_save`.
  - `upgrade:FROM`: release FROM plus fixture, then install build over it. Required when change touches stored data (`lautta.db`, trash) or settings.
- Test files in user dirs (Documents, Downloads, Pictures): own subfolder, listed in harness `apps/harbour-lautta.dirs`, so `reset` removes them.
- Footprint check (`fs_mark`, `footprint`): file app touched outside known dirs → add to reset or fail run. Leftovers taint results.
- Before/after: same start state, same steps.
- Motion or timing change: launch with `FRAMELOG`, report `frames` (fps, slow frames, worst gap per animation). Never measure frames while recording: recorder slows compositor.
- Privacy: user stays muhnschein, nothing more. Harness results come scrubbed (no location, addresses, device or network ids, server names; its AGENTS.md "Privacy"). PR evidence only from those files (crops, GIFs, charts made from them OK); never type identifiers in. No passwords, login links, QR codes. Location tests: harness fake places only, never real fix.
- Harness proves function and frame timing. Feel, lag, transitions stay user's call on device.

## Local checks

- Host deps (Ubuntu 24.04): `./tools/ci/install-host-deps.sh [coverage] [mutants] [cross]`: Qt 5 dev files, `lupdate`, pinned Rust 1.95; extras = pinned cargo-llvm-cov, cargo-mutants, `libc6-i386`.
- `make check` = rustfmt, catalogue drift, clippy (`clippy::all` denied), host tests incl. host QML (`tests/qml/host`: real Lautta types, offscreen, no Silica). Fake bridge (`lautta-bridge-proto` feature `fake`) stands in for netvfs.
- Page tests (`tests/qml/tst_*.qml`, real Silica, Qt 5.6) run only on SDK target: `make rpm && make check-rpm` (docker, `libc6-i386`; also Harbour validator, linked libs, installability) or `tools/sdk-qml-check.sh` against running SDK container. QML change → run one, or say only CI did.
- Missing tool (docker, cargo-mutants) → step skipped locally. Say which; CI runs all.
- Local failure → rerun on untouched `main` before calling it environment.
- Iterating → run only what change touches: `cargo test -p lautta-core <filter>`, host QML `cargo test -p lautta-qt --features lautta-bridge-proto/fake --test qml_host`, page tests `tools/sdk-qml-check.sh tests/qml/tst_<page>.qml`. Before every push: full `make check`. CI runs everything (docs-only PR: no build or test jobs, `tools/ci/changes.sh`).
- Gates catch device-only breakage (Qt 5.6 QML, Harbour rules `HBR-*` of SPEC §3.2, linked libs, validator). Gate red → fix code, never loosen gate. New validator warning only if unavoidable, listed with reason in `doc/harbour-warnings.md`.
- Mutant exclusion (`.cargo/mutants.toml`) only with proof it changes no observable behaviour, written beside it.
- Tests wait on signals or conditions, not fixed sleeps or wall-clock asserts (those flake on loaded runners).
- New dependency must earn its keep: well maintained, widely used, more than someone's pet project, builds at Rust 1.95, pure Rust preferred, native links only to HBR-2 libraries (RS-2). Say why in PR. `Cargo.lock` via `cargo +stable update -p <crate>` (CONTRIBUTING-CODE.md "Toolchain").

## UI and words

- Silica stock components only (UI-1), built like existing pages; idioms in `design/STYLE.md` (pulley, no push-up menu, remorse, dialog pages). Jolla app has same feature (e.g. Gallery's image viewer) → match it. Feature references: SPEC Appendix A (Harbour File Browser, Transmit/Cyberduck, iOS Files).
- "Design first" / "no PR yet" → artboard on design canvas (`design/canvas/project/*.dc.html`, kit `design/STYLE.md`), wait for approval, then code.
- Every string `qsTrId()` with `//%` engineering English (UI-7); error text as SPEC §20. Changed strings → `make translations` (`make check` fails on drift). A count in a text → one `%n` plural string, never "(s)"; `make translations` adds its English singular (read the printed pairs).
- Host QML tests have no Silica; target page tests have no finger, no phone: looks and gestures unproven until device test. Say so.
- Phone = battery: no always-running animations, timers, polling. Idle in background = 0 % CPU, no timers (PRF-8); only a running transfer keeps device awake (XFR-5).

## Comments and docs

- Caveman-full = drop articles, filler, pleasantries, hedging. Fragments OK, short words. Identifiers, paths, code, errors exact. E.g. "`Cargo.lock` from stable cargo with fallback resolver: no crate newer than `rust-version` allows."
- Code comments, `doc/*.md`, this file: caveman-full. What is true now and why; keep constraints; cite requirement IDs. No changelog, bug report, history, story (git's job): delete comment rather than turn it into history. Existing prose: rewrite what you touch, not more.
- Default: no comment. One only for constraint, external quirk, invariant code can't show; one line where possible, four max. Applies to new and touched code; trim old long comments when changing that code anyway, no sweep.
- SPDX header on every source file. Behaviour change → SPEC updated, same PR.
- `README.md`, `CHANGELOG.md`, release notes, store and forum posts: plain prose for users, as short and to the point as possible.
- User-visible change → line in `CHANGELOG.md` under `## Unreleased` (file or section missing → create it), same PR.
- No plans, research notes, scratch files in repo.

## Git and PRs

- One branch, one PR per task. Stacked PR only when asked.
- Bridge protocol (`org.netvfs.Bridge1`) change → PRs in lautta and netvfs together, lockstep. `tests/bridge-contract/` stays identical to netvfs `tests/bridge/contract/` (JSON, XML).
- Unrelated finding → tell user; separate PR or issue, not this PR.
- Commit and PR titles: Conventional Commits, plain language, say outcome: `fix(transfers): paste from SFTP to SMB no longer fails`. Types `feat` `fix` `perf` `refactor` `test` `docs` `ci` `build` `chore`; scope = app area or subsystem (`browse`, `directory`, `operations`, `transfers`, `viewers`, `search`, `bridge`, `rpm`, …). Body says why, plain prose; brief, to the point.
- PR body: the problem in a sentence or two, then how you fixed it. End with the model and harness that did the work.
- Final diff: no debug logging, no leftovers.
- Evidence shown in PR body, not just described:
  - UI → before/after device screenshots.
  - Transitions, motion, feel, lag → before/after GIF (preferred: plays inline; mp4 by URL shows as link only) or mp4. GIF from harness mp4 via `ffmpeg`.
  - Performance → before/after chart (image), numbers in table beside it. Frame timing from separate `frames` run: recording slows compositor.
  - Non-visual proof (tests, mutations, logs) → text.
- Evidence on GitHub, never in tree: push to orphan branch `evidence/pr-N`, embed by URL in PR, delete branch after merge. No PR-only screenshots or assets in `main` or PR branch.
- Flaky test → find and fix cause. Never add retries: pass on second try = defect. Never re-run CI until green and call it done. Never skip test or loosen assert. One re-run OK only for infra failure before any test ran (runner lost, apt mirror timeout); say so.
- Merge only on user's explicit go. Merge commit, not squash/rebase.
- Before asking for merge go: `main` moved since PR's last green run → merge `main` in, CI green again. Each PR green alone ≠ merge green.

## Releases

- Only on user's go. None cut yet, no tags. Latest `main` commit green first, Sonar included: check main, not just PR.
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
