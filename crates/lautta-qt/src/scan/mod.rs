// SPDX-License-Identifier: LGPL-2.1-or-later
//! Scan facade. Logic lives in `lautta_core::scan`; this only queues work and publishes pages.

mod model;
mod rows;
mod worker;

pub use model::ScanModel;

pub fn register() {
    let uri = crate::qml_uri();
    qmetaobject::qml_register_type::<ScanModel>(&uri, 1, 0, &crate::cstr("ScanModel"));
}
