// SPDX-License-Identifier: LGPL-2.1-or-later
//! `lautta-core`: everything below the UI (SPEC ARC-2). No Qt dependency;
//! testable on the host with `cargo test`. Unsafe code is denied crate-wide
//! (workspace lint) and allowed only in [`sys`] (SPEC RS-5).

pub mod app;
pub mod db;
pub mod entry;
pub mod error;
pub mod paths;
pub mod provider;
pub mod sys;
pub mod uri;
pub mod vpath;

// Locations and browsing
pub mod dircache;
pub mod filter;
pub mod listing;
pub mod locations;
pub mod mime;
pub mod sort;
pub mod viewprefs;
pub mod volumes;
pub mod watch;

// netvfs bridge
pub mod bridge;
pub mod questions;

// Operations
pub mod clipboard;
pub mod ops;
pub mod trash;
pub mod undo;

// Transfers and edit-in-place
pub mod transfer;
pub mod workcopy;

// Preview
pub mod compress;
pub mod preview;
pub mod thumbs;

// Search, sync, organisation
pub mod compare;
pub mod org;
pub mod search;

// App-wide
pub mod crash;
pub mod messages;
pub mod settings;

pub use error::{Error, ErrorKind, Result};
pub use uri::{LocationId, Uri};
pub use vpath::VPath;
