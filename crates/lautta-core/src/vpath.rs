// SPDX-License-Identifier: LGPL-2.1-or-later
//! Location-relative paths as raw bytes (SPEC NVB-8, Appendix B). Names may
//! contain bytes that are not UTF-8; display uses lossy decoding.

use crate::error::{Error, ErrorKind, Result};
use std::fmt;

/// A normalised path inside a location: no leading or trailing `/`, no empty,
/// `.` or `..` components. The root is the empty path.
#[derive(Clone, PartialEq, Eq, Hash, PartialOrd, Ord, Default)]
pub struct VPath(Vec<u8>);

impl VPath {
    pub fn root() -> VPath {
        VPath(Vec::new())
    }

    /// Normalises `raw`: splits on `/`, drops empty and `.` components and
    /// rejects `..` and NUL (a path never escapes its location).
    pub fn parse(raw: &[u8]) -> Result<VPath> {
        let mut out: Vec<u8> = Vec::with_capacity(raw.len());
        for comp in raw.split(|b| *b == b'/') {
            if comp.is_empty() || comp == b"." {
                continue;
            }
            if comp == b".." || comp.contains(&0) {
                return Err(Error::new(ErrorKind::InvalidName, "path component not allowed"));
            }
            if !out.is_empty() {
                out.push(b'/');
            }
            out.extend_from_slice(comp);
        }
        Ok(VPath(out))
    }

    pub fn from_str_lossless(s: &str) -> Result<VPath> {
        VPath::parse(s.as_bytes())
    }

    pub fn as_bytes(&self) -> &[u8] {
        &self.0
    }

    pub fn into_bytes(self) -> Vec<u8> {
        self.0
    }

    pub fn is_root(&self) -> bool {
        self.0.is_empty()
    }

    pub fn components(&self) -> impl Iterator<Item = &[u8]> {
        self.0.split(|b| *b == b'/').filter(|c| !c.is_empty())
    }

    pub fn depth(&self) -> usize {
        self.components().count()
    }

    /// Last component, `None` for the root.
    pub fn name(&self) -> Option<&[u8]> {
        if self.0.is_empty() {
            return None;
        }
        Some(match self.0.iter().rposition(|b| *b == b'/') {
            Some(i) => &self.0[i + 1..],
            None => &self.0[..],
        })
    }

    pub fn parent(&self) -> Option<VPath> {
        if self.0.is_empty() {
            return None;
        }
        Some(match self.0.iter().rposition(|b| *b == b'/') {
            Some(i) => VPath(self.0[..i].to_vec()),
            None => VPath::root(),
        })
    }

    /// Appends one name. The name must be a single valid component.
    pub fn join(&self, name: &[u8]) -> Result<VPath> {
        validate_name(name)?;
        let mut out = self.0.clone();
        if !out.is_empty() {
            out.push(b'/');
        }
        out.extend_from_slice(name);
        Ok(VPath(out))
    }

    /// Appends a relative path (several components).
    pub fn join_path(&self, rel: &VPath) -> VPath {
        if rel.is_root() {
            return self.clone();
        }
        if self.is_root() {
            return rel.clone();
        }
        let mut out = self.0.clone();
        out.push(b'/');
        out.extend_from_slice(&rel.0);
        VPath(out)
    }

    /// True when `self` equals `other` or lies below it.
    pub fn starts_with(&self, other: &VPath) -> bool {
        if other.is_root() {
            return true;
        }
        self.0 == other.0
            || (self.0.len() > other.0.len() && self.0.starts_with(&other.0) && self.0[other.0.len()] == b'/')
    }

    /// `self` relative to `base`, if `self` is inside it.
    pub fn strip_prefix(&self, base: &VPath) -> Option<VPath> {
        if !self.starts_with(base) {
            return None;
        }
        if base.is_root() {
            return Some(self.clone());
        }
        if self.0.len() == base.0.len() {
            return Some(VPath::root());
        }
        Some(VPath(self.0[base.0.len() + 1..].to_vec()))
    }

    /// Ancestors from the root down to the parent (for the path menu, BRW-9).
    pub fn ancestors(&self) -> Vec<VPath> {
        let mut out = vec![VPath::root()];
        let mut cur = VPath::root();
        let comps: Vec<&[u8]> = self.components().collect();
        for comp in comps.iter().take(comps.len().saturating_sub(1)) {
            cur = VPath(if cur.0.is_empty() {
                comp.to_vec()
            } else {
                [cur.0.as_slice(), b"/", comp].concat()
            });
            out.push(cur.clone());
        }
        if self.is_root() {
            out.clear();
        }
        out
    }

    pub fn display(&self) -> String {
        String::from_utf8_lossy(&self.0).into_owned()
    }

    pub fn is_lossy(&self) -> bool {
        std::str::from_utf8(&self.0).is_err()
    }
}

/// A single name is valid when non-empty, not `.`/`..`, and has no `/` or NUL.
pub fn validate_name(name: &[u8]) -> Result<()> {
    if name.is_empty() || name == b"." || name == b".." || name.contains(&b'/') || name.contains(&0) {
        return Err(Error::new(ErrorKind::InvalidName, "invalid name"));
    }
    Ok(())
}

pub fn display_name(name: &[u8]) -> String {
    String::from_utf8_lossy(name).into_owned()
}

impl fmt::Debug for VPath {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "VPath({:?})", self.display())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn p(s: &str) -> VPath {
        VPath::parse(s.as_bytes()).unwrap()
    }

    #[test]
    fn normalises() {
        assert_eq!(p("/a//b/./c/").as_bytes(), b"a/b/c");
        assert!(p("/").is_root());
        assert!(p("").is_root());
    }

    #[test]
    fn rejects_escape_and_nul() {
        assert!(VPath::parse(b"a/../b").is_err());
        assert!(VPath::parse(b"a/b\0c").is_err());
    }

    #[test]
    fn name_parent_join() {
        let x = p("a/b/c");
        assert_eq!(x.name(), Some(&b"c"[..]));
        assert_eq!(x.parent(), Some(p("a/b")));
        assert_eq!(p("a").parent(), Some(VPath::root()));
        assert_eq!(VPath::root().parent(), None);
        assert_eq!(VPath::root().name(), None);
        assert_eq!(VPath::root().join(b"x").unwrap(), p("x"));
        assert_eq!(p("a").join(b"x").unwrap(), p("a/x"));
        assert!(p("a").join(b"x/y").is_err());
        assert!(p("a").join(b"..").is_err());
        assert!(p("a").join(b"").is_err());
        assert_eq!(p("a").join_path(&p("b/c")), p("a/b/c"));
        assert_eq!(VPath::root().join_path(&p("b")), p("b"));
        assert_eq!(p("a").join_path(&VPath::root()), p("a"));
    }

    #[test]
    fn prefixes() {
        assert!(p("a/b").starts_with(&p("a")));
        assert!(p("a").starts_with(&p("a")));
        assert!(!p("ab").starts_with(&p("a")));
        assert!(p("x").starts_with(&VPath::root()));
        assert_eq!(p("a/b/c").strip_prefix(&p("a")), Some(p("b/c")));
        assert_eq!(p("a").strip_prefix(&p("a")), Some(VPath::root()));
        assert_eq!(p("ab").strip_prefix(&p("a")), None);
        assert_eq!(p("a/b").strip_prefix(&VPath::root()), Some(p("a/b")));
    }

    #[test]
    fn ancestors_and_depth() {
        assert_eq!(p("a/b/c").ancestors(), vec![VPath::root(), p("a"), p("a/b")]);
        assert!(VPath::root().ancestors().is_empty());
        assert_eq!(p("a").ancestors(), vec![VPath::root()]);
        assert_eq!(p("a/b/c").depth(), 3);
    }

    #[test]
    fn non_utf8_is_lossy() {
        let x = VPath::parse(b"caf\xe9").unwrap();
        assert!(x.is_lossy());
        assert_eq!(x.display(), "caf\u{fffd}");
        assert!(!p("café").is_lossy());
    }
}
