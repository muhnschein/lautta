// SPDX-License-Identifier: LGPL-2.1-or-later

pub mod app;
pub mod app_archives;
pub mod app_checksum;
pub mod app_directory;
pub mod app_home;
pub mod app_index;
pub mod app_operations;
pub mod app_pdf;
pub mod app_scan;
pub mod app_search;
pub mod app_transfers;
pub mod app_viewers;
pub mod db;
pub mod entry;
pub mod error;
pub mod fonts;
pub mod paths;
pub mod provider;
pub mod room;
pub mod sys;
pub mod uri;
pub mod vpath;

pub mod dircache;
pub mod filter;
pub mod foldercount;
pub mod home_layout;
pub mod listing;
pub mod locations;
pub mod mime;
pub mod sort;
pub mod viewprefs;
pub mod volumes;
pub mod watch;

pub mod bridge;

pub mod clipboard;
pub mod ops;
pub mod trash;

pub mod activity;
pub mod transfer;
pub mod workcopy;

pub mod checksum;
pub mod compress;
pub mod password;
pub mod pdf_pages;
pub mod preview;
pub mod scan;
pub mod thumbs;
pub mod thumbs_pdf;
pub mod thumbs_text;
pub(crate) mod zipaes;
pub(crate) mod zipcrypto;

pub mod index;
pub mod org;
pub mod search;

pub mod date_group;
pub mod messages;
pub mod panics;
pub mod powersave;
pub mod settings;

pub use error::{Error, ErrorKind, Result};
pub use uri::{LocationId, Uri};
pub use vpath::VPath;
