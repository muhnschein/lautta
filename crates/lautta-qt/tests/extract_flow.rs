// SPDX-License-Identifier: LGPL-2.1-or-later
//! Extract and open on a temporary home, in the real Qt event loop of `qml_check`. First two
//! extracts end on their worker before the facade hears they started; the first runs before the
//! facade listens for transfer events at all. Last one meets a planted file and waits on that
//! question until answered after its start is heard, so its end comes as a transfer event.

const FLOW: &str = r#"
import QtQuick 2.6
import Lautta 1.0

Item {
    id: root

    property bool scenarioRunning: true
    property var opened: []
    property int ticks: 0
    property int lastId: 0
    property bool awaitBig: false
    property int bigId: 0
    property bool bigAsked: false

    function ended(id) {
        var s = Transfers.summaryJson(id)
        return s !== "" && JSON.parse(s).kind === "extract"
            && ["completed", "failed", "canceled"].indexOf(JSON.parse(s).state) >= 0
    }

    // Transfer ids count up from 1 in a fresh home.
    function extract(archive, dest, hold) {
        Operations.extractAndOpen(archive, dest)
        var id = ++root.lastId
        if (!hold) {
            root.awaitBig = true
            return
        }
        // Holds the event loop, so the facade hears of the start only after the end.
        var until = Date.now() + 30000
        while (!ended(id) && Date.now() < until) { }
        if (!ended(id))
            console.error("FAILED: extract " + id + " did not end within 30 s: " + Transfers.summaryJson(id))
    }

    Timer {
        interval: 20
        repeat: true
        running: root.scenarioRunning
        onTriggered: {
            var asked = root.bigId ? JSON.parse(Transfers.questionsJson(root.bigId)) : []
            for (var i = 0; i < asked.length; ++i) {
                root.bigAsked = true
                if (!Transfers.answer(root.bigId, asked[i].item, asked[i].dst_is_dir ? "Merge" : "Replace", false))
                    console.error("FAILED: answer " + JSON.stringify(asked[i]))
            }
            if (root.opened.length < 3 && ++root.ticks <= 2500)
                return
            var want = ["lautta://user-documents/out", "failed", "lautta://user-documents/out-big"]
            if (JSON.stringify(root.opened) !== JSON.stringify(want))
                console.error("FAILED: extracts reported " + JSON.stringify(root.opened) + ", want " + JSON.stringify(want)
                              + "; big extract " + root.bigId + ": " + Transfers.summaryJson(root.bigId))
            if (!root.bigAsked)
                console.error("FAILED: the big extract never asked about the planted folder and file")
            root.scenarioRunning = false
        }
    }

    Component.onCompleted: {
        Operations.failed.connect(function(kind, message) { console.error("FAILED: " + kind + " " + message) })
        Operations.started.connect(function(id) {
            if (!root.awaitBig)
                return
            root.awaitBig = false
            root.bigId = id
        })
        Operations.jobFinished.connect(function(job, ok, uri) {
            if (!ok)
                console.error("FAILED: compress")
            var big = uri.indexOf("big.zip") >= 0
            root.extract(uri, "lautta://user-documents/out" + (big ? "-big" : ""), !big)
        })
        Operations.extractFinished.connect(function(id, ok, dest) {
            root.opened = root.opened.concat([ok ? dest : "failed"])
            root.lastId = id
            if (root.opened.length === 1)
                root.extract("lautta://user-documents/bad.zip", "lautta://user-documents/out-bad", true)
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
fn extract_and_open_reports_a_quick_extract() {
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
    std::fs::create_dir_all(home.path().join("Documents/out-big/big")).unwrap();
    std::fs::write(home.path().join("Documents/out-big/big/noise.bin"), b"planted").unwrap();
    std::env::set_var("HOME", home.path());
    std::env::set_var("QT_QPA_PLATFORM", "offscreen");
    let qml = home.path().join("flow.qml");
    std::fs::write(&qml, FLOW).unwrap();

    let rc = lautta_qt::qml_check(&[qml.display().to_string()]);
    assert_eq!(rc, 0, "the QML flow reported failures (see output)");
    assert_eq!(
        std::fs::read(home.path().join("Documents/out/src/a.txt")).unwrap(),
        b"hello",
        "the archive was extracted"
    );
    assert_eq!(
        std::fs::read(home.path().join("Documents/out-big/big/noise.bin")).unwrap(),
        noise,
        "the big archive was extracted"
    );
}
