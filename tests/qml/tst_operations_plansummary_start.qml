// SPDX-License-Identifier: LGPL-2.1-or-later
import QtQuick 2.6
import Sailfish.Silica 1.0
import Lautta 1.0

ApplicationWindow {
    id: win

    property bool scenarioRunning: true
    property int step: 0
    property int waited: 0
    property int planId: -1
    property var summary
    property var sheet

    ViewerTools { id: tools }

    initialPage: Component {
        Page { }
    }

    function fail(what) {
        console.error("FAILED: " + what)
        finish()
    }

    function finish() {
        App.setSetting("large_op_items", JSON.stringify(1000))
        scenarioRunning = false
    }

    function install(fixture, uri) {
        if (!tools.installFixture(Qt.resolvedUrl("host/fixtures/" + fixture), uri))
            fail("fixture " + fixture + " not installed")
    }

    Connections {
        target: Operations
        onNeedsSummary: {
            win.planId = planId
            win.summary = JSON.parse(summaryJson)
        }
        onStarted: {
            if (win.step === 3)
                win.finish()
            else
                win.fail("started in step " + win.step)
        }
        onFailed: win.fail("the plan did not start: " + kind + " " + message)
    }

    Timer {
        interval: 100
        repeat: true
        running: win.scenarioRunning
        onTriggered: {
            win.waited++
            if (win.waited > 300)
                return win.fail("step " + win.step + " timed out")
            switch (win.step) {
            case 0:
                if (!App.setSetting("large_op_items", JSON.stringify(1)))
                    return win.fail("large_op_items not set")
                win.install("doc.md", "lautta://user-documents/plan-start.md")
                win.install("data.bin", "lautta://user-documents/plan-start.bin")
                Operations.copyTo(JSON.stringify(["lautta://user-documents/plan-start.md",
                                                  "lautta://user-documents/plan-start.bin"]),
                                  "lautta://user-downloads/")
                win.step++
                win.waited = 0
                break
            case 1:
                if (win.planId < 0)
                    break
                win.sheet = pageStack.push(Qt.resolvedUrl("../../qml/dialogs/PlanSummaryDialog.qml"),
                                           { "planId": win.planId, "summary": win.summary },
                                           PageStackAction.Immediate)
                win.step++
                win.waited = 0
                break
            case 2:
                if (pageStack.busy || pageStack.currentPage !== win.sheet)
                    break
                win.step++
                win.waited = 0
                win.sheet.accept()
                break
            }
        }
    }
}
