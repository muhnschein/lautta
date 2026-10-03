// SPDX-License-Identifier: LGPL-2.1-or-later
//! transfers area facades (doc/QML-API.md): the `Transfers` singleton, the
//! grouped `TransfersModel`, `TransferItemsModel` and `WorkingCopiesModel`.

mod edited;
pub mod events;
mod items;
mod list;
mod singleton;

pub use singleton::Transfers;

use crate::{cstr, qml_uri};
use qmetaobject::{qml_register_singleton_type, qml_register_type};

/// Registers this area's QML types under `Lautta 1.0`.
pub fn register() {
    qml_register_singleton_type::<Transfers>(&qml_uri(), 1, 0, &cstr("Transfers"));
    qml_register_type::<list::TransfersModel>(&qml_uri(), 1, 0, &cstr("TransfersModel"));
    qml_register_type::<items::TransferItemsModel>(&qml_uri(), 1, 0, &cstr("TransferItemsModel"));
    qml_register_type::<edited::WorkingCopiesModel>(&qml_uri(), 1, 0, &cstr("WorkingCopiesModel"));
}
