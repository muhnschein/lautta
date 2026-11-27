// SPDX-License-Identifier: LGPL-2.1-or-later

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
    count: qt_property!(i32; NOTIFY uriChanged),
    fullPath: qt_property!(QString; NOTIFY uriChanged),
    trail: qt_property!(QVariantList; NOTIFY uriChanged),

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
        if let (Some(core), Ok(u)) = (core(), Uri::parse(&self.uri.to_string())) {
            self.steps = core
                .path_steps(&u)
                .into_iter()
                .map(|s| (s.uri.to_string(), s.name))
                .collect();
        }
        self.count = self.steps.len() as i32;
        self.trail = self
            .steps
            .iter()
            .map(|(uri, _)| QVariant::from(QString::from(uri.as_str())))
            .collect();
        let names: Vec<&str> = self.steps.iter().map(|(_, n)| n.as_str()).collect();
        self.fullPath = QString::from(names.join(" \u{203a} ").as_str());
        self.end_reset_model();
        self.uriChanged();
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
