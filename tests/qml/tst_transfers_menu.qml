// SPDX-License-Identifier: LGPL-2.1-or-later
import QtQuick 2.6
import Sailfish.Silica 1.0
import "../../qml/pages"

ApplicationWindow {
    id: win

    property bool scenarioRunning: true
    property bool reachedEnd
    property int waited: 0
    property int index: 0
    property int menusClosed: 0
    property var page
    property var openRow
    property var openMenu

    ListModel {
        id: rows

        ListElement { transferId: 1; group: "active"; dateGroup: "inProgress"; kind: "copy"; title: "Copy a.txt"; name: "Copy a.txt"; state: "running"; waitReason: ""; bytesDone: 10; bytesTotal: 100; rate: 5; eta: 18; itemsDone: 0; itemsTotal: 1; itemsFailed: 0; failed: false; direction: "local"; destination: "Documents"; destName: "Documents"; createdMs: 1; finishedMs: -1; questions: 0; progress: 0.1 }
        ListElement { transferId: 2; group: "waiting"; dateGroup: "inProgress"; kind: "copy"; title: "Copy b.txt"; name: "Copy b.txt"; state: "waiting"; waitReason: "network"; bytesDone: 0; bytesTotal: 100; rate: 0; eta: -1; itemsDone: 0; itemsTotal: 1; itemsFailed: 0; failed: false; direction: "local"; destination: "Documents"; destName: "Documents"; createdMs: 2; finishedMs: -1; questions: 0; progress: 0 }
        ListElement { transferId: 3; group: "waiting"; dateGroup: "inProgress"; kind: "copy"; title: "Copy c.txt"; name: "Copy c.txt"; state: "waiting"; waitReason: "question"; bytesDone: 0; bytesTotal: 100; rate: 0; eta: -1; itemsDone: 0; itemsTotal: 1; itemsFailed: 0; failed: false; direction: "local"; destination: "Documents"; destName: "Documents"; createdMs: 3; finishedMs: -1; questions: 1; progress: 0 }
        ListElement { transferId: 4; group: "paused"; dateGroup: "inProgress"; kind: "move"; title: "Move d.txt"; name: "Move d.txt"; state: "paused"; waitReason: ""; bytesDone: 62; bytesTotal: 100; rate: 0; eta: -1; itemsDone: 0; itemsTotal: 1; itemsFailed: 0; failed: false; direction: "local"; destination: "Documents"; destName: "Documents"; createdMs: 4; finishedMs: -1; questions: 0; progress: 0.62 }
        ListElement { transferId: 5; group: "history"; dateGroup: "today"; kind: "copy"; title: "Copy e.txt"; name: "Copy e.txt"; state: "failed"; waitReason: ""; bytesDone: 50; bytesTotal: 100; rate: 0; eta: -1; itemsDone: 0; itemsTotal: 1; itemsFailed: 1; failed: true; direction: "local"; destination: "Documents"; destName: "Documents"; createdMs: 5; finishedMs: 50; questions: 0; progress: 0.5 }
        ListElement { transferId: 6; group: "history"; dateGroup: "today"; kind: "copy"; title: "Copy f.txt"; name: "Copy f.txt"; state: "completed"; waitReason: ""; bytesDone: 1; bytesTotal: 1; rate: 0; eta: -1; itemsDone: 1; itemsTotal: 1; itemsFailed: 0; failed: false; direction: "local"; destination: "Documents"; destName: "Documents"; createdMs: 6; finishedMs: 40; questions: 0; progress: 1 }
        ListElement { transferId: 7; group: "history"; dateGroup: "today"; kind: "copy"; title: "Copy g.txt"; name: "Copy g.txt"; state: "canceled"; waitReason: ""; bytesDone: 0; bytesTotal: 1; rate: 0; eta: -1; itemsDone: 0; itemsTotal: 1; itemsFailed: 0; failed: false; direction: "local"; destination: "Documents"; destName: "Documents"; createdMs: 7; finishedMs: 30; questions: 0; progress: 0 }
    }

    initialPage: Component {
        TransfersPage {
            listModel: rows
            Component.onCompleted: win.page = this
        }
    }

    function fail(what) {
        console.error("FAILED: " + what)
        scenarioRunning = false
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

    function rowFor(index) {
        var view = findAll(win.page, function(x) { return x.forceLayout !== undefined && x.cacheBuffer !== undefined })[0]
        view.positionViewAtIndex(index, ListView.Contain)
        view.forceLayout()
        var id = rows.get(index).transferId
        var found = findAll(win.page, function(x) {
            return x.openMenu !== undefined && x.titleParts !== undefined && x.row.transferId === id
        })
        return found.length === 1 ? found[0] : null
    }

    function checkFinished(d, what) {
        if (d.menu !== null || d.openMenu() !== null)
            return fail(what + " has a long-press menu, with nothing to pick")
        if (d.menuOpen || d.highlighted)
            return fail(what + " stays highlighted after a long press")
        return true
    }

    function next() {
        index++
        openRow = null
        waited = 0
    }

    Timer {
        interval: 100
        repeat: true
        running: win.scenarioRunning
        onTriggered: {
            if (++win.waited > 100)
                return win.fail("timed out at task " + (win.index + 1))
            if (!win.page || win.page.status !== PageStatus.Active)
                return
            if (win.index === rows.count) {
                if (win.menusClosed !== 5)
                    return win.fail("five tasks offer actions, " + win.menusClosed + " menus opened and closed")
                win.reachedEnd = true
                win.scenarioRunning = false
                return
            }
            if (!win.openRow) {
                var d = win.rowFor(win.index)
                if (!d)
                    return
                var what = "task " + d.row.transferId + " (" + d.row.group + ", " + d.row.state + ")"
                if (d.row.group === "history" && d.row.state !== "failed") {
                    if (win.checkFinished(d, what) === true)
                        win.next()
                    return
                }
                if (d.menu === null)
                    return win.fail(what + " has no long-press menu")
                win.openRow = d
                win.openMenu = d.openMenu()
                return
            }
            if (win.openMenu) {
                if (!win.openMenu._expanded)
                    return
                win.openMenu.close()
                win.openMenu = null
                return
            }
            if (win.openRow.menuOpen || win.openRow.highlighted)
                return
            win.menusClosed++
            win.next()
        }
    }
}
