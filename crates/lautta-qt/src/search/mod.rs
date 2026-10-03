// SPDX-License-Identifier: LGPL-2.1-or-later
//! search area facades (doc/QML-API.md): search, compare and sync, storage
//! settings and per-location preferences.

mod compare_model;
mod search_model;
mod storage;
mod sync_pairs;
mod testsupport;

use crate::{cstr, qml_uri};
use qmetaobject::qml_register_type;

/// Registers this area's QML types under `Lautta 1.0`.
pub fn register() {
    let uri = qml_uri();
    qml_register_type::<search_model::SearchModel>(&uri, 1, 0, &cstr("SearchModel"));
    qml_register_type::<compare_model::CompareModel>(&uri, 1, 0, &cstr("CompareModel"));
    qml_register_type::<sync_pairs::SyncPairsModel>(&uri, 1, 0, &cstr("SyncPairsModel"));
    qml_register_type::<storage::CacheInfo>(&uri, 1, 0, &cstr("CacheInfo"));
    qml_register_type::<storage::LocationPrefsModel>(&uri, 1, 0, &cstr("LocationPrefsModel"));
    qml_register_type::<testsupport::SearchTestSupport>(&uri, 1, 0, &cstr("SearchTestSupport"));
}
