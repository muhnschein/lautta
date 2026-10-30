# Code conventions

What Lautta is: `PROJECT.md`. UI: design canvas in `../design` (one `.dc.html` artboard per screen; `design/STYLE.md` = Silica style kit).

## Toolchain

- Rust **1.95** everywhere (`rust-toolchain.toml`). 5.2 SDK target ships rustc 1.75, so RPM's binary is cross-built with host cargo against SDK's gcc, Qt, sysroot (`tools/ci/lift-sdk.sh`, `tools/ci/cross-build.sh`); `mb2` only packages it.
- `Cargo.lock` from current stable cargo honouring `.cargo/config.toml`'s `incompatible-rust-versions = "fallback"`: `cargo +stable update -p <crate>` / `cargo +stable generate-lockfile`. No crate newer than workspace `rust-version` allows.
- New dependencies: pure Rust preferred; native links only to Harbour's allowed libraries (`PROJECT.md` "Platform").

## Rules

- `make check` passes: `cargo fmt --check`, clippy with all of `clippy::all` denied, host tests.
- No `unwrap()`/`expect()`/`panic!` outside tests unless comment proves it cannot fire. Return `lautta_core::Error` (`crate::error`) with right `ErrorKind`.
- `unsafe` only in `lautta-core::sys` and `lautta-qt`.
- Unit tests in `#[cfg(test)] mod tests`; cross-module behaviour in `crates/<crate>/tests/`. Tests bite: CI runs `cargo-mutants` on core modules (changed lines per PR, all weekly; `make mutants-diff MUTANTS_DIFF=<file>` locally). New code ≥ 90 % line coverage (Sonar gates 80 %).
- Sonar gates: cognitive complexity ≤ 15, no duplicated blocks, no commented-out code, no TODO, no empty function without comment.
- CodeQL (`codeql.yml`) scans shipped code only: no `cfg(test)`, default features, no `crates/*/tests`, no `third_party` (`.github/codeql/codeql-config.yml`, `codeql-config-rust.yml`). `rust/access-invalid-pointer` is our copy (`.github/codeql/rust-queries`): silent on `#[derive(QObject)]` signal offsets (`&(*null).field`, reads no memory); its query test (`rust-queries-tests`, planted bugs) runs in same job. Finding → fix, or dismiss with reason.
- Paths and names are bytes (`VPath`, `Vec<u8>`); lossy decoding only at display edge. Never log secrets or file contents; paths and host names only at `debug`.
- Blocking file system calls in `tokio::task::spawn_blocking`. Database only through `crate::db::Db` (`Mutex<rusqlite::Connection>`), only from blocking contexts.
- SPDX `// SPDX-License-Identifier: LGPL-2.1-or-later` on every source file. Third-party code keeps own licence and headers: `third_party/` (crates), `qml/components/viewers/gallery/` (Jolla Gallery viewer, BSD-3-Clause); `LAUTTA.md` beside it says origin and changes.
- Comments: AGENTS.md "Comments and docs".
- Strings: `qsTrId("lautta-…")` with `//%` engineering English. Count in text = one plural string with `%n`, called `qsTrId(id, n)`: never "(s)", never separate id for "1 item". `make translations` refreshes `translations/harbour-lautta.ts`, adds English singular of new plurals: read printed pairs, write singular by hand where it cannot tell. `make check` fails on gap.

## Layout

```
crates/lautta-core           no Qt; providers, operations, transfers, persistence
crates/lautta-bridge-proto   org.netvfs.Bridge1 types + zbus proxy + fake bridge (feature "fake")
crates/lautta-pdf            PDF parsing, rendering, text (hayro)
crates/lautta-qt             qmetaobject models/facades, C++ glue (SailfishApp, MediaSource)
crates/harbour-lautta        binary (exported C main)
qml/                         pages/, components/, dialogs/, viewers/, cover/
tests/bridge-contract        netvfs's protocol contract, kept identical to netvfs
```

Shared core types: `Error`/`ErrorKind` (error.rs), `VPath` (vpath.rs), `Entry`/`Kind`/`EntryFlags`/`Capabilities`/`cap::*` (entry.rs), `Uri`/`LocationId` (uri.rs), `Db` (db.rs), `AppPaths` (paths.rs), `Provider`/`ReadHandle`/`ProviderResolver` (provider/mod.rs), `Plan`/`PlanItem`/`Conflict`/`ConflictChoice`/`OperationKind` (ops/mod.rs).
