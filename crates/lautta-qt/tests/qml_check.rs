// SPDX-License-Identifier: LGPL-2.1-or-later
//! `qml_check` fails a document when code it runs throws, wherever the throw site is.

use std::path::Path;

const LIB: &str = r#".pragma library
function boom(how) {
    if (how === "type")
        return undefined.length
    if (how === "throw")
        throw new Error("boom")
    return 1
}
"#;

const THROWING: &str = r#"import QtQuick 2.6
import "lib.js" as Lib
QtObject {
    Component.onCompleted: Lib.boom("type")
}
"#;

/// Engine warnings that are no exceptions, one with "Error: " inside.
const NOISY: &str = r#"import QtQuick 2.6
Item {
    property var none
    property int n: none
    Image { source: "./Error: none.png" }
}
"#;

fn check(dir: &Path, name: &str, body: &str) -> i32 {
    let qml = dir.join(name);
    std::fs::write(&qml, body).unwrap();
    lautta_qt::qml_check(&[qml.canonicalize().unwrap().display().to_string()])
}

fn calling(how: &str) -> String {
    format!(
        "import QtQuick 2.6\nimport \"lib.js\" as Lib\nItem {{ Component.onCompleted: Lib.boom(\"{how}\") }}\n"
    )
}

/// Qt takes one application per process: every run is in this test. Failing runs log QML errors on
/// purpose.
#[test]
fn uncaught_exceptions_fail_the_document() {
    let home = tempfile::tempdir().unwrap();
    std::env::set_var("HOME", home.path());
    std::env::set_var("QT_QPA_PLATFORM", "offscreen");
    let dir = tempfile::tempdir().unwrap();
    for (name, body) in [("lib.js", LIB), ("Throwing.qml", THROWING), ("Noisy.qml", NOISY)] {
        std::fs::write(dir.path().join(name), body).unwrap();
    }
    let dir = dir.path();

    assert_eq!(check(dir, "tst_calm.qml", &calling("none")), 0);
    assert_ne!(
        check(dir, "tst_throw.qml", &calling("throw")),
        0,
        "a library's throw in a handler passes"
    );
    assert_ne!(
        check(dir, "tst_type.qml", &calling("type")),
        0,
        "a library's TypeError in a handler passes"
    );
    let binding = "import QtQuick 2.6\nimport \"lib.js\" as Lib\nItem { width: Lib.boom(\"throw\") }\n";
    assert_ne!(
        check(dir, "tst_binding.qml", binding),
        0,
        "a library's throw in a binding passes"
    );
    let component = "import QtQuick 2.6\nItem { Throwing {} }\n";
    assert_ne!(
        check(dir, "tst_component.qml", component),
        0,
        "a component's TypeError passes"
    );
    let noisy = "import QtQuick 2.6\nItem { Noisy {} }\n";
    assert_eq!(
        check(dir, "tst_noisy.qml", noisy),
        0,
        "other files' warnings fail"
    );
}
