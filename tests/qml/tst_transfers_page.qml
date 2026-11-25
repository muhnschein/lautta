// SPDX-License-Identifier: LGPL-2.1-or-later
import QtQuick 2.6
import Sailfish.Silica 1.0

ApplicationWindow {
    id: win

    property bool scenarioRunning: true
    property bool reachedEnd
    property int waited: 0
    property var page
    readonly property var expected: [
        ["lautta-date-in-progress", "In progress"],
        ["lautta-date-today", "Today"],
        ["lautta-date-yesterday", "Yesterday"],
        ["lautta-date-last-7-days", "Last 7 days"],
        ["lautta-date-last-30-days", "Last 30 days"],
        ["lautta-date-older", "Older"]
    ]

    ListModel {
        id: rows

        ListElement { transferId: 1; group: "active"; dateGroup: "inProgress"; kind: "copy"; title: "Copy 3 items to NAS"; name: "Copy 3 items to NAS"; state: "running"; waitReason: ""; bytesDone: 10; bytesTotal: 100; rate: 5; eta: 18; itemsDone: 0; itemsTotal: 3; itemsFailed: 0; failed: false; direction: "upload"; destination: "NAS"; destName: "NAS"; createdMs: 1; finishedMs: -1; questions: 0; progress: 0.1 }
        ListElement { transferId: 2; group: "waiting"; dateGroup: "inProgress"; kind: "copy"; title: "Download holiday.mp4"; name: "Download holiday.mp4"; state: "waiting"; waitReason: "network"; bytesDone: 0; bytesTotal: 100; rate: 0; eta: -1; itemsDone: 0; itemsTotal: 1; itemsFailed: 0; failed: false; direction: "download"; destination: "Downloads"; destName: "Downloads"; createdMs: 2; finishedMs: -1; questions: 0; progress: 0 }
        ListElement { transferId: 3; group: "paused"; dateGroup: "inProgress"; kind: "move"; title: "Backup to NAS"; name: "Backup to NAS"; state: "paused"; waitReason: ""; bytesDone: 62; bytesTotal: 100; rate: 0; eta: -1; itemsDone: 1; itemsTotal: 4; itemsFailed: 0; failed: false; direction: "upload"; destination: "NAS"; destName: "NAS"; createdMs: 3; finishedMs: -1; questions: 0; progress: 0.62 }
        ListElement { transferId: 4; group: "history"; dateGroup: "today"; kind: "copy"; title: "Copy 14 items to Documents"; name: "Copy 14 items to Documents"; state: "completed"; waitReason: ""; bytesDone: 100; bytesTotal: 100; rate: 0; eta: -1; itemsDone: 14; itemsTotal: 14; itemsFailed: 0; failed: false; direction: "local"; destination: "Documents"; destName: "Documents"; createdMs: 4; finishedMs: 40; questions: 0; progress: 1 }
        ListElement { transferId: 5; group: "history"; dateGroup: "yesterday"; kind: "copy"; title: "Upload to NAS"; name: "Upload to NAS"; state: "failed"; waitReason: ""; bytesDone: 50; bytesTotal: 100; rate: 0; eta: -1; itemsDone: 3; itemsTotal: 5; itemsFailed: 2; failed: true; direction: "upload"; destination: "NAS"; destName: "NAS"; createdMs: 5; finishedMs: 30; questions: 0; progress: 0.5 }
        ListElement { transferId: 6; group: "history"; dateGroup: "last7Days"; kind: "delete"; title: "Delete 4 items"; name: "Delete 4 items"; state: "completed"; waitReason: ""; bytesDone: 0; bytesTotal: 0; rate: 0; eta: -1; itemsDone: 4; itemsTotal: 4; itemsFailed: 0; failed: false; direction: "delete"; destination: "Documents"; destName: "Documents"; createdMs: 6; finishedMs: 20; questions: 0; progress: 0 }
        ListElement { transferId: 7; group: "history"; dateGroup: "last30Days"; kind: "copy"; title: "Copy scan.png"; name: "Copy scan.png"; state: "canceled"; waitReason: ""; bytesDone: 0; bytesTotal: 10; rate: 0; eta: -1; itemsDone: 0; itemsTotal: 1; itemsFailed: 0; failed: false; direction: "local"; destination: "Pictures"; destName: "Pictures"; createdMs: 7; finishedMs: 10; questions: 0; progress: 0 }
        ListElement { transferId: 8; group: "history"; dateGroup: "older"; kind: "copy"; title: "Copy old.csv"; name: "Copy old.csv"; state: "completed"; waitReason: ""; bytesDone: 1; bytesTotal: 1; rate: 0; eta: -1; itemsDone: 1; itemsTotal: 1; itemsFailed: 0; failed: false; direction: "local"; destination: "Documents"; destName: "Documents"; createdMs: 8; finishedMs: 5; questions: 0; progress: 1 }
    }

    initialPage: Component {
        Loader {
            source: Qt.resolvedUrl("../../qml/pages/TransfersPage.qml")
            onLoaded: {
                item.listModel = rows
                win.page = item
            }
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

    function texts(id, english) {
        return findAll(win.page, function(x) { return x.text === id || x.text === english })
    }

    function sections() {
        var views = findAll(win.page, function(x) { return x.forceLayout !== undefined && x.cacheBuffer !== undefined })
        for (var i = 0; i < views.length; ++i) {
            views[i].cacheBuffer = 100000
            views[i].forceLayout()
        }
        var found = findAll(win.page, function(x) { return x.objectName === "dateSection" })
        found.sort(function(a, b) { return a.y - b.y })
        return found.map(function(x) { return x.text })
    }

    function checkTitles() {
        var rowsOf = findAll(win.page, function(x) { return x.toServer !== undefined && x.titleParts !== undefined })
        if (rowsOf.length !== rows.count)
            return fail(rowsOf.length + " task rows for " + rows.count + " tasks")
        for (var i = 0; i < rowsOf.length; ++i) {
            var d = rowsOf[i]
            var up = d.row.direction === "upload"
            if (d.toServer !== up)
                fail("task " + d.row.transferId + " (" + d.row.direction + ") goes to a server: " + d.toServer)
            var lines = findAll(d, function(x) { return x.iconShown !== undefined })
            if (lines.length !== 1 || (!up && lines[0].iconShown))
                fail("task " + d.row.transferId + "'s title marks a server it does not go to")
        }
    }

    function inside(ancestor, item) {
        for (var p = item; p; p = p.parent) {
            if (p === ancestor)
                return true
        }
        return false
    }

    function checkHeaderPinned() {
        var host = findAll(win.page, function(x) { return x.objectName === "transfersPulleyHost" })[0]
        var header = findAll(win.page, function(x) { return x.objectName === "transfersHeader" })[0]
        var view = findAll(win.page, function(x) { return x.forceLayout !== undefined && x.cacheBuffer !== undefined })[0]
        var menus = findAll(win.page, function(x) { return x._isPullDownMenu === true })
        if (!host || !header || !view)
            return fail("the page has a pulley host, a header and a list")
        if (view.headerItem !== null || inside(view.contentItem, header))
            return fail("the header is not part of the list")
        if (menus.length !== 1 || menus[0].flickable !== host || view.pullDownMenu)
            return fail("the one pulley is the host's, not the list's")
        if (view.boundsBehavior !== Flickable.StopAtBounds)
            return fail("the list stops at its top, so a pull goes on to the host")
        if (host.contentHeight !== host.height || header.mapToItem(win.page, 0, 0).y !== 0)
            return fail("the host does not scroll and the header is at the top of the page")
        if (Math.abs(view.mapToItem(win.page, 0, 0).y - header.height) > 0.5 || !view.clip)
            return fail("the list starts under the header and is cut there")
        if (view.quickScroll)
            return fail("the date pill is the quick way through the list, not Silica's quick scroll arrows")
        view.contentY = view.originY + view.height
        var stays = header.mapToItem(win.page, 0, 0).y === 0 && host.contentY === 0
        view.contentY = view.originY
        if (!stays)
            return fail("the header stays when the list scrolls")
        return true
    }

    function checkWords(bar) {
        var now = Date.now()
        var today = new Date()
        var at = function (daysBack) {
            return new Date(today.getFullYear(), today.getMonth(), today.getDate() - daysBack, 12).getTime()
        }
        var expectWords = function (row, text, english, description, what) {
            var got = bar.words(row, now)
            if ((got.text !== text && got.text !== english) || got.description !== description)
                return fail(what + ": " + JSON.stringify(got))
            return true
        }
        var day = function (ms) { return Format.formatDate(new Date(ms), Formatter.DateMediumWithoutYear) }
        var month = new Date(at(40))
        return expectWords({ "inProgress": true, "at": at(0) }, "lautta-date-in-progress", "In progress", "", "a running task")
            && expectWords({ "inProgress": false, "at": at(0) }, "lautta-date-today", "Today", day(at(0)), "today")
            && expectWords({ "inProgress": false, "at": at(1) }, "lautta-date-yesterday", "Yesterday", day(at(1)), "yesterday")
            && expectWords({ "inProgress": false, "at": at(4) }, Qt.locale().standaloneDayName(new Date(at(4)).getDay(), Locale.LongFormat),
                           "", day(at(4)), "four days back: the weekday")
            && expectWords({ "inProgress": false, "at": at(40) }, Format.formatDate(month, Format.MonthNameStandalone),
                           "", String(month.getFullYear()), "forty days back: month and year")
    }

    function checkTopRow(bar, view, index, what) {
        var item = null
        for (var y = view.originY; y < view.originY + view.contentHeight && !item; y += 2) {
            if (view.indexAt(view.width / 2, y) === index)
                item = y
        }
        if (item === null)
            return fail(what + ": row " + index + " is not in the list")
        view.contentY = item
        var got = bar.rowAtTop()
        view.contentY = view.originY
        if (got !== index)
            return fail(what + ": the row at the top is " + index + ", got " + got)
        return true
    }

    function checkDateBar() {
        var bar = findAll(win.page, function(x) { return x.objectName === "scrollDateBar" })[0]
        var view = findAll(win.page, function(x) { return x.forceLayout !== undefined && x.cacheBuffer !== undefined })[0]
        if (!bar || bar.flickable !== view)
            return fail("the date bar follows the list")
        if (bar.visible || bar.active)
            return fail("the date bar hides while the list stands still")
        if (!checkWords(bar))
            return false
        var running = bar.rowDate(0)
        var done = bar.rowDate(4)
        var older = bar.rowDate(5)
        if (!running || !running.inProgress)
            return fail("a running task reads In progress: " + JSON.stringify(running))
        if (!done || done.inProgress || done.at !== 30)
            return fail("a finished task is dated by when it finished: " + JSON.stringify(done))
        if (!older || older.at !== 20)
            return fail("each task by its own end: " + JSON.stringify(older))
        return checkTopRow(bar, view, 4, "activity")
    }

    function matches(got) {
        if (got.length !== expected.length)
            return false
        for (var i = 0; i < got.length; ++i) {
            if (got[i] !== expected[i][0] && got[i] !== expected[i][1])
                return false
        }
        return true
    }

    Timer {
        interval: 100
        repeat: true
        running: win.scenarioRunning
        onTriggered: {
            win.waited++
            if (!win.page)
                return win.waited > 100 ? win.fail("the page did not load") : undefined
            var got = win.sections()
            if (!win.matches(got))
                return win.waited > 100 ? win.fail("sections are the date groups, got " + JSON.stringify(got)) : undefined
            if (win.texts("lautta-xfr-menu-clear-activity", "Clear activity history").length !== 1)
                return win.fail("the pulley does not say Clear activity history")
            if (win.texts("lautta-xfr-menu-settings", "Settings").length !== 0)
                return win.fail("the pulley has no Settings entry")
            win.checkTitles()
            if (win.checkHeaderPinned() !== true || win.checkDateBar() !== true)
                return
            win.reachedEnd = true
            win.scenarioRunning = false
        }
    }
}
