// SPDX-License-Identifier: LGPL-2.1-or-later
//! The Transfers facade end to end on a temporary home: a folder is copied
//! between two user folders through `Transfers.queueCopy`, the models follow
//! (history row, items, summary), a second copy hits a conflict that is
//! answered through `Transfers.answer`, and the history is cleared. The QML
//! below runs in the real Qt event loop of `qml_check`; it blocks briefly
//! after each step so the runtime's results are queued for the next event
//! loop pass. The outcome is checked here afterwards (disk and engine).

const FLOW: &str = r#"
import QtQuick 2.6
import Lautta 1.0

Item {
    id: root

    property int stage: 0
    property int queuedId: 0
    property int finishedId: 0
    property bool finishedOk: false
    property int finishedCount: 0
    property var asked: []
    property bool seenWaiting: false

    function check(cond, what) {
        if (!cond)
            console.error("FAILED: " + what)
    }

    function blockFor(ms) {
        var end = Date.now() + ms
        while (Date.now() < end) { }
    }

    function next() {
        step.running = false
        step.running = true
    }

    function answerOpen() {
        var open = JSON.parse(Transfers.questionsJson(queuedId))
        for (var i = 0; i < open.length; ++i) {
            var c = open[i].choices
            var choice = c.indexOf("Merge") >= 0 ? "Merge" : (c.indexOf("Skip") >= 0 ? "Skip" : c[0])
            check(Transfers.answer(queuedId, open[i].item, choice, false), "answer accepted")
        }
        return open.length
    }

    TransfersModel { id: list }
    Repeater {
        id: rows
        model: list
        delegate: Item {
            property string g: group
            property string k: kind
            property string d: direction
            property string t: title
            property string dn: destName
            property string st: model.state
            property int q: questions
            onGChanged: if (g === "waiting") root.seenWaiting = true
            Component.onCompleted: if (g === "waiting") root.seenWaiting = true
        }
    }
    TransferItemsModel { id: items }

    Timer {
        id: step
        interval: 0
        onTriggered: root.advance()
    }

    function advance() {
        stage += 1
        if (stage === 1) {
            // The start-up restore has run; now the first copy.
            Transfers.queueCopy(JSON.stringify(["lautta://user-documents/src"]), "lautta://user-downloads/")
            blockFor(400)
            next()
        } else if (stage === 2) {
            // First copy done.
            check(queuedId > 0, "queued signal carried an id")
            check(finishedCount === 1 && finishedId === queuedId && finishedOk, "first copy finished ok")
            check(Transfers.activeCount === 0 && !Transfers.busy && Transfers.pendingCount === 0, "queue is idle")
            check(rows.count === 1, "one row, got " + rows.count)
            var r = rows.itemAt(0)
            check(r.g === "history" && r.k === "copy" && r.d === "local" && r.t === "src" && r.dn === "Downloads" && r.st === "completed",
                  "history row " + r.g + "/" + r.k + "/" + r.d + "/" + r.t + "/" + r.dn + "/" + r.st)
            items.transferId = queuedId
            check(items.count === 2, "folder and file are the items, got " + items.count)
            var s = JSON.parse(items.summaryJson)
            check(s.state === "completed" && s.itemsDone === 2 && s.kind === "copy" && s.destName === "Downloads", "summary " + items.summaryJson)
            // Second copy: the folder exists now.
            Transfers.queueCopy(JSON.stringify(["lautta://user-documents/src"]), "lautta://user-downloads/")
            blockFor(400)
            next()
        } else if (stage === 3) {
            check(asked.length >= 1, "the existing folder asks, got " + asked.length)
            check(Transfers.questionCount === 1 && Transfers.waitingCount === 1, "one transfer waits for an answer")
            check(seenWaiting, "the list showed the waiting transfer")
            if (asked.length > 0)
                check(asked[0].choices.length > 0 && asked[0].name === "src" && asked[0].dst_is_dir, "conflict " + JSON.stringify(asked[0]))
            check(answerOpen() >= 1, "there was a question to answer")
            blockFor(400)
            next()
        } else if (stage === 4) {
            if (finishedCount < 2 && answerOpen() > 0)
                blockFor(400)
            next()
        } else if (stage === 5) {
            check(finishedCount === 2 && finishedOk, "second copy finished, count " + finishedCount)
            check(rows.count === 2, "two history rows, got " + rows.count)
            check(Transfers.clearHistory() === 2, "history cleared")
            blockFor(200)
            next()
        } else if (stage === 6) {
            check(rows.count === 0, "list is empty after clearing, got " + rows.count)
        }
    }

    Component.onCompleted: {
        Transfers.queued.connect(function(id) { root.queuedId = id })
        Transfers.finished.connect(function(id, ok, failures) {
            root.finishedId = id
            root.finishedOk = ok
            root.finishedCount += 1
            root.check(failures === 0, "no failures")
        })
        Transfers.needsAnswer.connect(function(id, item, conflictJson) { root.asked.push(JSON.parse(conflictJson)) })
        Transfers.failed.connect(function(kind, message) { console.error("FAILED: unexpected failure " + kind + " " + message) })
        // Let the start-up restore (loading the persisted queue) finish first.
        Transfers.start()
        blockFor(300)
        next()
    }
}
"#;

#[test]
fn copy_conflict_answer_and_history() {
    let home = tempfile::tempdir().unwrap();
    for d in ["Documents/src", "Downloads", "Pictures"] {
        std::fs::create_dir_all(home.path().join(d)).unwrap();
    }
    std::fs::write(home.path().join("Documents/src/a.txt"), b"hello").unwrap();
    std::env::set_var("HOME", home.path());
    std::env::set_var("QT_QPA_PLATFORM", "offscreen");
    let qml = home.path().join("flow.qml");
    std::fs::write(&qml, FLOW).unwrap();

    let rc = lautta_qt::qml_check(&[qml.display().to_string()]);
    assert_eq!(rc, 0, "the QML flow reported failures (see output)");

    assert_eq!(
        std::fs::read(home.path().join("Downloads/src/a.txt")).unwrap(),
        b"hello",
        "the folder was copied"
    );
    let core = lautta_qt::runtime::core().unwrap();
    assert!(
        core.engine.list().is_empty(),
        "the flow reached its last step and cleared the history"
    );
}
