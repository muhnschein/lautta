// SPDX-License-Identifier: LGPL-2.1-or-later
//! netvfs bridge client: discovery, handshake, consent, reconnect (SPEC §7).
//!
//! [`BridgeClient`] finds the socket (NVB-1), connects peer-to-peer (NVB-2),
//! follows the user's consent (NVB-3), publishes the locations and nearby
//! servers (NVB-4, LOC-6) and keeps the connection alive with backoff while the
//! app is in the foreground (NVB-12). Requests are made through
//! [`crate::provider::netvfs::NetvfsProvider`].

mod client;
pub(crate) mod convert;
pub(crate) mod link;
mod nearby;
mod registry;
mod session;
mod types;

pub use client::BridgeClient;
pub use link::JobOutcome;
pub use types::{
    bridge_id, location_id, AdHocOptions, Attention, BridgeConfig, BridgeStatus, Consent, NearbyServer,
    RemoteKind, RemoteLocation, LOCATION_PREFIX,
};
