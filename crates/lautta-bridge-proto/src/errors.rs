// SPDX-License-Identifier: LGPL-2.1-or-later
//! Bridge errors: `org.netvfs.Error.<Name>` D-Bus errors with the message as
//! first argument and an optional `a{sv}` (`detail`, `retryAfterMs`) as second
//! (netvfs XB-9, XC-24).

use crate::wire::{value_i64, value_str};
use std::collections::HashMap;
use zbus::zvariant::OwnedValue;
use zbus::DBusError;

pub const NETVFS_ERROR_PREFIX: &str = "org.netvfs.Error.";
const INVALID_ARGS: &str = "org.freedesktop.DBus.Error.InvalidArgs";
const UNKNOWN_METHOD: &str = "org.freedesktop.DBus.Error.UnknownMethod";

/// Failure of a bridge call.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum BridgeError {
    /// A backend or policy error from the bridge (`org.netvfs.Error.*`).
    #[error("{name}: {message}")]
    Remote {
        /// Full D-Bus error name.
        name: String,
        message: String,
        detail: Option<String>,
        retry_after_ms: Option<u64>,
    },
    /// The bridge refused the arguments (malformed call).
    #[error("invalid arguments: {0}")]
    InvalidArgs(String),
    /// The bridge has no such method.
    #[error("unknown method: {0}")]
    UnknownMethod(String),
    /// The socket is gone or the connection broke.
    #[error("connection lost: {0}")]
    Disconnected(String),
    /// The bridge sent something this client cannot decode.
    #[error("protocol error: {0}")]
    Protocol(String),
}

impl BridgeError {
    /// A `Remote` error with just a name and message.
    pub fn remote(bare_name: &str, message: &str) -> BridgeError {
        BridgeError::Remote {
            name: format!("{NETVFS_ERROR_PREFIX}{bare_name}"),
            message: message.to_owned(),
            detail: None,
            retry_after_ms: None,
        }
    }

    /// The XC-21 name (`NotFound`, …) of a netvfs error.
    pub fn netvfs_name(&self) -> Option<&str> {
        match self {
            BridgeError::Remote { name, .. } => name.strip_prefix(NETVFS_ERROR_PREFIX),
            _ => None,
        }
    }

    /// Builds the error of `ListDone` / `JobFinished`, where the error is a
    /// bare name and an empty name means success.
    pub fn from_bare(name: &str, message: &str, extra: &HashMap<String, OwnedValue>) -> Option<BridgeError> {
        if name.is_empty() {
            return None;
        }
        Some(BridgeError::Remote {
            name: format!("{NETVFS_ERROR_PREFIX}{name}"),
            message: message.to_owned(),
            detail: extra.get("detail").and_then(|v| value_str(v)),
            retry_after_ms: extra
                .get("retryAfterMs")
                .and_then(|v| value_i64(v))
                .and_then(|n| u64::try_from(n).ok()),
        })
    }

    fn from_method_error(
        name: &str,
        desc: Option<String>,
        extra: &HashMap<String, OwnedValue>,
    ) -> BridgeError {
        let message = desc.unwrap_or_default();
        match name {
            INVALID_ARGS => BridgeError::InvalidArgs(message),
            UNKNOWN_METHOD => BridgeError::UnknownMethod(message),
            _ => {
                let mut e = BridgeError::from_bare(
                    name.strip_prefix(NETVFS_ERROR_PREFIX).unwrap_or(name),
                    &message,
                    extra,
                )
                .unwrap_or_else(|| BridgeError::Protocol("error without a name".into()));
                if let BridgeError::Remote { name: n, .. } = &mut e {
                    *n = name.to_owned();
                }
                e
            }
        }
    }
}

impl From<zbus::Error> for BridgeError {
    fn from(err: zbus::Error) -> BridgeError {
        match err {
            zbus::Error::MethodError(name, desc, msg) => {
                let extra = msg
                    .body()
                    .deserialize_unchecked::<(String, HashMap<String, OwnedValue>)>()
                    .map(|(_, extra)| extra)
                    .unwrap_or_default();
                BridgeError::from_method_error(name.as_str(), desc, &extra)
            }
            zbus::Error::FDO(e) => {
                let desc = e.description().map(str::to_owned);
                BridgeError::from_method_error(e.name().as_str(), desc, &HashMap::new())
            }
            zbus::Error::Variant(_) | zbus::Error::InvalidReply | zbus::Error::ExcessData => {
                BridgeError::Protocol(err.to_string())
            }
            other => BridgeError::Disconnected(other.to_string()),
        }
    }
}

impl From<std::io::Error> for BridgeError {
    fn from(err: std::io::Error) -> BridgeError {
        BridgeError::Disconnected(err.to_string())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use zbus::zvariant::Value;

    fn extra(pairs: Vec<(&str, Value<'static>)>) -> HashMap<String, OwnedValue> {
        pairs
            .into_iter()
            .map(|(k, v)| (k.to_owned(), OwnedValue::try_from(v).unwrap()))
            .collect()
    }

    #[test]
    fn bare_names_with_detail_and_retry() {
        assert_eq!(BridgeError::from_bare("", "", &HashMap::new()), None);
        let e = BridgeError::from_bare(
            "RateLimited",
            "slow down",
            &extra(vec![
                ("detail", Value::from("429")),
                ("retryAfterMs", Value::from(1500i64)),
            ]),
        )
        .unwrap();
        assert_eq!(e.netvfs_name(), Some("RateLimited"));
        match e {
            BridgeError::Remote {
                name,
                message,
                detail,
                retry_after_ms,
            } => {
                assert_eq!(name, "org.netvfs.Error.RateLimited");
                assert_eq!(message, "slow down");
                assert_eq!(detail.as_deref(), Some("429"));
                assert_eq!(retry_after_ms, Some(1500));
            }
            other => panic!("unexpected {other:?}"),
        }
    }

    #[test]
    fn negative_retry_is_ignored() {
        let e =
            BridgeError::from_bare("Locked", "", &extra(vec![("retryAfterMs", Value::from(-1i64))])).unwrap();
        assert!(matches!(
            e,
            BridgeError::Remote {
                retry_after_ms: None,
                ..
            }
        ));
    }

    #[test]
    fn method_error_names() {
        let none = HashMap::new();
        assert_eq!(
            BridgeError::from_method_error(INVALID_ARGS, Some("x".into()), &none),
            BridgeError::InvalidArgs("x".into())
        );
        assert_eq!(
            BridgeError::from_method_error(UNKNOWN_METHOD, None, &none),
            BridgeError::UnknownMethod(String::new())
        );
        let e = BridgeError::from_method_error("org.netvfs.Error.NotFound", Some("gone".into()), &none);
        assert_eq!(e.netvfs_name(), Some("NotFound"));
        assert!(e.to_string().contains("gone"));
        let other = BridgeError::from_method_error("org.example.Odd", None, &none);
        assert_eq!(other.netvfs_name(), None);
    }

    #[test]
    fn io_errors_are_disconnects() {
        let e = BridgeError::from(std::io::Error::from(std::io::ErrorKind::BrokenPipe));
        assert!(matches!(e, BridgeError::Disconnected(_)));
        let e = BridgeError::from(zbus::Error::Unsupported);
        assert!(matches!(e, BridgeError::Disconnected(_)));
        let e = BridgeError::from(zbus::Error::InvalidReply);
        assert!(matches!(e, BridgeError::Protocol(_)));
    }
}
