// SPDX-License-Identifier: LGPL-2.1-or-later
//! `PermissionsModel { uri }`: the rwx grid, octal and recursive masks
//! (OPS-12). The grid and the octal field are two views of `mode`.

use super::{error_parts, parse_uri};
use crate::runtime::{core, spawn_then};
use lautta_core::app_operations::{parse_octal, PermissionsInfo, RecursiveModes};
use lautta_core::{Error, Result};
use qmetaobject::prelude::*;
use qmetaobject::QPointer;
use std::sync::atomic::AtomicBool;
use std::sync::Arc;

const DEFAULT_FILES: u32 = 0o644;
const DEFAULT_FOLDERS: u32 = 0o755;

/// The bit of the grid cell: `who` 0 owner, 1 group, 2 others; `what` 0
/// read, 1 write, 2 execute.
fn bit(who: i32, what: i32) -> Option<u32> {
    if !(0..3).contains(&who) || !(0..3).contains(&what) {
        return None;
    }
    Some(1 << ((2 - what) + 3 * (2 - who)))
}

/// Save is offered when the location can set modes, the octal field parses,
/// the recursive masks parse (when used) and nothing is running.
fn apply_allowed(supported: bool, octal_ok: bool, masks_ok: bool, busy: bool) -> bool {
    supported && octal_ok && masks_ok && !busy
}

fn octal_text(mode: u32) -> String {
    format!("{:03o}", mode & 0o777)
}

#[derive(QObject, Default)]
pub struct PermissionsModel {
    base: qt_base_class!(trait QObject),
    uri: qt_property!(QString; WRITE set_uri NOTIFY uri_changed),
    uri_changed: qt_signal!(),
    mode: qt_property!(i32; NOTIFY mode_changed),
    octal: qt_property!(QString; WRITE set_octal NOTIFY mode_changed),
    mode_changed: qt_signal!(),
    recursive: qt_property!(bool; NOTIFY options_changed),
    filesOctal: qt_property!(QString; WRITE set_files_octal NOTIFY options_changed),
    foldersOctal: qt_property!(QString; WRITE set_folders_octal NOTIFY options_changed),
    canApply: qt_property!(bool; NOTIFY options_changed),
    options_changed: qt_signal!(),
    isDir: qt_property!(bool; NOTIFY loaded_changed),
    supported: qt_property!(bool; NOTIFY loaded_changed),
    fsType: qt_property!(QString; NOTIFY loaded_changed),
    locationName: qt_property!(QString; NOTIFY loaded_changed),
    owner: qt_property!(QString; NOTIFY loaded_changed),
    group: qt_property!(QString; NOTIFY loaded_changed),
    loaded: qt_property!(bool; NOTIFY loaded_changed),
    errorKind: qt_property!(QString; NOTIFY loaded_changed),
    errorMessage: qt_property!(QString; NOTIFY loaded_changed),
    loaded_changed: qt_signal!(),
    busy: qt_property!(bool; NOTIFY busy_changed),
    busy_changed: qt_signal!(),

    bit: qt_method!(fn(&self, who: i32, what: i32) -> bool),
    setBit: qt_method!(fn(&mut self, who: i32, what: i32, on: bool)),
    apply: qt_method!(fn(&mut self)),
    applied: qt_signal!(changed: i32, failedCount: i32),
    failed: qt_signal!(kind: QString, message: QString),

    files_mode: u32,
    folders_mode: u32,
    files_ok: bool,
    folders_ok: bool,
}

impl PermissionsModel {
    fn set_uri(&mut self, value: QString) {
        self.uri = value;
        self.uri_changed();
        self.files_mode = DEFAULT_FILES;
        self.folders_mode = DEFAULT_FOLDERS;
        self.files_ok = true;
        self.folders_ok = true;
        self.filesOctal = QString::from(octal_text(DEFAULT_FILES).as_str());
        self.foldersOctal = QString::from(octal_text(DEFAULT_FOLDERS).as_str());
        self.update_apply();
        self.load();
    }

    fn load(&mut self) {
        let (Some(core), Some(uri)) = (core(), parse_uri(&self.uri)) else {
            return;
        };
        let me = QPointer::from(&*self);
        spawn_then(async move { core.permissions(&uri).await }, move |res| {
            let Some(p) = me.as_pinned() else { return };
            p.borrow_mut().loaded_with(res);
            let this = p.borrow();
            this.loaded_changed();
            this.mode_changed();
            this.options_changed();
        });
    }

    fn loaded_with(&mut self, res: Result<PermissionsInfo>) {
        match res {
            Ok(info) => {
                self.set_mode(info.mode);
                self.isDir = info.is_dir;
                self.supported = info.supported;
                self.fsType = QString::from(info.fs_type.unwrap_or_default().as_str());
                self.locationName = QString::from(info.location_name.as_str());
                self.owner = QString::from(info.owner.unwrap_or_default().as_str());
                self.group = QString::from(info.group.unwrap_or_default().as_str());
                self.loaded = true;
                self.errorKind = QString::default();
                self.errorMessage = QString::default();
            }
            Err(e) => {
                self.loaded = false;
                (self.errorKind, self.errorMessage) = error_parts(&e);
            }
        }
        self.update_apply();
    }

    fn set_mode(&mut self, mode: u32) {
        let mode = mode & 0o777;
        self.mode = i32::try_from(mode).unwrap_or(0);
        self.octal = QString::from(octal_text(mode).as_str());
    }

    fn current(&self) -> u32 {
        u32::try_from(self.mode).unwrap_or(0)
    }

    fn set_octal(&mut self, value: QString) {
        self.octal = value.clone();
        if let Some(mode) = parse_octal(&value.to_string()) {
            self.mode = i32::try_from(mode & 0o777).unwrap_or(0);
        }
        self.update_apply();
        self.mode_changed();
    }

    fn bit(&self, who: i32, what: i32) -> bool {
        bit(who, what).is_some_and(|b| self.current() & b != 0)
    }

    fn setBit(&mut self, who: i32, what: i32, on: bool) {
        let Some(b) = bit(who, what) else { return };
        let mode = if on {
            self.current() | b
        } else {
            self.current() & !b
        };
        self.set_mode(mode);
        self.update_apply();
        self.mode_changed();
    }

    fn set_files_octal(&mut self, value: QString) {
        self.filesOctal = value.clone();
        let parsed = parse_octal(&value.to_string());
        self.files_ok = parsed.is_some();
        self.files_mode = parsed.unwrap_or(self.files_mode);
        self.update_apply();
        self.options_changed();
    }

    fn set_folders_octal(&mut self, value: QString) {
        self.foldersOctal = value.clone();
        let parsed = parse_octal(&value.to_string());
        self.folders_ok = parsed.is_some();
        self.folders_mode = parsed.unwrap_or(self.folders_mode);
        self.update_apply();
        self.options_changed();
    }

    /// The grid is always valid; the recursive masks must parse when used.
    fn update_apply(&mut self) {
        let octal_ok = parse_octal(&self.octal.to_string()).is_some();
        let masks_ok = !self.recursive || (self.files_ok && self.folders_ok);
        self.canApply = apply_allowed(self.supported, octal_ok, masks_ok, self.busy);
    }

    fn apply(&mut self) {
        let (Some(core), Some(uri)) = (core(), parse_uri(&self.uri)) else {
            return;
        };
        self.update_apply();
        if !self.canApply {
            return;
        }
        self.busy = true;
        self.busy_changed();
        let mode = self.current();
        let masks = (self.recursive && self.isDir).then_some(RecursiveModes {
            files: self.files_mode,
            dirs: self.folders_mode,
        });
        let me = QPointer::from(&*self);
        spawn_then(
            async move {
                let cancel = Arc::new(AtomicBool::new(false));
                core.set_permissions(&uri, mode, masks, &cancel).await
            },
            move |res| {
                let Some(p) = me.as_pinned() else { return };
                {
                    let mut this = p.borrow_mut();
                    this.busy = false;
                    this.update_apply();
                }
                let this = p.borrow();
                this.busy_changed();
                this.options_changed();
                this.finished(res);
            },
        );
    }

    fn finished(&self, res: Result<lautta_core::app_operations::PermissionsOutcome>) {
        match res {
            Ok(o) => self.applied(
                i32::try_from(o.changed).unwrap_or(i32::MAX),
                i32::try_from(o.failed).unwrap_or(i32::MAX),
            ),
            Err(e) => self.report(&e),
        }
    }

    fn report(&self, e: &Error) {
        let (kind, message) = error_parts(e);
        self.failed(kind, message);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn grid_cells_map_to_mode_bits() {
        assert_eq!(bit(0, 0), Some(0o400));
        assert_eq!(bit(0, 2), Some(0o100));
        assert_eq!(bit(1, 1), Some(0o020));
        assert_eq!(bit(2, 2), Some(0o001));
        assert_eq!(bit(3, 0), None);
        assert_eq!(bit(0, -1), None);
    }

    #[test]
    fn save_needs_everything_in_order() {
        assert!(apply_allowed(true, true, true, false));
        assert!(!apply_allowed(false, true, true, false), "unsupported location");
        assert!(!apply_allowed(true, false, true, false), "bad octal");
        assert!(!apply_allowed(true, true, false, false), "bad recursive mask");
        assert!(!apply_allowed(true, true, true, true), "already applying");
    }

    #[test]
    fn octal_is_three_digits() {
        assert_eq!(octal_text(0o7), "007");
        assert_eq!(octal_text(0o4755), "755");
        assert_eq!(octal_text(0o644), "644");
    }
}
