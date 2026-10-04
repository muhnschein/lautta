# Code conventions

The normative requirements are in [`SPEC.md`](SPEC.md) (requirement IDs such as `OPS-2`).
The UI/UX is the design canvas in [`../design`](../design) (one `.dc.html` artboard per
screen; `design/STYLE.md` is the Silica style kit).

## Toolchain

- Rust **1.75** everywhere (`rust-toolchain.toml`): the Sailfish OS 5.2 SDK target ships
  rustc 1.75, so nothing newer may be used (no `let … else` is fine, it is 1.65; no
  `LazyLock`, no `#[diagnostic]`, no inline `const {}` blocks, no async fn in traits — use
  `async-trait`).
- `Cargo.lock` is generated with a current stable cargo honouring
  `.cargo/config.toml`'s `incompatible-rust-versions = "fallback"`:
  `cargo +stable update -p <crate>` / `cargo +stable generate-lockfile`. Never let cargo
  1.75 re-resolve (it would pick crates needing newer Rust).
- New dependencies: pure Rust preferred (SPEC RS-2). Native links only to HBR-2 libraries.

## Rules

- `make check` must pass: `cargo fmt --check`, clippy with every lint in `clippy::all`
  denied, host tests.
- No `unwrap()`/`expect()`/`panic!` outside tests unless a comment proves it cannot fire.
  Return `lautta_core::Error` (`crate::error`) with the right `ErrorKind`.
- `unsafe` only in `lautta-core::sys` and `lautta-qt` (SPEC RS-5).
- Every module has unit tests in `#[cfg(test)] mod tests`; behaviour across modules goes in
  `crates/<crate>/tests/`. Tests must *bite*: each asserted behaviour must fail when the
  code is broken (CI runs `cargo-mutants` on core modules). Aim for ≥ 90 % line coverage
  of new code (SonarCloud gates on 80 %).
- SonarCloud is gating: keep functions short (cognitive complexity ≤ 15), no duplicated
  blocks, no commented-out code, no TODO comments, no empty functions without a comment,
  no `String` concatenation in loops where `format!`/`push_str` is meant, etc.
- Paths and names are bytes (`VPath`, `Vec<u8>`); display with lossy decoding only at the
  edge. Never log secrets or file contents; paths and host names only at `debug` (SEC-6).
- Blocking file system calls run in `tokio::task::spawn_blocking` (ARC-6). Database access
  goes through `crate::db::Db` (a `Mutex<rusqlite::Connection>`), only from blocking
  contexts.
- SPDX header `// SPDX-License-Identifier: LGPL-2.1-or-later` on every source file.
- Comments explain *why*; cite requirement IDs where a rule comes from the spec.

## Layout

```
crates/lautta-core           no Qt; providers, operations, transfers, persistence
crates/lautta-bridge-proto   org.netvfs.Bridge1 types + zbus proxy + fake bridge (feature "fake")
crates/lautta-qt             qmetaobject models/facades, C++ glue (SailfishApp, MediaSource)
crates/harbour-lautta        the binary (exported C main, SPEC RS-3)
qml/                         pages/, components/, dialogs/, viewers/, cover/
tests/bridge-contract        netvfs's protocol contract (copied from netvfs f94006e)
```

Shared core types: `Error`/`ErrorKind` (error.rs), `VPath` (vpath.rs), `Entry`/`Kind`/
`EntryFlags`/`Capabilities`/`cap::*` (entry.rs), `Uri`/`LocationId` (uri.rs), `Db` (db.rs,
schema DAT-2), `AppPaths` (paths.rs), `Provider`/`ReadHandle`/`ProviderResolver`
(provider/mod.rs), `Plan`/`PlanItem`/`Conflict`/`ConflictChoice`/`OperationKind`
(ops/mod.rs).
