// SPDX-License-Identifier: LGPL-2.1-or-later
//! Transfers facade end to end on a temporary home. QML runs as a scenario (`scenarioRunning`) in
//! the real Qt event loop of `qml_check`: each step acts once, then waits for its condition; fails
//! by name when it never holds. Outcome (disk, engine) is checked here afterwards.

const FLOW: &str = r#"
import QtQuick 2.6
import Lautta 1.0

Item {
    id: root

    property bool scenarioRunning: true
    property var steps: []
    property int stage: 0
    property bool acted: false
    property int waited: 0
    property bool restored: false
    property int firstId: 0
    property int queuedId: 0
    property int finishedId: 0
    property bool finishedOk: false
    property int finishedCount: 0
    property var asked: []
    property var answered: []
    property bool seenWaiting: false
    property string waitingSection: ""

    function check(cond, what) {
        if (!cond)
            console.error("FAILED: " + what)
    }

    function sawWaiting(section) {
        seenWaiting = true
        waitingSection = section
    }

    function answerOpen() {
        var open = JSON.parse(Transfers.questionsJson(queuedId))
        var n = 0
        for (var i = 0; i < open.length; ++i) {
            var key = queuedId + "/" + open[i].item
            if (answered.indexOf(key) >= 0)
                continue
            answered.push(key)
            var c = open[i].choices
            var choice = c.indexOf("Merge") >= 0 ? "Merge" : (c.indexOf("Skip") >= 0 ? "Skip" : c[0])
            check(Transfers.answer(queuedId, open[i].item, choice, false), "answer accepted")
            ++n
        }
        return n
    }

    function idle() {
        return Transfers.activeCount === 0 && !Transfers.busy && Transfers.pendingCount === 0
    }

    TransfersModel { id: list }
    Repeater {
        id: rows
        model: list
        delegate: Item {
            property string g: group
            property string dg: dateGroup
            property string k: kind
            property string d: direction
            property string t: title
            property string dn: destName
            property string st: model.state
            property int q: questions
            onGChanged: if (g === "waiting") root.sawWaiting(dg)
            Component.onCompleted: if (g === "waiting") root.sawWaiting(dg)
        }
    }
    TransferItemsModel { id: items }

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
                console.error("FAILED: timed out: " + s.label)
                root.scenarioRunning = false
            }
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
        // Start-up restore ends with `changed`; queue is empty, so nothing else changes before.
        Transfers.changed.connect(function() { root.restored = true })
        Transfers.start()

        step("start-up restore finished",
             null,
             function() { return root.restored })
        step("first copy finished",
             function() {
                 Transfers.queueCopy(JSON.stringify(["lautta://user-documents/src"]), "lautta://user-downloads/")
             },
             function() {
                 return root.finishedCount === 1 && root.idle() && Transfers.barMode === "done"
                        && rows.count === 1 && rows.itemAt(0).st === "completed"
             })
        step("first copy listed",
             function() {
                 check(queuedId > 0, "queued signal carried an id")
                 check(finishedId === queuedId && finishedOk, "first copy finished ok")
                 check(Transfers.barTransferId === queuedId && Transfers.barUnfinished === 0,
                       "bar shows the finished copy: " + Transfers.barMode + " " + Transfers.barTransferId)
                 Transfers.barSeen()
                 check(Transfers.barMode === "hidden" && Transfers.barTransferId === 0, "seen in Activity, the bar hides")
                 var r = rows.itemAt(0)
                 check(r.g === "history" && r.k === "copy" && r.d === "local" && r.t === "src" && r.dn === "Downloads",
                       "history row " + r.g + "/" + r.k + "/" + r.d + "/" + r.t + "/" + r.dn + "/" + r.st)
                 check(r.dg === "today", "a copy that just ended is today's, got " + r.dg)
                 items.transferId = queuedId
             },
             function() { return items.count === 2 && JSON.parse(items.summaryJson).state === "completed" })
        step("second copy asks",
             function() {
                 var s = JSON.parse(items.summaryJson)
                 check(s.itemsDone === 2 && s.kind === "copy" && s.destName === "Downloads", "summary " + items.summaryJson)
                 firstId = queuedId
                 Transfers.queueCopy(JSON.stringify(["lautta://user-documents/src"]), "lautta://user-downloads/")
             },
             function() {
                 return queuedId !== firstId && asked.length >= 1 && seenWaiting
                        && Transfers.questionCount === 1 && Transfers.waitingCount === 1
                        && Transfers.barMode === "question"
             })
        step("second copy finished",
             function() {
                 check(waitingSection === "inProgress", "the waiting transfer is in progress, got " + waitingSection)
                 check(Transfers.barTransferId === queuedId && Transfers.barUnfinished === 1,
                       "bar leads with the question: " + Transfers.barMode + " " + Transfers.barUnfinished)
                 Transfers.dismissBar()
                 check(Transfers.barMode === "hidden", "a swipe hides the bar, got " + Transfers.barMode)
                 check(asked[0].choices.length > 0 && asked[0].name === "src" && asked[0].dst_is_dir, "conflict " + JSON.stringify(asked[0]))
                 check(answerOpen() >= 1, "there was a question to answer")
             },
             function() {
                 if (finishedCount < 2)
                     answerOpen()
                 return finishedCount === 2 && idle() && Transfers.barMode === "done" && rows.count === 2
             })
        step("history cleared",
             function() {
                 check(finishedId === queuedId && finishedOk, "second copy finished ok")
                 check(Transfers.barTransferId === queuedId, "the end shows again after a swipe, got " + Transfers.barTransferId)
                 check(rows.itemAt(0).dg === "today" && rows.itemAt(1).dg === "today", "both ended today")
                 check(Transfers.clearHistory() === 2, "history cleared")
             },
             function() { return rows.count === 0 && Transfers.barMode === "hidden" })
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
