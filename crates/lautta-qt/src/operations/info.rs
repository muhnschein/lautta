// SPDX-License-Identifier: LGPL-2.1-or-later
//! `InfoModel { uri }`: the details of one item, tags, checksums (OPS-12,
//! ORG-3). The details are one JSON object (`InfoData`, camelCase keys).

use super::{error_parts, parse_uri};
use crate::json::to_json;
use crate::runtime::{core, spawn_then};
use lautta_core::Error;
use qmetaobject::prelude::*;
use qmetaobject::QPointer;

#[derive(QObject, Default)]
pub struct InfoModel {
    base: qt_base_class!(trait QObject),
    uri: qt_property!(QString; WRITE set_uri NOTIFY uri_changed),
    uri_changed: qt_signal!(),
    infoJson: qt_property!(QString; NOTIFY info_changed),
    tagsJson: qt_property!(QString; NOTIFY info_changed),
    loaded: qt_property!(bool; NOTIFY info_changed),
    errorKind: qt_property!(QString; NOTIFY info_changed),
    errorMessage: qt_property!(QString; NOTIFY info_changed),
    info_changed: qt_signal!(),
    checksumBusy: qt_property!(bool; NOTIFY checksum_changed),
    checksum_changed: qt_signal!(),

    reload: qt_method!(fn(&mut self)),
    computeChecksum: qt_method!(fn(&mut self, algo: QString)),
    setModified: qt_method!(fn(&mut self, ms: i64)),
    makeLink: qt_method!(fn(&mut self, dest_uri: QString, hard: bool)),

    checksumReady: qt_signal!(algo: QString, hex: QString),
    modifiedSet: qt_signal!(),
    linkMade: qt_signal!(uri: QString),
    failed: qt_signal!(kind: QString, message: QString),

    generation: u64,
}

impl InfoModel {
    fn set_uri(&mut self, value: QString) {
        self.uri = value;
        self.uri_changed();
        self.reload();
    }

    fn reload(&mut self) {
        let (Some(core), Some(uri)) = (core(), parse_uri(&self.uri)) else {
            return;
        };
        self.generation += 1;
        let generation = self.generation;
        let me = QPointer::from(&*self);
        spawn_then(async move { core.info(&uri).await }, move |res| {
            let Some(p) = me.as_pinned() else { return };
            let mut this = p.borrow_mut();
            if this.generation != generation {
                return;
            }
            match res {
                Ok(info) => {
                    this.infoJson = QString::from(to_json(&info).as_str());
                    this.tagsJson = QString::from(to_json(&info.tags).as_str());
                    this.loaded = true;
                    this.errorKind = QString::default();
                    this.errorMessage = QString::default();
                }
                Err(e) => {
                    this.loaded = false;
                    (this.errorKind, this.errorMessage) = error_parts(&e);
                }
            }
            drop(this);
            p.borrow().info_changed();
        });
    }

    fn emit_failed(&self, e: &Error) {
        let (kind, message) = error_parts(e);
        self.failed(kind, message);
    }

    /// Hashes the file in the background (large files take a while).
    fn computeChecksum(&mut self, algo: QString) {
        let (Some(core), Some(uri)) = (core(), parse_uri(&self.uri)) else {
            return;
        };
        if self.checksumBusy {
            return;
        }
        self.checksumBusy = true;
        self.checksum_changed();
        let me = QPointer::from(&*self);
        let name = algo.to_string();
        spawn_then(async move { core.checksum(&uri, &name).await }, move |res| {
            let Some(p) = me.as_pinned() else { return };
            p.borrow_mut().checksumBusy = false;
            let this = p.borrow();
            this.checksum_changed();
            match res {
                Ok(hex) => this.checksumReady(algo, QString::from(hex.as_str())),
                Err(e) => this.emit_failed(&e),
            }
        });
    }

    fn setModified(&mut self, ms: i64) {
        let (Some(core), Some(uri)) = (core(), parse_uri(&self.uri)) else {
            return;
        };
        let me = QPointer::from(&*self);
        spawn_then(async move { core.set_modified(&uri, ms).await }, move |res| {
            let Some(p) = me.as_pinned() else { return };
            match res {
                Ok(()) => {
                    p.borrow().modifiedSet();
                    p.borrow_mut().reload();
                }
                Err(e) => p.borrow().emit_failed(&e),
            }
        });
    }

    fn makeLink(&mut self, dest_uri: QString, hard: bool) {
        let (Some(core), Some(target), Some(dest)) = (core(), parse_uri(&self.uri), parse_uri(&dest_uri))
        else {
            return;
        };
        let me = QPointer::from(&*self);
        spawn_then(
            async move { core.make_link(&target, &dest, hard).await },
            move |res| {
                let Some(p) = me.as_pinned() else { return };
                let this = p.borrow();
                match res {
                    Ok(link) => this.linkMade(QString::from(link.to_string().as_str())),
                    Err(e) => this.emit_failed(&e),
                }
            },
        );
    }
}
