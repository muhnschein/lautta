// SPDX-License-Identifier: LGPL-2.1-or-later
//! netvfs bridge protocol `org.netvfs.Bridge1` (netvfs SPEC-v2 §8a): wire
//! types, the client proxy, and (feature `fake`) an in-process fake bridge
//! that replays the shared contract sequences (SPEC TST-2).
#![forbid(unsafe_code)]

pub mod connect;
pub mod errors;
pub mod proxy;
pub mod signals;
pub mod wire;

#[cfg(feature = "fake")]
pub mod fake;

pub use connect::{connect, Connection, SignalStream};
pub use errors::BridgeError;
pub use proxy::Bridge1Proxy;
pub use signals::Signal;
pub use wire::*;
pub use zeroize::Zeroizing;
/// Result of a proxy call, so that users need not depend on zbus.
pub type ProxyResult<T> = zbus::Result<T>;
pub use zbus::zvariant;
