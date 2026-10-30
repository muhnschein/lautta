// SPDX-License-Identifier: LGPL-2.1-or-later
//! `lautta-core`: everything below the UI. No Qt dependency. Unsafe code is denied crate-wide and
//! allowed only in [`sys`].

pub mod app;
pub mod app_browse;
pub mod app_directory;
pub mod app_operations;
pub mod app_pdf;
pub mod app_search;
pub mod app_transfers;
pub mod app_viewers;
pub mod db;
pub mod entry;
pub mod error;
pub mod paths;
pub mod provider;
pub mod sys;
pub mod uri;
pub mod vpath;

pub mod browse_layout;
pub mod dircache;
pub mod filter;
pub mod foldercount;
pub mod listing;
pub mod locations;
pub mod mime;
pub mod sort;
pub mod viewprefs;
pub mod volumes;
pub mod watch;

pub mod bridge;
pub mod questions;

pub mod clipboard;
pub mod ops;
pub mod trash;

pub mod activity;
pub mod transfer;
pub mod workcopy;

pub mod compress;
pub mod preview;
pub mod thumbs;
pub mod thumbs_pdf;
pub mod thumbs_text;

pub mod org;
pub mod search;

pub mod date_group;
pub mod messages;
pub mod panics;
pub mod settings;

pub use error::{Error, ErrorKind, Result};
pub use uri::{LocationId, Uri};
pub use vpath::VPath;
