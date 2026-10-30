// SPDX-License-Identifier: LGPL-2.1-or-later
//! Facades: folder list model, header path model, folder picker roots. Sample folder of the view
//! options preview is `App.previewRows`.

mod model;
mod path;
mod picker;
pub(crate) mod preview;

pub use model::DirectoryModel;
pub use path::PathModel;
pub use picker::PickerRootsModel;

pub fn register() {
    let uri = crate::qml_uri();
    qmetaobject::qml_register_type::<DirectoryModel>(&uri, 1, 0, &crate::cstr("DirectoryModel"));
    qmetaobject::qml_register_type::<PathModel>(&uri, 1, 0, &crate::cstr("PathModel"));
    qmetaobject::qml_register_type::<PickerRootsModel>(&uri, 1, 0, &crate::cstr("PickerRootsModel"));
}
