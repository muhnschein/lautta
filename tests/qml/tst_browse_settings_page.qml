// SPDX-License-Identifier: LGPL-2.1-or-later
// Settings › Browse page (BRW-12) on the target: every row of the Browse
// page with its switch, Recents among them, and a preview that follows the
// switches. A switch changes its row in place: the list keeps its position
// and its rows. A press on a drag handle shows nothing until the finger
// moves; then only the dragged row stands out and one line shows where it
// lands, in list rows (a favourite above makes them differ from the places
// in the section). Restore defaults shows every row again.
//
// The Silica window sits in a Qt Quick window: only items in one get the
// mouse events the drag needs.
import QtQuick 2.6
import QtQuick.Window 2.2 as QtQuick
import QtTest 1.0
import Sailfish.Silica 1.0
import Lautta 1.0
import "../../qml/pages"

QtQuick.Window {
    id: window

    property bool scenarioRunning: true
    // What Silica's pages look up in the window they are in.
    property var __silica_applicationwindow_instance: app
    property int step: 0
    property int waited: 0
    property var page
    property int rows
    property string folder
    property var folderRow
    property real scrolledTo
    property var handle
    property int fromRow: -1

    ViewerTools { id: tools }
    TestEvent { id: events }
    FavouritesModel { id: favourites }

    function fail(what) {
        console.error("FAILED: " + what)
        scenarioRunning = false
    }

    function expect(ok, what) {
        if (!ok)
            fail(what)
        return ok
    }

    function findAll(item, pred, out) {
        if (!item)
            return out
        // A flickable's contentItem is one of its children too.
        if (pred(item) && out.indexOf(item) < 0)
            out.push(item)
        var kids = item.children || []
        for (var i = 0; i < kids.length; ++i)
            findAll(kids[i], pred, out)
        if (item.contentItem && item.contentItem !== item)
            findAll(item.contentItem, pred, out)
        return out
    }

    // Settings rows the Browse page shows as rows of its own.
    function shownRows(layout) {
        var n = 0
        for (var i = 0; i < layout.count; ++i) {
            if (layout.sectionAt(i) !== "nearby")
                ++n
        }
        return n
    }

    // The list row of a device row, -1 when none.
    function rowOf(layout, item) {
        for (var i = 0; i < layout.count; ++i) {
            if (layout.sectionAt(i) === "device" && layout.itemAt(i) === item)
                return i
        }
        return -1
    }

    // The first device folder (not Recents or Recently deleted), "" when none.
    function deviceFolder(layout) {
        for (var i = 0; i < layout.count; ++i) {
            var item = layout.itemAt(i)
            if (layout.sectionAt(i) === "device" && item !== "deleted" && item !== "recents")
                return item
        }
        return ""
    }

    // "section/item" of every row, for failure messages.
    function describe(layout) {
        var out = []
        for (var i = 0; i < layout.count; ++i)
            out.push(layout.sectionAt(i) + "/" + layout.itemAt(i))
        return out.join(", ")
    }

    // The delegates on screen (headless, no render loop lays the list out).
    function delegates() {
        page.listView.forceLayout()
        return findAll(page.listView.contentItem, function(x) { return x.layoutItem !== undefined }, [])
    }

    function delegateOf(item) {
        var all = delegates()
        for (var i = 0; i < all.length; ++i) {
            if (all[i].layoutSection === "device" && all[i].layoutItem === item)
                return all[i]
        }
        return null
    }

    // "item" of every delegate that is dragged or a drop target.
    function marked(what) {
        return delegates().filter(function(d) { return d[what] }).map(function(d) { return d.layoutItem }).join(",")
    }

    function switchOf(row) {
        var found = findAll(row, function(x) { return x.automaticCheck !== undefined && x.checked !== undefined }, [])
        return found.length > 0 ? found[0] : null
    }

    function handleOf(row) {
        var found = findAll(row, function(x) { return x.pressY !== undefined }, [])
        return found.length > 0 ? found[0] : null
    }

    function start() {
        var layout = page.layoutModel
        rows = shownRows(layout)
        folder = deviceFolder(layout)
        if (!expect(folder !== "" && rowOf(layout, "recents") >= 0, "a device folder and Recents are listed: " + describe(layout)))
            return
        expect(page.previewCount === rows, "the preview shows every row: " + page.previewCount + " of " + rows)
        folderRow = delegateOf(folder)
        var toggle = switchOf(folderRow)
        if (!expect(toggle !== null && toggle.checked, "the row of " + folder + " has its switch on"))
            return
        // Scrolled down a little, as when the user toggles a row further down.
        page.listView.contentY = page.listView.contentY + 40
        scrolledTo = page.listView.contentY
        toggle.clicked(null)
    }

    function afterSwitch() {
        var layout = page.layoutModel
        expect(delegateOf(folder) === folderRow, "the switched row is the same row, not a rebuilt one")
        expect(page.listView.contentY === scrolledTo, "the list stays where it was: " + page.listView.contentY + ", was " + scrolledTo)
        expect(shownRows(layout) === rows, "a hidden row stays in the settings")
    }

    function pressHandle() {
        page.listView.positionViewAtIndex(rowOf(page.layoutModel, folder), ListView.Beginning)
        handle = handleOf(folderRow)
        if (!expect(handle !== null && handle.visible, "the row of " + folder + " has a drag handle"))
            return
        events.mousePress(handle, handle.width / 2, handle.height / 2, Qt.LeftButton, Qt.NoModifier, -1)
        expect(marked("dragged") === "" && marked("target") === "", "a press alone shows no drag: dragged " + marked("dragged") + ", target " + marked("target"))
        events.mouseMove(handle, handle.width / 2, handle.height / 2 + Theme.startDragDistance / 2, -1, Qt.LeftButton)
        expect(marked("dragged") === "" && marked("target") === "", "nor a move shorter than a drag")
        events.mouseMove(handle, handle.width / 2, handle.height / 2 + folderRow.height, -1, Qt.LeftButton)
        fromRow = rowOf(page.layoutModel, folder)
        var below = page.layoutModel.itemAt(fromRow + 1)
        expect(marked("dragged") === folder, "only the dragged row stands out: " + marked("dragged"))
        expect(marked("target") === below, "one line, under " + below + ": " + marked("target"))
        events.mouseRelease(handle, handle.width / 2, handle.height / 2 + folderRow.height, Qt.LeftButton, Qt.NoModifier, -1)
        expect(marked("dragged") === "" && marked("target") === "", "the drag ends on release")
    }

    Timer {
        interval: 100
        repeat: true
        running: window.scenarioRunning
        onTriggered: {
            if (!window.page)
                return
            if (++window.waited > 150)
                return window.fail("step " + window.step + " timed out: " + window.describe(window.page.layoutModel)
                                   + ", preview " + window.page.previewCount)
            var layout = window.page.layoutModel
            switch (window.step) {
            case 0:
                if (layout.count === 0 || window.page.previewCount === 0)
                    return
                window.start()
                break
            case 1:
                // The switch is off (a rebuilt row has no switch any more)
                // and the preview followed.
                var toggle = window.switchOf(window.folderRow)
                if ((toggle && toggle.checked) || window.page.previewCount !== window.rows - 1)
                    return
                window.afterSwitch()
                if (window.scenarioRunning)
                    window.pressHandle()
                break
            case 2:
                // Moved one row down, by the same delegate.
                if (window.rowOf(layout, window.folder) !== window.fromRow + 1)
                    return
                window.expect(window.delegateOf(window.folder) === window.folderRow, "the moved row is the same row")
                layout.restoreDefaults()
                break
            case 3:
                if (window.page.previewCount !== window.rows)
                    return
                favourites.removeUri("lautta://user-documents/")
                break
            case 4:
                if (favourites.idOf("lautta://user-documents/") !== -1)
                    return
                window.scenarioRunning = false
                break
            }
            window.step++
            window.waited = 0
        }
    }

    Component.onCompleted: {
        // A favourite first: list rows and places in a section differ.
        favourites.add("lautta://user-documents/", "Docs", "")
        tools.pump(300)
        page = app.pageStack.push(pageComponent, {}, PageStackAction.Immediate)
    }

    width: 540
    height: 960

    ApplicationWindow {
        id: app
    }

    Component {
        id: pageComponent
        BrowseSettingsPage { }
    }
}
