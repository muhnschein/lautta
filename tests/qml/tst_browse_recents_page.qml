// SPDX-License-Identifier: LGPL-2.1-or-later
import QtQuick 2.6
import Sailfish.Silica 1.0
import Lautta 1.0

ApplicationWindow {
    id: win

    property bool scenarioRunning: true
    property int step: 0
    property int waited: 0
    property var page
    readonly property var expected: [
        ["lautta-date-today", "Today"],
        ["lautta-date-yesterday", "Yesterday"],
        ["lautta-date-last-7-days", "Last 7 days"],
        ["lautta-date-last-30-days", "Last 30 days"],
        ["lautta-date-older", "Older"]
    ]

    ListModel {
        id: rows

        ListElement { itemId: 1; uri: "lautta://nv-account:1/srv/photos/IMG_2041.jpg"; name: "IMG_2041.jpg"; kind: "opened"; place: "NAS › /srv/photos"; at: 1; dateGroup: "today" }
        ListElement { itemId: 2; uri: "lautta://user-documents/notes.txt"; name: "notes.txt"; kind: "previewed"; place: "Documents"; at: 2; dateGroup: "today" }
        ListElement { itemId: 3; uri: "lautta://nv-account:1/invoice-0912.pdf"; name: "invoice-0912.pdf"; kind: "transferred"; place: "NAS"; at: 3; dateGroup: "yesterday" }
        ListElement { itemId: 4; uri: "lautta://user-documents/README.md"; name: "README.md"; kind: "previewed"; place: "Documents"; at: 4; dateGroup: "last7Days" }
        ListElement { itemId: 5; uri: "lautta://user-documents/budget.csv"; name: "budget.csv"; kind: "opened"; place: "Documents"; at: 5; dateGroup: "last30Days" }
        ListElement { itemId: 6; uri: "lautta://user-documents/old.csv"; name: "old.csv"; kind: "opened"; place: ""; at: 6; dateGroup: "older" }
    }

    initialPage: Component {
        Loader {
            source: Qt.resolvedUrl("../../qml/pages/RecentsPage.qml")
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
        // No render loop headless: lay out by hand.
        var views = findAll(win.page, function(x) { return x.forceLayout !== undefined && x.cacheBuffer !== undefined })
        for (var i = 0; i < views.length; ++i) {
            views[i].cacheBuffer = 100000
            views[i].forceLayout()
        }
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

    function hint() {
        var placeholders = findAll(win.page, function(x) { return x.hintText !== undefined && x.text !== undefined })
        return placeholders.length === 1 ? placeholders[0].hintText : "(" + placeholders.length + " placeholders)"
    }

    function checkPage() {
        var got = sections()
        if (!matches(got))
            return waited > 100 ? fail("sections are the date groups, got " + JSON.stringify(got)) : false
        if (texts("lautta-recents-clear", "Clear recents").length !== 1)
            return fail("the pulley's entries are not where the test looks")
        if (texts("lautta-recents-turn-off", "Turn off recents").length + texts("lautta-recents-turn-on", "Turn on recents").length !== 0)
            return fail("the pulley does not switch recents off or on")
        var boxes = findAll(win.page, function(x) { return String(x).indexOf("ComboBox") === 0 })
        if (boxes.length !== 0)
            return fail("Recents have no Show filter")
        return checkPlaces()
    }

    readonly property var places: ({
        "NAS › /srv/photos": true,
        "NAS": true,
        "Documents": false
    })

    function checkPlaces() {
        var used = findAll(win.page, function(x) {
            return typeof x.text === "string" && /lautta-recents-(opened|previewed|transferred)|Opened|Previewed|Transferred/.test(x.text)
        })
        if (used.length > 0)
            return fail("a line says how the file was used: " + used[0].text)
        var lines = findAll(win.page, function(x) { return x.iconShown !== undefined })
        if (lines.length !== rows.count)
            return fail(lines.length + " place lines for " + rows.count + " rows")
        for (var i = 0; i < lines.length; ++i) {
            var want = win.places[lines[i].text]
            if (lines[i].text !== "" && lines[i].iconShown !== want)
                return fail(lines[i].text + (want ? " has no server icon" : " has the server icon"))
        }
        return true
    }

    Timer {
        interval: 100
        repeat: true
        running: win.scenarioRunning
        onTriggered: {
            win.waited++
            if (win.waited > 150)
                return win.fail("step " + win.step + " timed out")
            switch (win.step) {
            case 0:
                if (win.page && win.checkPage() === true) {
                    App.setSetting("recents_enabled", JSON.stringify(false))
                    win.step++
                    win.waited = 0
                }
                break
            case 1:
                var h = win.hint()
                if (h === "lautta-recents-off-settings-hint" || h === "Turn them on in Settings") {
                    App.setSetting("recents_enabled", JSON.stringify(true))
                    win.scenarioRunning = false
                } else if (win.waited > 50) {
                    win.fail("with recents off the hint points to Settings, got " + h)
                }
                break
            }
        }
    }
}
