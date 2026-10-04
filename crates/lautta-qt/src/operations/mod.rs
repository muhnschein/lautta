// SPDX-License-Identifier: LGPL-2.1-or-later
//! operations area facades (doc/QML-API.md): the `Operations` singleton and
//! the models behind BulkRename, Info, Permissions and Recently deleted.

mod bulkrename;
mod info;
mod permissions;
mod singleton;
mod trash;

use crate::json::from_json;
use lautta_core::{Error, Uri};
use qmetaobject::prelude::*;

pub use bulkrename::BulkRenameModel;
pub use info::InfoModel;
pub use permissions::PermissionsModel;
pub use singleton::Operations;
pub use trash::TrashModel;

/// Registers this area's QML types under `Lautta 1.0`.
pub fn register() {
    let uri = crate::qml_uri();
    qmetaobject::qml_register_singleton_type::<Operations>(&uri, 1, 0, &crate::cstr("Operations"));
    qmetaobject::qml_register_type::<BulkRenameModel>(&uri, 1, 0, &crate::cstr("BulkRenameModel"));
    qmetaobject::qml_register_type::<InfoModel>(&uri, 1, 0, &crate::cstr("InfoModel"));
    qmetaobject::qml_register_type::<PermissionsModel>(&uri, 1, 0, &crate::cstr("PermissionsModel"));
    qmetaobject::qml_register_type::<TrashModel>(&uri, 1, 0, &crate::cstr("TrashModel"));
}

pub(crate) fn parse_uri(s: &QString) -> Option<Uri> {
    Uri::parse(&s.to_string()).ok()
}

/// A JSON array of URI strings; `None` when any entry is not a URI.
pub(crate) fn parse_uris(json: &QString) -> Option<Vec<Uri>> {
    let list: Vec<String> = from_json(&json.to_string())?;
    list.iter().map(|s| Uri::parse(s).ok()).collect()
}

/// The error as QML sees it: engineering kind and detail (SPEC §20).
pub(crate) fn error_parts(e: &Error) -> (QString, QString) {
    (QString::from(e.kind.name()), QString::from(e.message.as_str()))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn uri_lists_need_every_entry_valid() {
        let ok = QString::from(r#"["lautta://user-documents/a","lautta://user-documents/b%20c"]"#);
        assert_eq!(parse_uris(&ok).map(|v| v.len()), Some(2));
        assert!(parse_uris(&QString::from(r#"["lautta://user-documents/a","nope"]"#)).is_none());
        assert!(parse_uris(&QString::from("not json")).is_none());
        assert!(parse_uri(&QString::from("lautta://user-documents/x")).is_some());
        assert!(parse_uri(&QString::from("x")).is_none());
    }

    #[test]
    fn errors_split_into_kind_and_message() {
        let e = Error::new(lautta_core::ErrorKind::NoSpace, "disk full");
        let (kind, message) = error_parts(&e);
        assert_eq!(
            (kind.to_string().as_str(), message.to_string().as_str()),
            ("NoSpace", "disk full")
        );
    }
}
