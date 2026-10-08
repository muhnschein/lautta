// SPDX-License-Identifier: LGPL-2.1-or-later
//! search area facades (doc/QML-API.md): search, storage settings and
//! per-location preferences.

mod search_model;
mod storage;
mod testsupport;

use crate::{cstr, qml_uri};
use qmetaobject::qml_register_type;

/// Registers this area's QML types under `Lautta 1.0`.
pub fn register() {
    let uri = qml_uri();
    qml_register_type::<search_model::SearchModel>(&uri, 1, 0, &cstr("SearchModel"));
    qml_register_type::<storage::CacheInfo>(&uri, 1, 0, &cstr("CacheInfo"));
    qml_register_type::<testsupport::SearchTestSupport>(&uri, 1, 0, &cstr("SearchTestSupport"));
}
