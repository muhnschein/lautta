// SPDX-License-Identifier: LGPL-2.1-or-later
import QtQuick 2.6
import Sailfish.Silica 1.0
import Lautta 1.0

ApplicationWindow {
    id: win

    property bool scenarioRunning: true
    property bool reachedEnd
    property int step: 0
    property int waited: 0
    property int planId: -1
    property var summary
    property var sheet
    property var first

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

    function settled() {
        return !pageStack.busy && pageStack.currentPage
    }

    function isConflict(page) {
        return page && page.choices !== undefined && page.position !== undefined
    }

    Connections {
        target: Operations
        onNeedsSummary: {
            win.planId = planId
            win.summary = JSON.parse(summaryJson)
        }
    }

    Timer {
        interval: 100
        repeat: true
        running: win.scenarioRunning
        onTriggered: {
            win.waited++
            if (win.waited > 300)
                return win.fail("step " + win.step + " timed out")
            var page = win.settled()
            switch (win.step) {
            case 0:
                if (!App.setSetting("large_op_items", JSON.stringify(1)))
                    return win.fail("large_op_items not set")
                win.install("doc.md", "lautta://user-documents/plan-conflict.md")
                win.install("data.bin", "lautta://user-documents/plan-conflict.bin")
                win.install("doc.md", "lautta://user-downloads/plan-conflict.md")
                win.install("data.bin", "lautta://user-downloads/plan-conflict.bin")
                Operations.copyTo(JSON.stringify(["lautta://user-documents/plan-conflict.md",
                                                  "lautta://user-documents/plan-conflict.bin"]),
                                  "lautta://user-downloads/")
                win.step++
                win.waited = 0
                break
            case 1:
                if (win.planId < 0)
                    break
                if (win.summary.conflicts !== 2)
                    return win.fail("two conflicts expected, not " + win.summary.conflicts)
                win.sheet = pageStack.push(Qt.resolvedUrl("../../qml/dialogs/PlanSummaryDialog.qml"),
                                           { "planId": win.planId, "summary": win.summary },
                                           PageStackAction.Immediate)
                win.step++
                win.waited = 0
                break
            case 2:
                if (page !== win.sheet)
                    break
                win.sheet.resolveNext()
                win.step++
                win.waited = 0
                break
            case 3:
                if (!win.isConflict(page))
                    break
                if (page.position !== 1 || page.total !== 2)
                    return win.fail("first conflict reads " + page.position + " of " + page.total)
                win.first = page
                win.step++
                win.waited = 0
                page.accept()
                break
            case 4:
                if (!win.isConflict(page) || page === win.first)
                    break
                if (page.position !== 2 || page.total !== 2)
                    return win.fail("second conflict reads " + page.position + " of " + page.total)
                win.step++
                win.waited = 0
                page.accept()
                break
            case 5:
                if (page !== win.sheet)
                    break
                if (win.sheet.conflictCount !== 0 || win.sheet.resolving)
                    return win.fail("the summary still counts " + win.sheet.conflictCount + " conflicts")
                win.reachedEnd = true
                win.finish()
                break
            }
        }
    }
}
