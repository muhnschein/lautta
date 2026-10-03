// SPDX-License-Identifier: LGPL-2.1-or-later
//! `PathModel { uri }`: the ancestors of a folder for the path menu (BRW-9)
//! and the path editor's completion from cached listings.

use crate::runtime::core;
use lautta_core::Uri;
use qmetaobject::prelude::*;
use qmetaobject::{QByteArray, QVariantList};
use std::collections::HashMap;

const ROLE_URI: i32 = 0x0100;
const ROLE_NAME: i32 = 0x0101;

#[derive(QObject, Default)]
pub struct PathModel {
    base: qt_base_class!(trait QAbstractListModel),
    uri: qt_property!(QString; NOTIFY uriChanged WRITE set_uri),
    uriChanged: qt_signal!(),
    /// Text for the path editor: `~/Documents/Uni` or `/srv/photos`.
    address: qt_property!(QString; NOTIFY uriChanged),
    count: qt_property!(i32; NOTIFY uriChanged),
    /// Names above the folder joined with " › " (header description).
    breadcrumb: qt_property!(QString; NOTIFY uriChanged),
    /// All names down to the folder joined with " › ".
    fullPath: qt_property!(QString; NOTIFY uriChanged),

    complete: qt_method!(fn(&self, prefix: QString) -> QVariantList),
    resolve: qt_method!(fn(&self, text: QString) -> QString),

    steps: Vec<(String, String)>,
}

impl PathModel {
    fn set_uri(&mut self, value: QString) {
        if self.uri == value {
            return;
        }
        self.begin_reset_model();
        self.uri = value;
        self.steps.clear();
        self.address = QString::default();
        if let (Some(core), Ok(u)) = (core(), Uri::parse(&self.uri.to_string())) {
            self.steps = core
                .path_steps(&u)
                .into_iter()
                .map(|s| (s.uri.to_string(), s.name))
                .collect();
            self.address = QString::from(core.edit_address(&u).as_str());
        }
        self.count = self.steps.len() as i32;
        let names: Vec<&str> = self.steps.iter().map(|(_, n)| n.as_str()).collect();
        self.fullPath = QString::from(names.join(" \u{203a} ").as_str());
        let above = names.len().saturating_sub(1);
        self.breadcrumb = QString::from(names[..above].join(" \u{203a} ").as_str());
        self.end_reset_model();
        self.uriChanged();
    }

    fn complete(&self, prefix: QString) -> QVariantList {
        let mut out = QVariantList::default();
        let (Some(core), Ok(base)) = (core(), Uri::parse(&self.uri.to_string())) else {
            return out;
        };
        for u in core.complete_address(&prefix.to_string(), &base) {
            out.push(QVariant::from(QString::from(u.to_string().as_str())));
        }
        out
    }

    fn resolve(&self, text: QString) -> QString {
        let (Some(core), Ok(base)) = (core(), Uri::parse(&self.uri.to_string())) else {
            return QString::default();
        };
        core.resolve_address(&text.to_string(), &base)
            .map(|u| QString::from(u.to_string().as_str()))
            .unwrap_or_default()
    }
}

impl QAbstractListModel for PathModel {
    fn row_count(&self) -> i32 {
        self.steps.len() as i32
    }

    fn data(&self, index: QModelIndex, role: i32) -> QVariant {
        let Some((uri, name)) = usize::try_from(index.row()).ok().and_then(|i| self.steps.get(i)) else {
            return QVariant::default();
        };
        match role {
            ROLE_URI => QString::from(uri.as_str()).into(),
            ROLE_NAME => QString::from(name.as_str()).into(),
            _ => QVariant::default(),
        }
    }

    fn role_names(&self) -> HashMap<i32, QByteArray> {
        HashMap::from([(ROLE_URI, "uri".into()), (ROLE_NAME, "name".into())])
    }
}
