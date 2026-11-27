// SPDX-License-Identifier: LGPL-2.1-or-later
import QtQuick 2.6
import Sailfish.Silica 1.0
import Lautta 1.0
import "../../qml/pages"

ApplicationWindow {
    id: win

    readonly property string source: "lautta://user-documents/transfer-remove.md"
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

    Connections {
        target: Transfers
        onQueued: win.transferId = id
        onFailed: win.fail("Transfers failed: " + kind + " " + message)
    }

    initialPage: Component {
        TransfersPage {
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

    function rowOf(id) {
        var found = findAll(win.page, function(x) {
            return x.openMenu !== undefined && x.titleParts !== undefined && x.row.transferId === id
        })
        return found.length === 1 ? found[0] : null
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
                if (!tools.installFixture(Qt.resolvedUrl("host/fixtures/doc.md"), win.source))
                    return win.fail("fixture")
                Transfers.queueCopy(JSON.stringify([win.source]), win.folder)
                win.next()
                break
            case 1:
                if (win.transferId === 0 || win.state() !== "completed")
                    break
                win.next()
                break
            case 2:
                win.row = win.rowOf(win.transferId)
                if (!win.row || win.row.row.group !== "history")
                    break
                win.menu = win.row.openMenu()
                win.next()
                break
            case 3:
                if (!win.menu._expanded)
                    break
                var items = win.findAll(win.menu, function(x) {
                    return x.hasOwnProperty("__silica_menuitem") && x.text === qsTrId("lautta-xfr-menu-remove")
                })
                if (items.length !== 1)
                    return win.fail("one Remove from Activity in the menu")
                win.menu._activatedMenuItem(items[0])
                win.next()
                break
            case 4:
                if (win.row.menuOpen)
                    break
                if (win.state() !== "completed")
                    return win.fail("removed before the remorse ran out")
                win.next()
                break
            case 5:
                if (win.state() !== "" || win.rowOf(win.transferId) !== null)
                    break
                win.reachedEnd = true
                win.finish()
                break
            }
        }
    }
}
