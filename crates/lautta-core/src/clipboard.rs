// SPDX-License-Identifier: LGPL-2.1-or-later
//! Cut/copy clipboard (OPS-10).
//!
//! `Clipboard` is the app-wide slot; its content is a [`ClipboardContents`]
//! (mode, items and the folder they were taken from). Copy persists across
//! pastes, cut is consumed by the paste.

use crate::error::{Error, ErrorKind, Result};
use crate::uri::Uri;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum ClipboardMode {
    Copy,
    Cut,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ClipboardContents {
    pub mode: ClipboardMode,
    pub items: Vec<Uri>,
    /// The folder the items were taken from (their parent, OPS-10).
    pub source: Uri,
}

#[derive(Debug, Clone, Default)]
pub struct Clipboard {
    contents: Option<ClipboardContents>,
}

impl Clipboard {
    pub fn new() -> Clipboard {
        Clipboard::default()
    }

    /// Replaces the clipboard. Fails for an empty list or a location root.
    pub fn set(&mut self, mode: ClipboardMode, items: Vec<Uri>) -> Result<()> {
        let source = items
            .first()
            .and_then(Uri::parent)
            .ok_or_else(|| Error::new(ErrorKind::InvalidArgument, "nothing to put on the clipboard"))?;
        if items.iter().any(|i| i.parent().is_none()) {
            return Err(Error::new(
                ErrorKind::InvalidArgument,
                "a location root cannot be copied",
            ));
        }
        self.contents = Some(ClipboardContents { mode, items, source });
        Ok(())
    }

    pub fn clear(&mut self) {
        self.contents = None;
    }

    pub fn contents(&self) -> Option<&ClipboardContents> {
        self.contents.as_ref()
    }

    pub fn is_empty(&self) -> bool {
        self.contents.is_none()
    }

    /// What a paste takes: a copy stays on the clipboard, a cut is consumed.
    pub fn take(&mut self) -> Option<ClipboardContents> {
        match self.contents.as_ref().map(|c| c.mode) {
            Some(ClipboardMode::Cut) => self.contents.take(),
            Some(ClipboardMode::Copy) => self.contents.clone(),
            None => None,
        }
    }

    /// Whether a paste into `dest` makes sense: not into one of the items or
    /// below it (a folder cannot contain itself).
    pub fn can_paste_into(&self, dest: &Uri) -> bool {
        match &self.contents {
            Some(c) => !c.items.iter().any(|item| dest.is_inside(item)),
            None => false,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::vpath::VPath;

    fn uri(loc: &str, path: &str) -> Uri {
        Uri::new(loc, VPath::parse(path.as_bytes()).unwrap())
    }

    #[test]
    fn starts_empty_and_refuses_paste() {
        let mut c = Clipboard::new();
        assert!(c.is_empty());
        assert!(c.contents().is_none());
        assert!(c.take().is_none());
        assert!(!c.can_paste_into(&uri("l", "x")));
    }

    #[test]
    fn set_records_mode_items_and_source_folder() {
        let mut c = Clipboard::new();
        c.set(ClipboardMode::Copy, vec![uri("l", "a/b"), uri("l", "a/c")])
            .unwrap();
        let cur = c.contents().unwrap();
        assert_eq!(cur.mode, ClipboardMode::Copy);
        assert_eq!(cur.items.len(), 2);
        assert_eq!(cur.source, uri("l", "a"));
        assert!(!c.is_empty());
        c.set(ClipboardMode::Cut, vec![uri("m", "z")]).unwrap();
        assert_eq!(c.contents().unwrap().source, Uri::root("m"));
        assert_eq!(c.contents().unwrap().mode, ClipboardMode::Cut);
    }

    #[test]
    fn set_rejects_empty_and_roots_and_keeps_old_content() {
        let mut c = Clipboard::new();
        c.set(ClipboardMode::Copy, vec![uri("l", "a")]).unwrap();
        assert_eq!(
            c.set(ClipboardMode::Cut, vec![]).unwrap_err().kind,
            ErrorKind::InvalidArgument
        );
        assert_eq!(
            c.set(ClipboardMode::Cut, vec![Uri::root("l")]).unwrap_err().kind,
            ErrorKind::InvalidArgument
        );
        assert_eq!(
            c.set(ClipboardMode::Cut, vec![uri("l", "a"), Uri::root("l")])
                .unwrap_err()
                .kind,
            ErrorKind::InvalidArgument
        );
        assert_eq!(c.contents().unwrap().mode, ClipboardMode::Copy);
    }

    #[test]
    fn copy_persists_and_cut_is_consumed() {
        let mut c = Clipboard::new();
        c.set(ClipboardMode::Copy, vec![uri("l", "a")]).unwrap();
        assert_eq!(c.take().unwrap().items, vec![uri("l", "a")]);
        assert_eq!(c.take().unwrap().items, vec![uri("l", "a")]);
        c.set(ClipboardMode::Cut, vec![uri("l", "b")]).unwrap();
        assert_eq!(c.take().unwrap().mode, ClipboardMode::Cut);
        assert!(c.take().is_none());
        assert!(c.is_empty());
    }

    #[test]
    fn clear_empties() {
        let mut c = Clipboard::new();
        c.set(ClipboardMode::Copy, vec![uri("l", "a")]).unwrap();
        c.clear();
        assert!(c.is_empty());
    }

    #[test]
    fn cannot_paste_into_an_item_or_its_subfolder() {
        let mut c = Clipboard::new();
        c.set(ClipboardMode::Cut, vec![uri("l", "a/dir"), uri("l", "a/file")])
            .unwrap();
        assert!(!c.can_paste_into(&uri("l", "a/dir")));
        assert!(!c.can_paste_into(&uri("l", "a/dir/sub/deeper")));
        assert!(c.can_paste_into(&uri("l", "a")));
        assert!(c.can_paste_into(&uri("l", "a/dirty")));
        assert!(c.can_paste_into(&uri("l", "b")));
        assert!(c.can_paste_into(&uri("other", "a/dir")));
        c.set(ClipboardMode::Copy, vec![uri("l", "a/dir")]).unwrap();
        assert!(!c.can_paste_into(&uri("l", "a/dir/x")));
    }
}
