// SPDX-License-Identifier: LGPL-2.1-or-later
//! `FolderInfo { uri }`: how many items the folder shown by the cover has
//! (INT-3). Counted from the cached listing the Directory page just made,
//! so the cover never lists a remote folder itself.

use crate::runtime::{blocking_then, core};
use lautta_core::Uri;
use qmetaobject::prelude::*;
use qmetaobject::QPointer;

#[derive(QObject, Default)]
pub struct FolderInfo {
    base: qt_base_class!(trait QObject),
    uri: qt_property!(QString; NOTIFY uriChanged WRITE set_uri),
    uriChanged: qt_signal!(),
    /// Visible items, `-1` while unknown.
    count: qt_property!(i32; NOTIFY countChanged),
    countChanged: qt_signal!(),
    refresh: qt_method!(fn(&mut self)),
}

impl FolderInfo {
    fn set_uri(&mut self, uri: QString) {
        if self.uri != uri {
            self.uri = uri;
            self.uriChanged();
            // Unknown until the cached listing has been looked at.
            if self.count != -1 {
                self.count = -1;
                self.countChanged();
            }
            self.refresh();
        }
    }

    fn refresh(&mut self) {
        let Some(core) = core() else { return };
        let uri = self.uri.to_string();
        let me = QPointer::from(&*self);
        blocking_then(
            move || {
                Uri::parse(&uri)
                    .ok()
                    .and_then(|u| core.cached_folder_count(&u))
                    .map_or(-1, |n| i32::try_from(n).unwrap_or(i32::MAX))
            },
            move |count| {
                if let Some(info) = me.as_pinned() {
                    let mut info = info.borrow_mut();
                    if info.count != count {
                        info.count = count;
                        info.countChanged();
                    }
                }
            },
        );
    }
}
