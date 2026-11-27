// SPDX-License-Identifier: LGPL-2.1-or-later

mod index_info;
mod search_model;
mod storage;
mod testsupport;

use crate::{cstr, qml_uri};
use qmetaobject::qml_register_type;

pub fn register() {
    let uri = qml_uri();
    qml_register_type::<search_model::SearchModel>(&uri, 1, 0, &cstr("SearchModel"));
    qml_register_type::<index_info::IndexInfo>(&uri, 1, 0, &cstr("IndexInfo"));
    qml_register_type::<storage::CacheInfo>(&uri, 1, 0, &cstr("CacheInfo"));
    qml_register_type::<testsupport::SearchTestSupport>(&uri, 1, 0, &cstr("SearchTestSupport"));
}
