# qttypes 0.2.12 (patched)

Copy of the crates.io release `qttypes 0.2.12` (MIT, © Olivier Goffart and the
qmetaobject-rs contributors, https://github.com/woboq/qmetaobject-rs).

One change: `build.rs` links `Qt5Widgets` only with the new `qtwidgets` feature.
Harbour's allowed-library list (sdk-harbour-rpmvalidator) does not contain
QtWidgets and Lautta never uses it.
