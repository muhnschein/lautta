// SPDX-License-Identifier: LGPL-2.1-or-later
//! Client of netvfs `org.netvfs.Bridge1`: wire types, proxy, and (feature `fake`) an
//! in-process fake bridge replaying the shared contract sequences.
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
pub type ProxyResult<T> = zbus::Result<T>;
pub use zbus::zvariant;
