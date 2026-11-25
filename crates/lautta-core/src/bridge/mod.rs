// SPDX-License-Identifier: LGPL-2.1-or-later

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
    bridge_id, location_id, Attention, BridgeConfig, BridgeStatus, Consent, Drive, DriveKind, Level,
    NearbyServer, RemoteLocation, LOCATION_PREFIX,
};
