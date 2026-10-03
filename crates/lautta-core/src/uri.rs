// SPDX-License-Identifier: LGPL-2.1-or-later
//! Internal URIs `lautta://<locationId>/<path>` with percent-encoded path
//! bytes (SPEC LOC-7).

use crate::error::{Error, ErrorKind, Result};
use crate::vpath::VPath;
use percent_encoding::{percent_decode_str, percent_encode, AsciiSet, CONTROLS};
use std::fmt;

const SCHEME: &str = "lautta://";

/// Bytes kept literally: unreserved characters and `/`.
const PATH_SET: &AsciiSet = &CONTROLS
    .add(b' ')
    .add(b'"')
    .add(b'#')
    .add(b'%')
    .add(b'<')
    .add(b'>')
    .add(b'?')
    .add(b'[')
    .add(b'\\')
    .add(b']')
    .add(b'^')
    .add(b'`')
    .add(b'{')
    .add(b'|')
    .add(b'}');

/// Location identifiers: `user-documents`, `android-dcim`, `vol-<uuid>`,
/// `nv-<bridge id>`, `arc-<hash>` (LOC-7).
pub type LocationId = String;

#[derive(Clone, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct Uri {
    pub location: LocationId,
    pub path: VPath,
}

impl Uri {
    pub fn new(location: impl Into<LocationId>, path: VPath) -> Uri {
        Uri {
            location: location.into(),
            path,
        }
    }

    pub fn root(location: impl Into<LocationId>) -> Uri {
        Uri::new(location, VPath::root())
    }

    pub fn parse(s: &str) -> Result<Uri> {
        let rest = s
            .strip_prefix(SCHEME)
            .ok_or_else(|| Error::new(ErrorKind::InvalidArgument, "not a lautta URI"))?;
        let (loc, path) = match rest.find('/') {
            Some(i) => (&rest[..i], &rest[i + 1..]),
            None => (rest, ""),
        };
        if loc.is_empty()
            || !loc
                .bytes()
                .all(|b| b.is_ascii_alphanumeric() || b == b'-' || b == b'_' || b == b'.' || b == b':')
        {
            return Err(Error::new(ErrorKind::InvalidArgument, "bad location id"));
        }
        let bytes: Vec<u8> = percent_decode_str(path).collect();
        Ok(Uri::new(loc, VPath::parse(&bytes)?))
    }

    pub fn join(&self, name: &[u8]) -> Result<Uri> {
        Ok(Uri::new(self.location.clone(), self.path.join(name)?))
    }

    pub fn parent(&self) -> Option<Uri> {
        self.path.parent().map(|p| Uri::new(self.location.clone(), p))
    }

    pub fn name(&self) -> Option<&[u8]> {
        self.path.name()
    }

    pub fn is_inside(&self, other: &Uri) -> bool {
        self.location == other.location && self.path.starts_with(&other.path)
    }
}

impl fmt::Display for Uri {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "{SCHEME}{}/{}",
            self.location,
            percent_encode(self.path.as_bytes(), PATH_SET)
        )
    }
}

impl fmt::Debug for Uri {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "Uri({self})")
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn round_trip_with_odd_bytes() {
        let path = VPath::parse(b"Photos/caf\xe9 #1/100%.jpg").unwrap();
        let u = Uri::new("nv-account:3", path.clone());
        let s = u.to_string();
        assert_eq!(s, "lautta://nv-account:3/Photos/caf%E9%20%231/100%25.jpg");
        let back = Uri::parse(&s).unwrap();
        assert_eq!(back, u);
        assert_eq!(back.path, path);
    }

    #[test]
    fn root_forms() {
        assert_eq!(
            Uri::parse("lautta://user-documents").unwrap(),
            Uri::root("user-documents")
        );
        assert_eq!(
            Uri::parse("lautta://user-documents/").unwrap(),
            Uri::root("user-documents")
        );
        assert_eq!(Uri::root("vol-1234").to_string(), "lautta://vol-1234/");
    }

    #[test]
    fn rejects_bad_input() {
        assert!(Uri::parse("file:///x").is_err());
        assert!(Uri::parse("lautta:///x").is_err());
        assert!(Uri::parse("lautta://a b/x").is_err());
        assert!(Uri::parse("lautta://loc/a/../b").is_err());
        assert!(Uri::parse("lautta://loc/a%00b").is_err());
    }

    #[test]
    fn navigation() {
        let u = Uri::parse("lautta://user-documents/a/b").unwrap();
        assert_eq!(u.parent().unwrap().to_string(), "lautta://user-documents/a");
        assert_eq!(u.join(b"c").unwrap().to_string(), "lautta://user-documents/a/b/c");
        assert!(u.is_inside(&Uri::parse("lautta://user-documents/a").unwrap()));
        assert!(!u.is_inside(&Uri::parse("lautta://user-music/a").unwrap()));
        assert_eq!(u.name(), Some(&b"b"[..]));
    }
}
