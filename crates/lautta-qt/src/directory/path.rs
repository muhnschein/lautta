// SPDX-License-Identifier: LGPL-2.1-or-later

use crate::json::to_json;
use crate::runtime::core;
use lautta_core::Uri;
use qmetaobject::prelude::*;
use qmetaobject::QByteArray;
use std::collections::HashMap;

const ROLE_URI: i32 = 0x0100;
const ROLE_NAME: i32 = 0x0101;

#[derive(QObject, Default)]
pub struct PathModel {
    base: qt_base_class!(trait QAbstractListModel),
    uri: qt_property!(QString; NOTIFY uriChanged WRITE set_uri),
    uriChanged: qt_signal!(),
    address: qt_property!(QString; NOTIFY uriChanged),
    count: qt_property!(i32; NOTIFY uriChanged),
    fullPath: qt_property!(QString; NOTIFY uriChanged),
    /// Folders above this one, parent first: `[{uri, name, icon}]`.
    ancestorsJson: qt_property!(QString; NOTIFY uriChanged),

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
        let mut ancestors: Vec<serde_json::Value> = Vec::new();
        if let (Some(core), Ok(u)) = (core(), Uri::parse(&self.uri.to_string())) {
            let steps = core.path_steps(&u);
            ancestors = steps
                .iter()
                .rev()
                .skip(1)
                .map(|s| serde_json::json!({ "uri": s.uri.to_string(), "name": s.name, "icon": s.icon }))
                .collect();
            self.steps = steps.into_iter().map(|s| (s.uri.to_string(), s.name)).collect();
            self.address = QString::from(core.edit_address(&u).as_str());
        }
        self.ancestorsJson = QString::from(to_json(&ancestors).as_str());
        self.count = self.steps.len() as i32;
        let names: Vec<&str> = self.steps.iter().map(|(_, n)| n.as_str()).collect();
        self.fullPath = QString::from(names.join(" \u{203a} ").as_str());
        self.end_reset_model();
        self.uriChanged();
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
