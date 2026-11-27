// SPDX-License-Identifier: LGPL-2.1-or-later

const FLOW: &str = r#"
import QtQuick 2.6
import Lautta 1.0

Item {
    id: root

    property bool scenarioRunning: true
    property bool begun
    property var ids: []
    property int ticks: 0

    function state(id) {
        var s = Transfers.summaryJson(id)
        return s === "" ? "" : JSON.parse(s).state
    }

    Timer {
        interval: 20
        repeat: true
        running: root.scenarioRunning
        onTriggered: {
            var states = root.ids.map(root.state)
            var ended = states.filter(function(s) { return ["completed", "failed", "canceled"].indexOf(s) >= 0 })
            if (ended.length < 2 && ++root.ticks <= 1500)
                return
            var want = ["completed", "failed"]
            if (JSON.stringify(states) !== JSON.stringify(want))
                console.error("FAILED: extracts ended " + JSON.stringify(states) + ", want " + JSON.stringify(want)
                              + ": " + root.ids.map(Transfers.summaryJson).join(" "))
            root.scenarioRunning = false
        }
    }

    Component.onCompleted: {
        Operations.failed.connect(function(kind, message) { console.error("FAILED: " + kind + " " + message) })
        Operations.needsSummary.connect(function(id) { console.error("FAILED: extract asked first") })
        Operations.started.connect(function(id) {
            root.ids = root.ids.concat([id])
            if (root.ids.length === 1)
                Operations.extract("lautta://user-documents/bad.zip", "")
        })
        Operations.jobFinished.connect(function(job, ok, uri) {
            if (!ok)
                console.error("FAILED: compress")
            Operations.extract(uri, "")
        })
        // First change after creating Transfers: its start has loaded stored tasks.
        Transfers.changed.connect(function() {
            if (root.begun)
                return
            root.begun = true
            Operations.compress(JSON.stringify(["lautta://user-documents/src"]), "lautta://user-documents/", "pack", "zip", "")
        })
    }
}
"#;

fn bad_zip(name: &[u8], data: &[u8]) -> Vec<u8> {
    let le16 = |v: usize| u16::try_from(v).unwrap().to_le_bytes();
    let le32 = |v: usize| u32::try_from(v).unwrap().to_le_bytes();
    let common = [
        &[0, 0, 0, 0, 0, 0, 0x21, 0][..],
        &le32(0xdead_beef),
        &le32(data.len()),
        &le32(data.len()),
        &le16(name.len()),
        &le16(0),
    ]
    .concat();
    let local = [&[0x50, 0x4b, 3, 4, 20, 0][..], &common, name, data].concat();
    let central = [
        &[0x50, 0x4b, 1, 2, 20, 0, 20, 0][..],
        &common,
        &[0; 10],
        &le32(0),
        name,
    ]
    .concat();
    let end = [
        &[0x50, 0x4b, 5, 6, 0, 0, 0, 0, 1, 0, 1, 0][..],
        &le32(central.len()),
        &le32(local.len()),
        &le16(0),
    ]
    .concat();
    [local, central, end].concat()
}

#[test]
fn extract_lands_beside_the_archive() {
    let home = tempfile::tempdir().unwrap();
    for d in ["Documents/src", "Downloads", "Pictures"] {
        std::fs::create_dir_all(home.path().join(d)).unwrap();
    }
    let docs = home.path().join("Documents");
    std::fs::write(docs.join("src/a.txt"), b"hello").unwrap();
    std::fs::write(docs.join("bad.zip"), bad_zip(b"b.txt", b"hello")).unwrap();
    std::env::set_var("HOME", home.path());
    std::env::set_var("QT_QPA_PLATFORM", "offscreen");
    let qml = home.path().join("flow.qml");
    std::fs::write(&qml, FLOW).unwrap();

    let rc = lautta_qt::qml_check(&[qml.display().to_string()]);
    assert_eq!(rc, 0, "the QML flow reported failures (see output)");
    assert_eq!(std::fs::read(docs.join("src 2/a.txt")).unwrap(), b"hello");
    assert_eq!(std::fs::read(docs.join("src/a.txt")).unwrap(), b"hello");
    let mut left: Vec<_> = std::fs::read_dir(&docs)
        .unwrap()
        .map(|e| e.unwrap().file_name().into_string().unwrap())
        .collect();
    left.sort();
    assert_eq!(left, ["bad.zip", "pack.zip", "src", "src 2"]);
}
