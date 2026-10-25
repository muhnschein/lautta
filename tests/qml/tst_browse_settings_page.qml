// SPDX-License-Identifier: LGPL-2.1-or-later
// Settings › Browse page (BRW-12) on the target, rows as Salama's tab group
// rows: every row of the Browse page with its switch, Recents first in On
// this device, and a preview that follows the switches. A tap on a row
// switches it in place: the list keeps its position and its rows. A press
// on a row's grip picks up that row only; moved by a row's height it moves
// at once, by the same delegate (the rows it passes slide aside), it stays
// there once let go and stored, in list rows (a favourite above makes them
// differ from the places in the section). Restore defaults shows every row
// again with Recents first. A favourite added elsewhere shows in the list
// and the preview, as the Browse page shows it. Volumes and Servers lead
// with a switch for the whole section: off, it is a row of its own with its
// switch off and no grip (the target has no volumes, so it shows only
// because it is off, and no servers, which need the bridge), a tap on it or
// Restore defaults turns the section on and the row goes; the rows of such a
// section stand a step in from the switch.
//
// The Silica window sits in a Qt Quick window: only items in one get the
// mouse events the drag needs.
import QtQuick 2.6
import QtQuick.Window 2.2 as QtQuick
import QtTest 1.0
import Sailfish.Silica 1.0
import Lautta 1.0
import "../../qml/pages"
import "../../qml/components/browse"

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
    property int fromRow: -1
    property int listed

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

    // How many rows of `section` the list has.
    function sectionRows(layout, section) {
        var n = 0
        for (var i = 0; i < layout.count; ++i) {
            if (layout.sectionAt(i) === section)
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

    // The first device row, "" when none.
    function firstDeviceRow(layout) {
        for (var i = 0; i < layout.count; ++i) {
            if (layout.sectionAt(i) === "device")
                return layout.itemAt(i)
        }
        return ""
    }

    // The first device folder (not Recents or Trash), "" when none.
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

    // The delegates, all of them made (headless, no render loop lays the
    // list out, and the list is shorter than the window).
    function delegates() {
        page.listView.cacheBuffer = 100000
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

    // "item" of every delegate that is carried, or lit.
    function marked(what) {
        return delegates().filter(function(d) { return d[what] }).map(function(d) { return d.layoutItem }).join(",")
    }

    // The delegate of the switch of `section`, null when it has none.
    function switchOf(section) {
        var all = delegates()
        for (var i = 0; i < all.length; ++i) {
            if (all[i].layoutSection === section && all[i].layoutKind === "section")
                return all[i]
        }
        return null
    }

    // The switch row of Volumes, off: its row, its switch and no grip.
    function checkSectionSwitch(row) {
        var toggle = named(row, "layoutSwitch")
        var grip = named(row, "layoutGrip")
        expect(toggle !== null && toggle.visible && !toggle.checked, "the Volumes switch is off")
        expect(grip === null || !grip.visible, "the Volumes switch has no grip")
        expect(row.title === "lautta-browse-layout-volumes", "the Volumes switch is named by its own text: " + row.title)
        expect(!row.layoutNested && row.layoutMovable === false, "the switch is neither nested nor movable")
        var layout = page.layoutModel
        for (var i = 0; i < layout.count; ++i) {
            if (layout.sectionAt(i) === "volumes" && layout.itemAt(i) === "section")
                expect(layout.placeAt(i) === -1, "the switch takes no place in its section: " + layout.placeAt(i))
        }
    }

    // A row of a section with a switch stands in from it by two paddings.
    function checkIndent() {
        var flat = indentComponent.createObject(window.contentItem, { "layoutNested": false })
        var nested = indentComponent.createObject(window.contentItem, { "layoutNested": true })
        var at = function(row) { return named(row, "layoutSwitch").mapToItem(row, 0, 0).x }
        expect(at(nested) - at(flat) === 2 * Theme.paddingLarge,
               "a nested row's switch stands in by " + (at(nested) - at(flat)) + ", not " + 2 * Theme.paddingLarge)
        var label = function(row) { return findAll(row, function(x) { return x.text === row.title }, [])[0].mapToItem(row, 0, 0).x }
        expect(label(nested) - label(flat) === 2 * Theme.paddingLarge, "and so does its name")
        flat.destroy()
        nested.destroy()
    }

    // The rows a carried row can pass are the section's own: the switch
    // that leads the section has no place and is not one of them.
    function checkSectionEnds() {
        var row = indentComponent.createObject(window.contentItem, { "layoutModel": sectionStub })
        row.sectionEnds(2)
        expect(row.firstRow === 1, "a carried row stops below the section's switch: first row " + row.firstRow)
        expect(row.lastRow === 2, "and at the section's last row: " + row.lastRow)
        row.destroy()
    }

    function named(row, name) {
        var found = findAll(row, function(x) { return x.objectName === name }, [])
        return found.length > 0 ? found[0] : null
    }

    function start() {
        var layout = page.layoutModel
        rows = shownRows(layout)
        folder = deviceFolder(layout)
        if (!expect(folder !== "" && firstDeviceRow(layout) === "recents",
                    "Recents is the first row of On this device: " + describe(layout)))
            return
        expect(page.previewCount === rows, "the preview shows every row: " + page.previewCount + " of " + rows)
        folderRow = delegateOf(folder)
        var toggle = named(folderRow, "layoutSwitch")
        if (!expect(toggle !== null && toggle.checked, "the row of " + folder + " has its switch on"))
            return
        // Scrolled down a little, as when the user switches a row further down.
        page.listView.contentY = page.listView.contentY + 40
        scrolledTo = page.listView.contentY
        // A tap on the row switches it.
        folderRow.clicked(null)
    }

    function afterSwitch() {
        var layout = page.layoutModel
        expect(delegateOf(folder) === folderRow, "the switched row is the same row, not a rebuilt one")
        expect(page.listView.contentY === scrolledTo, "the list stays where it was: " + page.listView.contentY + ", was " + scrolledTo)
        expect(shownRows(layout) === rows, "a hidden row stays in the settings")
    }

    function carry() {
        var layout = page.layoutModel
        page.listView.positionViewAtIndex(rowOf(layout, folder), ListView.Beginning)
        var grip = named(folderRow, "layoutGrip")
        if (!expect(grip !== null && grip.visible, "the row of " + folder + " has a grip"))
            return
        fromRow = rowOf(layout, folder)
        var below = layout.itemAt(fromRow + 1)
        events.mousePress(grip, grip.width / 2, grip.height / 2, Qt.LeftButton, Qt.NoModifier, -1)
        expect(marked("carried") === folder, "a press on the grip picks up its row only: " + marked("carried"))
        expect(marked("highlighted") === "", "and lights no row as pressed: " + marked("highlighted"))
        events.mouseMove(grip, grip.width / 2, grip.height / 2 + folderRow.contentHeight, -1, Qt.LeftButton)
        expect(rowOf(layout, folder) === fromRow + 1 && layout.itemAt(fromRow) === below,
               "moved by a row it moves at once, past " + below + ": " + describe(layout))
        expect(delegateOf(folder) === folderRow && folderRow.carried, "by the same delegate, still carried")
        events.mouseRelease(grip, grip.width / 2, grip.height / 2 + folderRow.contentHeight, Qt.LeftButton, Qt.NoModifier, -1)
        expect(marked("carried") === "", "let go, no row is carried")
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
                var toggle = window.named(window.folderRow, "layoutSwitch")
                if ((toggle && toggle.checked) || window.page.previewCount !== window.rows - 1)
                    return
                window.afterSwitch()
                if (window.scenarioRunning)
                    window.carry()
                break
            case 2:
                // Stored and read again, the row stays where it was let go.
                if (window.waited < 10)
                    return
                window.expect(window.rowOf(layout, window.folder) === window.fromRow + 1,
                              "the moved row stays once stored: " + window.describe(layout))
                window.expect(window.delegateOf(window.folder) === window.folderRow, "the moved row is the same row")
                layout.restoreDefaults()
                break
            case 3:
                if (window.page.previewCount !== window.rows || window.firstDeviceRow(layout) !== "recents")
                    return
                window.listed = layout.count
                favourites.add("lautta://user-music/", "Tunes", "")
                break
            case 4:
                // Both follow the store; the list has no reload of its own here.
                if (layout.count !== window.listed + 1 || window.page.previewCount !== window.rows + 1)
                    return
                favourites.removeUri("lautta://user-music/")
                favourites.removeUri("lautta://user-documents/")
                break
            case 5:
                // Removed and gone from the list, which follows the store.
                if (favourites.idOf("lautta://user-documents/") !== -1 || favourites.idOf("lautta://user-music/") !== -1
                        || window.sectionRows(layout, "favourites") > 0)
                    return
                window.checkIndent()
                window.checkSectionEnds()
                window.listed = layout.count
                // The section is off: its switch shows although there is
                // no volume, and nothing else changes.
                layout.setHidden("volumes", "section", true)
                break
            case 6:
                var volumes = window.switchOf("volumes")
                if (layout.count !== window.listed + 1 || !volumes)
                    return
                window.checkSectionSwitch(volumes)
                // A tap on the row turns the section on, as on any row.
                volumes.clicked(null)
                break
            case 7:
                if (layout.count !== window.listed)
                    return
                window.expect(window.switchOf("volumes") === null, "a section that is on and empty has no switch")
                layout.setHidden("volumes", "section", true)
                break
            case 8:
                if (layout.count !== window.listed + 1)
                    return
                // Restore defaults turns the sections on.
                layout.restoreDefaults()
                break
            case 9:
                if (layout.count !== window.listed)
                    return
                window.expect(window.switchOf("volumes") === null, "Restore defaults leaves Volumes off")
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

    // The list of Servers: its switch, then two rows with places 0 and 1.
    QtObject {
        id: sectionStub

        readonly property int count: 3
        function sectionAt(row) { return "servers" }
        function placeAt(row) { return row - 1 }
    }

    Component {
        id: indentComponent
        BrowseLayoutDelegate {
            width: 540
            layoutSection: "servers"
            layoutItem: "nv-account:1"
            layoutKind: "account"
            layoutName: "NAS"
        }
    }
}
