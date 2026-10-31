// SPDX-License-Identifier: LGPL-2.1-or-later
//! Extract here on a temporary home, in the real Qt event loop of `qml_check`. First two
//! extracts end on their worker before the facade hears they started; the first runs before the
//! facade listens for transfer events at all. Last one is big enough to still run then.

const FLOW: &str = r#"
import QtQuick 2.6
import Lautta 1.0

Item {
    id: root

    property bool scenarioRunning: true
    property var opened: []
    property int ticks: 0

    function extract(archive, hold) {
        Operations.extractHere(archive)
        // Holds the event loop until the extract has ended.
        var until = Date.now() + (hold ? 500 : 0)
        while (Date.now() < until) { }
    }

    Timer {
        interval: 20
        repeat: true
        running: root.scenarioRunning
        onTriggered: {
            if (root.opened.length < 3 && ++root.ticks <= 1500)
                return
            var want = ["lautta://user-documents/src%202", "failed", "lautta://user-documents/big%202"]
            if (JSON.stringify(root.opened) !== JSON.stringify(want))
                console.error("FAILED: extracts reported " + JSON.stringify(root.opened) + ", want " + JSON.stringify(want))
            root.scenarioRunning = false
        }
    }

    Component.onCompleted: {
        Operations.failed.connect(function(kind, message) { console.error("FAILED: " + kind + " " + message) })
        Operations.jobFinished.connect(function(job, ok, uri) {
            if (!ok)
                console.error("FAILED: compress")
            root.extract(uri, uri.indexOf("big.zip") < 0)
        })
        Operations.extractFinished.connect(function(id, ok, dest) {
            root.opened = root.opened.concat([ok ? dest : "failed"])
            if (root.opened.length === 1)
                root.extract("lautta://user-documents/bad.zip", true)
            else if (root.opened.length === 2)
                Operations.compress(JSON.stringify(["lautta://user-documents/big"]), "lautta://user-documents/", "big", "zip")
        })
        Operations.compress(JSON.stringify(["lautta://user-documents/src"]), "lautta://user-documents/", "pack", "zip")
    }
}
"#;

/// One stored entry whose CRC-32 does not match its data.
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
fn extract_here_reports_a_quick_extract() {
    let home = tempfile::tempdir().unwrap();
    for d in ["Documents/src", "Documents/big", "Downloads", "Pictures"] {
        std::fs::create_dir_all(home.path().join(d)).unwrap();
    }
    std::fs::write(home.path().join("Documents/src/a.txt"), b"hello").unwrap();
    std::fs::write(home.path().join("Documents/bad.zip"), bad_zip(b"b.txt", b"hello")).unwrap();
    let mut x = 1u32;
    let noise: Vec<u8> = (0..8 << 20)
        .map(|_| {
            x ^= x << 13;
            x ^= x >> 17;
            x ^= x << 5;
            x.to_le_bytes()[0]
        })
        .collect();
    std::fs::write(home.path().join("Documents/big/noise.bin"), &noise).unwrap();
    std::env::set_var("HOME", home.path());
    std::env::set_var("QT_QPA_PLATFORM", "offscreen");
    let qml = home.path().join("flow.qml");
    std::fs::write(&qml, FLOW).unwrap();

    let rc = lautta_qt::qml_check(&[qml.display().to_string()]);
    assert_eq!(rc, 0, "the QML flow reported failures (see output)");
    assert_eq!(
        std::fs::read(home.path().join("Documents/src 2/a.txt")).unwrap(),
        b"hello",
        "the archive was extracted"
    );
    assert_eq!(
        std::fs::read(home.path().join("Documents/big 2/noise.bin")).unwrap(),
        noise,
        "the big archive was extracted"
    );
}
