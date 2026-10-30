// SPDX-License-Identifier: LGPL-2.1-or-later
import QtQuick 2.6
import Lautta 1.0

Item {
    id: test

    readonly property string dir: "lautta://user-documents/"
    property bool scenarioRunning: true
    property int step: 0
    property int waited: 0
    property string archive
    property string extracted

    ViewerTools { id: tools }

    function fail(what) {
        console.error("FAILED: " + what)
        scenarioRunning = false
    }

    Component.onCompleted: {
        Operations.jobFinished.connect(function (jobId, ok, resultUri) { if (ok) test.archive = resultUri })
        Operations.extractFinished.connect(function (transferId, ok, destUri) { if (ok) test.extracted = destUri })
        Operations.failed.connect(function (kind, message) { test.fail(kind + ": " + message) })
    }

    Timer {
        interval: 50
        repeat: true
        running: test.scenarioRunning
        onTriggered: {
            if (++test.waited > 200)
                return test.fail("step " + test.step + " timed out")
            switch (test.step) {
            case 0:
                if (!tools.installFixture(Qt.resolvedUrl("fixtures/doc.md"), test.dir + "xh.md"))
                    return test.fail("fixture not installed")
                Operations.compress(JSON.stringify([test.dir + "xh.md"]), test.dir, "xh", "zip")
                test.step++
                break
            case 1:
                if (test.archive.length === 0)
                    break
                Operations.extractHere(test.archive)
                // Holds the GUI thread: transfer finishes before its start reaches Operations.
                for (var t = Date.now(); Date.now() - t < 1000;) { }
                test.step++
                break
            case 2:
                if (test.extracted.length === 0)
                    break
                if (test.extracted !== test.dir + "xh")
                    return test.fail("extracted to " + test.extracted)
                test.scenarioRunning = false
            }
        }
    }
}
