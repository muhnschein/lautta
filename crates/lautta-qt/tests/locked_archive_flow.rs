// SPDX-License-Identifier: LGPL-2.1-or-later
//! Locked compressed file from QML, in the real Qt event loop of `qml_check`: made with a
//! password, asked for it on open and extract (none, then a wrong one), open while a page or the
//! clipboard holds it, closed after; extract with the password lands beside it.

const FLOW: &str = r#"
import QtQuick 2.6
import Lautta 1.0

Item {
    id: root

    property bool scenarioRunning: true
    property bool begun
    property string zip
    property string archiveRoot
    property var asked: []
    property var ids: []
    property int ticks: 0

    function check(ok, what) {
        if (!ok)
            console.error("FAILED: " + what)
        return ok
    }

    function state(id) {
        var s = Transfers.summaryJson(id)
        return s === "" ? "" : JSON.parse(s).state
    }

    Timer {
        interval: 20
        repeat: true
        running: root.scenarioRunning
        onTriggered: {
            var done = root.ids.length === 1 && root.state(root.ids[0]) === "completed"
            if (!(done && !Operations.isArchive(root.archiveRoot)) && ++root.ticks <= 1500)
                return
            root.check(done, "extract ended " + root.ids.map(Transfers.summaryJson).join(" "))
            root.check(!Operations.isArchive(root.archiveRoot), "closed once the extract ended")
            root.check(JSON.stringify(root.asked) === JSON.stringify(["open", "open wrong", "extract"]),
                       "asked " + JSON.stringify(root.asked))
            root.scenarioRunning = false
        }
    }

    Component.onCompleted: {
        check(Operations.weakPassword("password"), "a common word is weak")
        check(!Operations.weakPassword("correct horse battery staple"), "four random words are not")
        check(!Operations.weakPassword(""), "no password is not a weak one")
        Operations.failed.connect(function(kind, message) { console.error("FAILED: " + kind + " " + message) })
        Operations.needsSummary.connect(function(id) { console.error("FAILED: extract asked first") })
        Operations.jobFinished.connect(function(job, ok, uri) {
            check(ok, "compress with a password")
            root.zip = uri
            Operations.openArchive(uri, "")
        })
        Operations.locked.connect(function(uri, action, wrong) {
            check(uri === root.zip, "asked about " + uri)
            root.asked = root.asked.concat([action + (wrong ? " wrong" : "")])
            if (root.asked.length === 1)
                Operations.openArchive(uri, "nope")
            else if (root.asked.length === 2)
                Operations.openArchive(uri, "pw")
            else
                Operations.extract(uri, "pw")
        })
        Operations.archiveOpened.connect(function(uri, rootUri) {
            root.archiveRoot = rootUri
            check(Operations.isArchive(rootUri), "open with the right password")
            Operations.holdArchive(rootUri)
            check(App.copy(JSON.stringify([rootUri + "src/a.txt"])), "copied from it")
            Operations.releaseArchive(rootUri)
            check(Operations.isArchive(rootUri), "the clipboard keeps it open")
            App.clearClipboard()
            check(!Operations.isArchive(rootUri), "closed with its last hold")
            Operations.extract(root.zip, "")
        })
        Operations.started.connect(function(id) { root.ids = root.ids.concat([id]) })
        // First change after creating Transfers: its start has loaded stored tasks.
        Transfers.changed.connect(function() {
            if (root.begun)
                return
            root.begun = true
            Operations.compress(JSON.stringify(["lautta://user-documents/src"]), "lautta://user-documents/", "locked", "zip", "pw")
        })
    }
}
"#;

#[test]
fn locked_archive_asks_for_its_password_and_closes_after() {
    let home = tempfile::tempdir().unwrap();
    for d in ["Documents/src", "Downloads", "Pictures"] {
        std::fs::create_dir_all(home.path().join(d)).unwrap();
    }
    let docs = home.path().join("Documents");
    std::fs::write(docs.join("src/a.txt"), b"alpha").unwrap();
    std::env::set_var("HOME", home.path());
    std::env::set_var("QT_QPA_PLATFORM", "offscreen");
    let qml = home.path().join("flow.qml");
    std::fs::write(&qml, FLOW).unwrap();

    let rc = lautta_qt::qml_check(&[qml.display().to_string()]);
    assert_eq!(rc, 0, "the QML flow reported failures (see output)");
    assert_eq!(std::fs::read(docs.join("src 2/a.txt")).unwrap(), b"alpha");
}
