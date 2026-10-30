// SPDX-License-Identifier: LGPL-2.1-or-later
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
        if (pred(item) && out.indexOf(item) < 0)
            out.push(item)
        var kids = item.children || []
        for (var i = 0; i < kids.length; ++i)
            findAll(kids[i], pred, out)
        if (item.contentItem && item.contentItem !== item)
            findAll(item.contentItem, pred, out)
        return out
    }

    function shownRows(layout) {
        var n = 0
        for (var i = 0; i < layout.count; ++i) {
            if (layout.sectionAt(i) !== "nearby" && layout.itemAt(i) !== "section")
                ++n
        }
        return n
    }

    function sectionRows(layout, section) {
        var n = 0
        for (var i = 0; i < layout.count; ++i) {
            if (layout.sectionAt(i) === section)
                ++n
        }
        return n
    }

    function rowOf(layout, item) {
        for (var i = 0; i < layout.count; ++i) {
            if (layout.sectionAt(i) === "device" && layout.itemAt(i) === item)
                return i
        }
        return -1
    }

    function firstDeviceRow(layout) {
        for (var i = 0; i < layout.count; ++i) {
            if (layout.sectionAt(i) === "device")
                return layout.itemAt(i)
        }
        return ""
    }

    function deviceFolder(layout) {
        for (var i = 0; i < layout.count; ++i) {
            var item = layout.itemAt(i)
            if (layout.sectionAt(i) === "device" && item !== "deleted" && item !== "recents")
                return item
        }
        return ""
    }

    function describe(layout) {
        var out = []
        for (var i = 0; i < layout.count; ++i)
            out.push(layout.sectionAt(i) + "/" + layout.itemAt(i))
        return out.join(", ")
    }

    // Headless: no render loop; list shorter than window, make all delegates.
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

    function marked(what) {
        return delegates().filter(function(d) { return d[what] }).map(function(d) { return d.layoutItem }).join(",")
    }

    function switchOf(section) {
        var all = delegates()
        for (var i = 0; i < all.length; ++i) {
            if (all[i].layoutSection === section && all[i].layoutKind === "section")
                return all[i]
        }
        return null
    }

    function checkSectionSwitch(row, on) {
        var toggle = named(row, "layoutSwitch")
        var grip = named(row, "layoutGrip")
        expect(toggle !== null && toggle.visible && toggle.checked === on, "the Volumes switch is " + (on ? "on" : "off"))
        expect(grip === null || !grip.visible, "the Volumes switch has no grip")
        expect(row.title === "lautta-browse-layout-volumes", "the Volumes switch is named by its own text: " + row.title)
        expect(!row.layoutNested && row.layoutMovable === false, "the switch is neither nested nor movable")
        var layout = page.layoutModel
        for (var i = 0; i < layout.count; ++i) {
            if (layout.sectionAt(i) === "volumes" && layout.itemAt(i) === "section")
                expect(layout.placeAt(i) === -1, "the switch takes no place in its section: " + layout.placeAt(i))
        }
    }

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
        page.listView.contentY = page.listView.contentY + 40
        scrolledTo = page.listView.contentY
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
                var toggle = window.named(window.folderRow, "layoutSwitch")
                if ((toggle && toggle.checked) || window.page.previewCount !== window.rows - 1)
                    return
                window.afterSwitch()
                if (window.scenarioRunning)
                    window.carry()
                break
            case 2:
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
                if (layout.count !== window.listed + 1 || window.page.previewCount !== window.rows + 1)
                    return
                favourites.removeUri("lautta://user-music/")
                favourites.removeUri("lautta://user-documents/")
                break
            case 5:
                if (favourites.idOf("lautta://user-documents/") !== -1 || favourites.idOf("lautta://user-music/") !== -1
                        || window.sectionRows(layout, "favourites") > 0)
                    return
                window.checkIndent()
                window.checkSectionEnds()
                window.listed = layout.count
                var volumesOn = window.switchOf("volumes")
                if (!window.expect(volumesOn !== null, "the Volumes switch is there without a volume: " + window.describe(layout)))
                    return
                window.checkSectionSwitch(volumesOn, true)
                window.expect(window.sectionRows(layout, "volumes") === 1, "an empty Volumes section is more than its switch: " + window.describe(layout))
                window.expect(window.switchOf("servers") === null && window.sectionRows(layout, "servers") === 0,
                              "standalone shows the Servers switch: " + window.describe(layout))
                layout.setHidden("volumes", "section", true)
                break
            case 6:
                var volumes = window.switchOf("volumes")
                if (!volumes || !volumes.layoutHidden)
                    return
                window.expect(layout.count === window.listed, "switching Volumes off changed the rows: " + window.describe(layout))
                window.checkSectionSwitch(volumes, false)
                volumes.clicked(null)
                break
            case 7:
                var volumesBack = window.switchOf("volumes")
                if (!volumesBack || volumesBack.layoutHidden)
                    return
                window.expect(layout.count === window.listed, "a section that is on and empty is more than its switch: " + window.describe(layout))
                layout.setHidden("volumes", "section", true)
                break
            case 8:
                if (!window.switchOf("volumes") || !window.switchOf("volumes").layoutHidden)
                    return
                layout.restoreDefaults()
                break
            case 9:
                if (!window.switchOf("volumes") || window.switchOf("volumes").layoutHidden)
                    return
                window.expect(layout.count === window.listed, "Restore defaults changed the rows: " + window.describe(layout))
                window.scenarioRunning = false
                break
            }
            window.step++
            window.waited = 0
        }
    }

    Component.onCompleted: {
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
