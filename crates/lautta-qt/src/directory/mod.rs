// SPDX-License-Identifier: LGPL-2.1-or-later
//! directory area facades (doc/QML-API.md): the folder list model, the path
//! model of the header menu and the folder picker's root list.

mod model;
mod path;
mod picker;

pub use model::DirectoryModel;
pub use path::PathModel;
pub use picker::PickerRootsModel;

/// Registers this area's QML types under `Lautta 1.0`.
pub fn register() {
    let uri = crate::qml_uri();
    qmetaobject::qml_register_type::<DirectoryModel>(&uri, 1, 0, &crate::cstr("DirectoryModel"));
    qmetaobject::qml_register_type::<PathModel>(&uri, 1, 0, &crate::cstr("PathModel"));
    qmetaobject::qml_register_type::<PickerRootsModel>(&uri, 1, 0, &crate::cstr("PickerRootsModel"));
}
