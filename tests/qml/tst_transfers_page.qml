// SPDX-License-Identifier: LGPL-2.1-or-later
// Scenario on the target (real Silica): Activity's sections are the rows'
// date groups (SPEC §15.4), In progress first, then Today … Older, in the
// shared DateSectionHeader; and its pulley has no Settings entry. Fails
// when the sections follow anything but `dateGroup` (such as the old
// Active / Waiting / Paused / History groups) or Settings comes back.
import QtQuick 2.6
import Sailfish.Silica 1.0

ApplicationWindow {
    id: win

    property bool scenarioRunning: true
    property int waited: 0
    property var page
    // The section headers in order: [qsTrId id, engineering English].
    readonly property var expected: [
        ["lautta-date-in-progress", "In progress"],
        ["lautta-date-today", "Today"],
        ["lautta-date-yesterday", "Yesterday"],
        ["lautta-date-last-7-days", "Last 7 days"],
        ["lautta-date-last-30-days", "Last 30 days"],
        ["lautta-date-older", "Older"]
    ]

    // Every role the delegate reads; `group` says what the task does and
    // differs from `dateGroup` on purpose.
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

    // Loaded rather than declared: a page item on its own gets no size
    // here, and its list would make no delegates.
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

    // Every item under `item` that `pred` accepts. A flickable's
    // contentItem is one of its children too: each item is looked at once.
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

    // The section headers top to bottom, all of them made.
    function sections() {
        // Headless, no render loop polishes the list: lay it out by hand.
        var views = findAll(win.page, function(x) { return x.forceLayout !== undefined && x.cacheBuffer !== undefined })
        for (var i = 0; i < views.length; ++i) {
            views[i].cacheBuffer = 100000
            views[i].forceLayout()
        }
        // The page is not on screen here, so `visible` is false throughout.
        var found = findAll(win.page, function(x) { return x.objectName === "dateSection" })
        found.sort(function(a, b) { return a.y - b.y })
        return found.map(function(x) { return x.text })
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
            if (win.texts("lautta-xfr-menu-clear", "Clear history").length !== 1)
                return win.fail("the pulley's entries are not where the test looks")
            if (win.texts("lautta-xfr-menu-settings", "Settings").length !== 0)
                return win.fail("the pulley has no Settings entry")
            win.scenarioRunning = false
        }
    }
}
