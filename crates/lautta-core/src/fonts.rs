// SPDX-License-Identifier: LGPL-2.1-or-later
//! Bundled fonts (`fonts/`, shipped in `/usr/share/harbour-lautta/fonts/`).

/// Family every monospace text asks for: QML via `App.monoFont`, rich text via `font-family`.
/// Must match name table of bundled file; target and host QML tests check it after loading.
pub const MONO_FAMILY: &str = "JetBrains Mono";
