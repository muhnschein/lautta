// SPDX-License-Identifier: LGPL-2.1-or-later
import QtQuick 2.6
import Sailfish.Silica 1.0
import Lautta 1.0
import "../../qml/pages"

ApplicationWindow {
    id: win

    readonly property string source: "lautta://user-documents/transfer-cancel.md"
    readonly property string folder: "lautta://user-downloads/"
    property bool scenarioRunning: true
    property bool reachedEnd
    property int step: 0
    property int waited: 0
    property int transferId: 0
    property var page
    property var row
    property var menu
    property var remorseBefore

    ViewerTools { id: tools }

    ListModel { id: rows }

    Connections {
        target: Transfers
        onQueued: win.transferId = id
        onFailed: win.fail("Transfers failed: " + kind + " " + message)
    }

    initialPage: Component {
        TransfersPage {
            listModel: rows
            Component.onCompleted: win.page = this
        }
    }

    function fail(what) {
        console.error("FAILED: " + what)
        finish()
    }

    function finish() {
        App.setSetting("remorse_seconds", JSON.stringify(remorseBefore))
        scenarioRunning = false
    }

    function state() {
        var s = Transfers.summaryJson(transferId)
        return s ? JSON.parse(s).state : ""
    }

    function findAll(item, pred, out) {
        if (out === undefined) {
            out = []
            out.seen = []
        }
        if (!item || out.seen.indexOf(item) >= 0)
            return out
        out.seen.push(item)
        if (pred(item))
            out.push(item)
        var kids = item.children || []
        for (var i = 0; i < kids.length; ++i)
            findAll(kids[i], pred, out)
        if (item.contentItem && item.contentItem !== item)
            findAll(item.contentItem, pred, out)
        return out
    }

    function next() {
        step++
        waited = 0
    }

    Timer {
        interval: 100
        repeat: true
        running: win.scenarioRunning
        onTriggered: {
            if (++win.waited > 100)
                return win.fail("step " + win.step + " timed out, transfer " + win.transferId + " " + win.state())
            switch (win.step) {
            case 0:
                if (!win.page || win.page.status !== PageStatus.Active)
                    break
                win.remorseBefore = App.setting("remorse_seconds")
                App.setSetting("remorse_seconds", JSON.stringify(1))
                if (!tools.installFixture(Qt.resolvedUrl("host/fixtures/doc.md"), win.source)
                        || !tools.installFixture(Qt.resolvedUrl("host/fixtures/doc.md"), win.folder + "transfer-cancel.md"))
                    return win.fail("fixtures")
                // Same name in target: transfer waits for an answer, stays cancellable.
                Transfers.queueCopy(JSON.stringify([win.source]), win.folder)
                win.next()
                break
            case 1:
                if (win.transferId === 0 || win.state() !== "waiting")
                    break
                rows.append({ transferId: win.transferId, group: "waiting", dateGroup: "inProgress", kind: "copy",
                    title: "Copy doc.md", name: "Copy doc.md", state: "waiting", waitReason: "question",
                    bytesDone: 0, bytesTotal: 1, rate: 0, eta: -1, itemsDone: 0, itemsTotal: 1, itemsFailed: 0,
                    failed: false, direction: "local", destination: "Downloads", destName: "Downloads",
                    createdMs: 1, finishedMs: -1, questions: 1, progress: 0 })
                win.next()
                break
            case 2:
                var views = win.findAll(win.page, function(x) { return x.forceLayout !== undefined && x.cacheBuffer !== undefined })
                for (var i = 0; i < views.length; ++i)
                    views[i].forceLayout()
                var found = win.findAll(win.page, function(x) { return x.openMenu !== undefined && x.titleParts !== undefined })
                if (found.length !== 1)
                    break
                win.row = found[0]
                win.menu = win.row.openMenu()
                win.next()
                break
            case 3:
                if (!win.row.menuOpen)
                    break
                var items = win.findAll(win.menu, function(x) {
                    return x.hasOwnProperty("__silica_menuitem") && (x.text === "Cancel" || x.text === "lautta-xfr-menu-cancel")
                })
                if (items.length !== 1)
                    return win.fail("one Cancel in the menu")
                win.menu._activatedMenuItem(items[0])
                win.next()
                break
            case 4:
                // Menu closes long before the remorse runs out, as on the phone.
                if (win.row.menuOpen)
                    break
                if (win.state() !== "waiting")
                    return win.fail("cancelled before the remorse ran out: " + win.state())
                win.next()
                break
            case 5:
                if (win.state() !== "canceled")
                    break
                win.reachedEnd = true
                win.finish()
                break
            }
        }
    }
}
