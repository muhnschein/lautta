// SPDX-License-Identifier: LGPL-2.1-or-later
//! One error model for every provider (SPEC RS-8): the netvfs v2 taxonomy
//! (netvfs SPEC-v2 XC-21) plus local I/O mappings, so the UI has a single
//! message table (SPEC §20, [`crate::messages`]).

use std::fmt;

/// Error kinds. The names returned by [`ErrorKind::name`] are the netvfs
/// `errorName()` strings, so bridge errors (`org.netvfs.Error.<Name>`) map
/// one-to-one.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum ErrorKind {
    NotFound,
    AlreadyExists,
    PermissionDenied,
    NoSpace,
    Unsupported,
    InvalidArgument,
    Canceled,
    TimedOut,
    NetworkUnreachable,
    ConnectionLost,
    AuthFailed,
    ServerIdentityUnknown,
    ServerIdentityChanged,
    SecurityPolicy,
    ProtocolError,
    NotADirectory,
    IsADirectory,
    DirectoryNotEmpty,
    InvalidName,
    ReadOnlyFilesystem,
    Locked,
    TooManyConnections,
    RateLimited,
    NotModified,
    /// The bridge socket is gone or not answering (SPEC §20 "bridge gone").
    BridgeUnavailable,
    /// The sandbox does not let the app read the path (SPEC §20 "sandbox").
    Sandbox,
    /// A name crosses into a location where it is not representable.
    CrossesDevice,
    Io,
    Internal,
}

impl ErrorKind {
    pub const ALL: [ErrorKind; 29] = [
        ErrorKind::NotFound,
        ErrorKind::AlreadyExists,
        ErrorKind::PermissionDenied,
        ErrorKind::NoSpace,
        ErrorKind::Unsupported,
        ErrorKind::InvalidArgument,
        ErrorKind::Canceled,
        ErrorKind::TimedOut,
        ErrorKind::NetworkUnreachable,
        ErrorKind::ConnectionLost,
        ErrorKind::AuthFailed,
        ErrorKind::ServerIdentityUnknown,
        ErrorKind::ServerIdentityChanged,
        ErrorKind::SecurityPolicy,
        ErrorKind::ProtocolError,
        ErrorKind::NotADirectory,
        ErrorKind::IsADirectory,
        ErrorKind::DirectoryNotEmpty,
        ErrorKind::InvalidName,
        ErrorKind::ReadOnlyFilesystem,
        ErrorKind::Locked,
        ErrorKind::TooManyConnections,
        ErrorKind::RateLimited,
        ErrorKind::NotModified,
        ErrorKind::BridgeUnavailable,
        ErrorKind::Sandbox,
        ErrorKind::CrossesDevice,
        ErrorKind::Io,
        ErrorKind::Internal,
    ];

    pub fn name(self) -> &'static str {
        match self {
            ErrorKind::NotFound => "NotFound",
            ErrorKind::AlreadyExists => "AlreadyExists",
            ErrorKind::PermissionDenied => "PermissionDenied",
            ErrorKind::NoSpace => "NoSpace",
            ErrorKind::Unsupported => "Unsupported",
            ErrorKind::InvalidArgument => "InvalidArgument",
            ErrorKind::Canceled => "Canceled",
            ErrorKind::TimedOut => "TimedOut",
            ErrorKind::NetworkUnreachable => "NetworkUnreachable",
            ErrorKind::ConnectionLost => "ConnectionLost",
            ErrorKind::AuthFailed => "AuthFailed",
            ErrorKind::ServerIdentityUnknown => "ServerIdentityUnknown",
            ErrorKind::ServerIdentityChanged => "ServerIdentityChanged",
            ErrorKind::SecurityPolicy => "SecurityPolicy",
            ErrorKind::ProtocolError => "ProtocolError",
            ErrorKind::NotADirectory => "NotADirectory",
            ErrorKind::IsADirectory => "IsADirectory",
            ErrorKind::DirectoryNotEmpty => "DirectoryNotEmpty",
            ErrorKind::InvalidName => "InvalidName",
            ErrorKind::ReadOnlyFilesystem => "ReadOnlyFilesystem",
            ErrorKind::Locked => "Locked",
            ErrorKind::TooManyConnections => "TooManyConnections",
            ErrorKind::RateLimited => "RateLimited",
            ErrorKind::NotModified => "NotModified",
            ErrorKind::BridgeUnavailable => "BridgeUnavailable",
            ErrorKind::Sandbox => "Sandbox",
            ErrorKind::CrossesDevice => "CrossesDevice",
            ErrorKind::Io => "Io",
            ErrorKind::Internal => "Internal",
        }
    }

    /// Maps a netvfs error name (with or without the `org.netvfs.Error.`
    /// prefix). Unknown names become [`ErrorKind::ProtocolError`] so a newer
    /// bridge never makes the app misreport an error as success.
    pub fn from_name(name: &str) -> ErrorKind {
        let bare = name.strip_prefix("org.netvfs.Error.").unwrap_or(name);
        ErrorKind::ALL
            .iter()
            .copied()
            .find(|k| k.name() == bare)
            .unwrap_or(ErrorKind::ProtocolError)
    }

    /// Transient errors are retried by transfers (SPEC XFR-9).
    pub fn is_transient(self) -> bool {
        matches!(
            self,
            ErrorKind::TimedOut
                | ErrorKind::NetworkUnreachable
                | ErrorKind::ConnectionLost
                | ErrorKind::TooManyConnections
                | ErrorKind::RateLimited
                | ErrorKind::Locked
                | ErrorKind::BridgeUnavailable
        )
    }
}

impl fmt::Display for ErrorKind {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.name())
    }
}

/// An error with its kind, a human-oriented message from the source (never
/// secrets), an optional protocol detail for the *Details* view, and an
/// optional retry hint (netvfs XC-24).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Error {
    pub kind: ErrorKind,
    pub message: String,
    pub detail: Option<String>,
    pub retry_after_ms: Option<u64>,
}

impl Error {
    pub fn new(kind: ErrorKind, message: impl Into<String>) -> Error {
        Error {
            kind,
            message: message.into(),
            detail: None,
            retry_after_ms: None,
        }
    }

    pub fn kind(kind: ErrorKind) -> Error {
        Error::new(kind, String::new())
    }

    pub fn with_detail(mut self, detail: impl Into<String>) -> Error {
        self.detail = Some(detail.into());
        self
    }

    pub fn is(&self, kind: ErrorKind) -> bool {
        self.kind == kind
    }

    pub fn from_io(err: &std::io::Error) -> Error {
        Error::new(io_kind(err), err.to_string())
    }
}

impl fmt::Display for Error {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        if self.message.is_empty() {
            write!(f, "{}", self.kind)
        } else {
            write!(f, "{}: {}", self.kind, self.message)
        }
    }
}

impl std::error::Error for Error {}

impl From<std::io::Error> for Error {
    fn from(err: std::io::Error) -> Error {
        Error::from_io(&err)
    }
}

impl From<rustix::io::Errno> for Error {
    fn from(err: rustix::io::Errno) -> Error {
        Error::from(std::io::Error::from(err))
    }
}

impl From<rusqlite::Error> for Error {
    fn from(err: rusqlite::Error) -> Error {
        Error::new(ErrorKind::Internal, err.to_string())
    }
}

fn io_kind(err: &std::io::Error) -> ErrorKind {
    use std::io::ErrorKind as K;
    if let Some(code) = err.raw_os_error() {
        if let Some(kind) = errno_kind(code) {
            return kind;
        }
    }
    match err.kind() {
        K::NotFound => ErrorKind::NotFound,
        K::PermissionDenied => ErrorKind::PermissionDenied,
        K::AlreadyExists => ErrorKind::AlreadyExists,
        K::InvalidInput | K::InvalidData => ErrorKind::InvalidArgument,
        K::TimedOut => ErrorKind::TimedOut,
        K::Unsupported => ErrorKind::Unsupported,
        K::Interrupted => ErrorKind::Canceled,
        _ => ErrorKind::Io,
    }
}

fn errno_kind(code: i32) -> Option<ErrorKind> {
    use rustix::io::Errno;
    let e = Errno::from_raw_os_error(code);
    Some(if e == Errno::NOSPC || e == Errno::DQUOT {
        ErrorKind::NoSpace
    } else if e == Errno::ROFS {
        ErrorKind::ReadOnlyFilesystem
    } else if e == Errno::NOTDIR {
        ErrorKind::NotADirectory
    } else if e == Errno::ISDIR {
        ErrorKind::IsADirectory
    } else if e == Errno::NOTEMPTY {
        ErrorKind::DirectoryNotEmpty
    } else if e == Errno::NAMETOOLONG || e == Errno::ILSEQ {
        ErrorKind::InvalidName
    } else if e == Errno::XDEV {
        ErrorKind::CrossesDevice
    } else if e == Errno::ACCESS || e == Errno::PERM {
        ErrorKind::PermissionDenied
    } else if e == Errno::NOENT {
        ErrorKind::NotFound
    } else if e == Errno::EXIST {
        ErrorKind::AlreadyExists
    } else if e == Errno::OPNOTSUPP || e == Errno::NOSYS {
        ErrorKind::Unsupported
    } else if e == Errno::BUSY || e == Errno::TXTBSY {
        ErrorKind::Locked
    } else {
        return None;
    })
}

pub type Result<T, E = Error> = std::result::Result<T, E>;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn names_round_trip() {
        for kind in ErrorKind::ALL {
            assert_eq!(ErrorKind::from_name(kind.name()), kind);
            assert_eq!(
                ErrorKind::from_name(&format!("org.netvfs.Error.{}", kind.name())),
                kind
            );
        }
    }

    #[test]
    fn unknown_bridge_error_is_protocol_error() {
        assert_eq!(
            ErrorKind::from_name("org.netvfs.Error.Brandnew"),
            ErrorKind::ProtocolError
        );
    }

    #[test]
    fn errno_mapping() {
        let e = Error::from(std::io::Error::from_raw_os_error(
            rustix::io::Errno::NOSPC.raw_os_error(),
        ));
        assert_eq!(e.kind, ErrorKind::NoSpace);
        let e = Error::from(std::io::Error::from_raw_os_error(
            rustix::io::Errno::XDEV.raw_os_error(),
        ));
        assert_eq!(e.kind, ErrorKind::CrossesDevice);
        let e = Error::from(std::io::Error::from_raw_os_error(
            rustix::io::Errno::NOTEMPTY.raw_os_error(),
        ));
        assert_eq!(e.kind, ErrorKind::DirectoryNotEmpty);
        let e = Error::from(std::io::Error::new(std::io::ErrorKind::NotFound, "x"));
        assert_eq!(e.kind, ErrorKind::NotFound);
    }

    #[test]
    fn transient() {
        assert!(ErrorKind::ConnectionLost.is_transient());
        assert!(!ErrorKind::NotFound.is_transient());
        assert!(!ErrorKind::AuthFailed.is_transient());
    }
}
