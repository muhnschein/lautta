// SPDX-License-Identifier: LGPL-2.1-or-later
//! netvfs bridge client. Requests go through [`crate::provider::netvfs::NetvfsProvider`].

mod client;
pub(crate) mod convert;
pub(crate) mod link;
mod nearby;
mod registry;
mod session;
mod types;

pub use client::BridgeClient;
pub use lautta_bridge_proto::feature;
pub use link::JobOutcome;
pub use types::{
    bridge_id, location_id, AdHocOptions, Attention, BridgeConfig, BridgeStatus, Consent, Drive, DriveKind,
    NearbyServer, RemoteKind, RemoteLocation, LOCATION_PREFIX,
};
