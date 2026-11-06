// SPDX-License-Identifier: LGPL-2.1-or-later
//! Conflict notifications of `TransferNotifications.qml` against real transfers on a temporary
//! home. Host has no Nemo.Notifications: stub module records what is on screen. Stub's `close()`
//! emits no `closed`, like Nemo's own: only user's dismiss or tap does.

const STUB_QMLDIR: &str =
    "module Nemo.Notifications\nNotification 1.0 Notification.qml\nNoteLog 1.0 NoteLog.js\n";

const STUB_LOG: &str = r#".pragma library
var next = 1
var shown = ({})

function count(category) {
    var n = 0
    for (var id in shown)
        if (shown[id] === category)
            ++n
    return n
}
"#;

const STUB_NOTE: &str = r#"import QtQuick 2.6
import "NoteLog.js" as NoteLog

QtObject {
    property string appName
    property string category
    property string summary
    property string body
    property real progress
    property bool resident
    property bool isTransient
    property var remoteActions
    property int itemCount
    property int replacesId

    signal clicked()
    signal closed(int reason)

    function publish() {
        if (replacesId === 0)
            replacesId = NoteLog.next++
        NoteLog.shown[replacesId] = category
    }

    function close() {
        delete NoteLog.shown[replacesId]
        replacesId = 0
    }

    function dismissByUser() {
        delete NoteLog.shown[replacesId]
        closed(2)
        replacesId = 0
    }

    function notificationsByCategory(c) {
        return []
    }
}
"#;

const FLOW: &str = r#"
import QtQuick 2.6
import Lautta 1.0
import Nemo.Notifications 1.0
import "file://COMPONENTS"

Item {
    id: root

    property bool scenarioRunning: true
    property var steps: []
    property int stage: 0
    property bool acted: false
    property int waited: 0
    property bool restored: false
    property int queuedId: 0
    property int finishedCount: 0
    property var answered: []
    property var lastNote: null

    function check(cond, what) {
        if (!cond)
            console.error("FAILED: " + what)
    }

    function questionsShown() {
        return NoteLog.count("x-lautta.transfer.question")
    }

    function notesKept() {
        return Object.keys(notes.questionNotes).length
    }

    function idle() {
        return Transfers.activeCount === 0 && !Transfers.busy && Transfers.pendingCount === 0
    }

    function answerOpen() {
        var open = JSON.parse(Transfers.questionsJson(queuedId))
        for (var i = 0; i < open.length; ++i) {
            var key = queuedId + "/" + open[i].item
            if (answered.indexOf(key) >= 0)
                continue
            answered.push(key)
            var c = open[i].choices
            check(Transfers.answer(queuedId, open[i].item, c.indexOf("Merge") >= 0 ? "Merge" : "Skip", false),
                  "answer accepted")
        }
    }

    function copy() {
        Transfers.queueCopy(JSON.stringify(["lautta://user-documents/src"]), "lautta://user-downloads/")
    }

    function asking(previous) {
        lastNote = notes.questionNotes[queuedId] || null
        return queuedId !== previous && Transfers.questionCount === 1 && questionsShown() === 1 && lastNote !== null
    }

    // Destroyed note reads as null.
    function gone() {
        return questionsShown() === 0 && notesKept() === 0 && lastNote === null
    }

    TransferNotifications { id: notes }

    function step(label, act, ok) {
        steps.push({ "label": label, "act": act, "ok": ok })
    }

    // Cap counts ticks: a loaded runner gets more time, not a false failure.
    Timer {
        interval: 20
        repeat: true
        running: root.scenarioRunning
        onTriggered: {
            var s = root.steps[root.stage]
            if (!s) {
                root.scenarioRunning = false
                return
            }
            if (!root.acted) {
                root.acted = true
                if (s.act)
                    s.act()
            }
            if (s.ok()) {
                root.stage++
                root.acted = false
                root.waited = 0
            } else if (++root.waited > 1500) {
                console.error("FAILED: timed out: " + s.label + " (shown " + root.questionsShown()
                              + ", kept " + root.notesKept() + ")")
                root.scenarioRunning = false
            }
        }
    }

    Component.onCompleted: {
        check(notes.inBackground, "offscreen app counts as in background")
        Transfers.queued.connect(function(id) { root.queuedId = id })
        Transfers.finished.connect(function(id, ok, failures) { root.finishedCount += 1 })
        Transfers.changed.connect(function() { root.restored = true })
        Transfers.start()
        var previous = 0

        step("start-up restore finished", null, function() { return root.restored })
        step("first copy finished without asking",
             function() { copy() },
             function() { return finishedCount === 1 && idle() && questionsShown() === 0 })
        step("second copy asks",
             function() { previous = queuedId; copy() },
             function() { return asking(previous) })
        step("answered conflict leaves no notification",
             function() { answerOpen() },
             function() {
                 if (finishedCount < 2)
                     answerOpen()
                 return finishedCount === 2 && idle() && gone()
             })
        step("third copy asks",
             function() { previous = queuedId; copy() },
             function() { return asking(previous) })
        step("canceled transfer leaves no notification",
             function() { check(Transfers.cancel(queuedId), "cancel accepted") },
             function() { return idle() && Transfers.questionCount === 0 && gone() })
        step("paused copy asks",
             function() { previous = queuedId; copy() },
             function() { return asking(previous) })
        step("answered while paused leaves no notification",
             function() {
                 check(Transfers.pause(queuedId), "pause accepted")
                 answerOpen()
             },
             function() { return JSON.parse(Transfers.summaryJson(queuedId)).state === "paused" && gone() })
        step("cancel paused copy",
             function() { check(Transfers.cancel(queuedId), "cancel accepted") },
             function() { return idle() })
        step("fourth copy asks",
             function() { previous = queuedId; copy() },
             function() { return asking(previous) })
        step("dismissed notification is forgotten",
             function() { lastNote.dismissByUser() },
             function() { return gone() })
        step("next conflict of same transfer shows again",
             function() {
                 check(Transfers.questionCount === 1, "transfer still waits for an answer")
                 notes.noteQuestion(queuedId)
             },
             function() {
                 lastNote = notes.questionNotes[queuedId] || null
                 return questionsShown() === 1 && notesKept() === 1 && lastNote !== null
             })
        step("cancel to finish",
             function() { check(Transfers.cancel(queuedId), "cancel accepted") },
             function() { return idle() && gone() })
        step("note of forgotten transfer closes",
             function() {
                 check(Transfers.clearHistory() > 0, "history cleared")
                 notes.noteQuestion(queuedId)
                 lastNote = notes.questionNotes[queuedId]
                 check(questionsShown() === 1 && lastNote, "note shown for the forgotten transfer")
                 notes.closeAnswered()
             },
             function() { return gone() })
    }
}
"#;

#[test]
fn conflict_notifications_close_once_nothing_waits() {
    let home = tempfile::tempdir().unwrap();
    for d in ["Documents/src", "Downloads", "Pictures"] {
        std::fs::create_dir_all(home.path().join(d)).unwrap();
    }
    std::fs::write(home.path().join("Documents/src/a.txt"), b"hello").unwrap();
    let stub = home.path().join("stub/Nemo/Notifications");
    std::fs::create_dir_all(&stub).unwrap();
    std::fs::write(stub.join("qmldir"), STUB_QMLDIR).unwrap();
    std::fs::write(stub.join("NoteLog.js"), STUB_LOG).unwrap();
    std::fs::write(stub.join("Notification.qml"), STUB_NOTE).unwrap();
    std::env::set_var("HOME", home.path());
    std::env::set_var("QT_QPA_PLATFORM", "offscreen");
    let components = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../qml/components")
        .canonicalize()
        .unwrap();
    let qml = home.path().join("flow.qml");
    let flow = FLOW.replace("COMPONENTS", &components.display().to_string());
    std::fs::write(&qml, flow).unwrap();

    let rc = lautta_qt::qml_check(&[
        "-I".to_string(),
        home.path().join("stub").display().to_string(),
        qml.display().to_string(),
    ]);
    assert_eq!(rc, 0, "the QML flow reported failures (see output)");
}
