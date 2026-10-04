// SPDX-License-Identifier: LGPL-2.1-or-later
//! Viewers' data: text, Markdown, hex, SQLite, EXIF (PRV-4). Everything here
//! is pure or works on a [`crate::provider::ReadHandle`]; the Qt side only
//! renders the results.

pub mod exif;
pub mod hex;
pub mod markdown;
pub mod sqlite;
pub mod text;
