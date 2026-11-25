// SPDX-License-Identifier: LGPL-2.1-or-later
#![forbid(unsafe_code)]

pub mod access;
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
pub type ProxyResult<T> = zbus::Result<T>;
pub use zbus::zvariant;
