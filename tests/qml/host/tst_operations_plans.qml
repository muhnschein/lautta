// SPDX-License-Identifier: LGPL-2.1-or-later
// Host scenario of the Operations singleton with plans that wait for their
// summary sheet: starting while a conflict answer is still working on the
// plan starts it once the answer is in, a plan that is gone says so, and
// a failed plan names the item it failed on.
import QtQuick 2.6
import Lautta 1.0

Item {
    id: test

    readonly property string docs: "lautta://user-documents/"
    readonly property string downloads: "lautta://user-downloads/"
    property bool scenarioRunning: true
    property int step: 0
    property int waited: 0
    property int planId: -1
    property var failures: []
    property int startedCount: 0

    ViewerTools { id: tools }

    function fail(what) {
        console.error("FAILED: " + what)
        finish()
    }

    function finish() {
        App.setSetting("large_op_items", JSON.stringify(1000))
        scenarioRunning = false
    }

    function install(fixture, uri) {
        if (!tools.installFixture(Qt.resolvedUrl("fixtures/" + fixture), uri))
            fail("fixture " + fixture + " not installed")
    }

    function next() {
        step++
        waited = 0
    }

    Component.onCompleted: {
        Operations.needsSummary.connect(function(planId) { test.planId = planId })
        Operations.started.connect(function() { test.startedCount++ })
        Operations.failed.connect(function(kind, message, subject) {
            test.failures.push({ "kind": kind, "message": message, "subject": subject })
        })
    }

    Timer {
        interval: 50
        repeat: true
        running: test.scenarioRunning
        onTriggered: {
            test.waited++
            if (test.waited > 400)
                return test.fail("step " + test.step + " timed out")
            switch (test.step) {
            case 0:
                // Two files are a large plan once one item is the limit; the
                // second one is already at the destination (a conflict).
                if (!App.setSetting("large_op_items", JSON.stringify(1)))
                    return test.fail("large_op_items not set")
                test.install("doc.md", test.docs + "plans-a.md")
                test.install("data.bin", test.docs + "plans-b.bin")
                test.install("data.bin", test.downloads + "plans-b.bin")
                Operations.copyTo(JSON.stringify([test.docs + "plans-a.md", test.docs + "plans-b.bin"]),
                                  test.downloads)
                test.next()
                break
            case 1:
                if (test.planId < 0)
                    break
                var open = JSON.parse(Operations.conflictsJson(test.planId))
                if (open.length !== 1)
                    return test.fail("one conflict expected: " + JSON.stringify(open))
                // The answer works on the plan in the background; Start comes
                // right after it, as a quick tap on the sheet does.
                Operations.resolveConflict(test.planId, open[0].index, "KeepBoth", false)
                Operations.startPlan(test.planId)
                test.next()
                break
            case 2:
                if (test.failures.length > 0)
                    return test.fail("start during a conflict answer failed: " + JSON.stringify(test.failures))
                if (test.startedCount !== 1)
                    break
                // A plan that is gone (started already, or never made) cannot
                // start, and says why.
                Operations.startPlan(test.planId)
                Operations.startPlan(999)
                test.next()
                break
            case 3:
                if (test.failures.length < 2)
                    break
                for (var i = 0; i < 2; ++i) {
                    var gone = test.failures[i]
                    if (gone.kind !== "NotFound" || gone.message === "" || gone.subject !== "")
                        return test.fail("a plan that is gone cannot start: " + JSON.stringify(gone))
                }
                // Planning a missing item names it.
                Operations.copyTo(JSON.stringify([test.docs + "plans-missing.txt"]), test.downloads)
                test.next()
                break
            case 4:
                if (test.failures.length < 3)
                    break
                var missing = test.failures[2]
                if (missing.kind !== "NotFound" || missing.subject !== test.docs + "plans-missing.txt")
                    return test.fail("the missing item is named: " + JSON.stringify(missing))
                test.finish()
                break
            }
        }
    }
}
