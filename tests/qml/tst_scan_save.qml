// SPDX-License-Identifier: LGPL-2.1-or-later Save dialog of a scan: name, pages in a grid, held
// page dragged to a new place, saved as PDF into the folder; a failed save keeps the pages.
import QtQuick 2.6
import Sailfish.Silica 1.0
import Lautta 1.0

ApplicationWindow {
    id: win

    readonly property string folder: "lautta://user-documents/"
    readonly property string sheet: Qt.resolvedUrl("host/fixtures/sheet.jpg")
    readonly property string plain: Qt.resolvedUrl("host/fixtures/photo.jpg")
    property bool scenarioRunning: true
    property bool reachedEnd
    property int step: 0
    property int waited: 0
    property var dialog
    property var tile
    property var ended: []
    // Null once the scan is destroyed.
    property QtObject scan

    ViewerTools { id: tools }
    PdfDocument { id: doc }

    DirectoryModel {
        id: docs
        uri: win.folder
    }

    Component {
        id: maker

        ScanModel { }
    }

    Item {
        id: owner

        function endScan(scan, saved) {
            win.ended = win.ended.concat([saved])
            scan.destroy()
        }
    }

    initialPage: Component {
        Page { }
    }

    function fail(what) {
        console.error("FAILED: " + what)
        scenarioRunning = false
    }

    function findAll(item, pred, out, seen) {
        seen = seen || []
        if (!item || seen.indexOf(item) >= 0)
            return out
        seen.push(item)
        if (pred(item))
            out.push(item)
        var kids = item.children || []
        for (var i = 0; i < kids.length; ++i)
            findAll(kids[i], pred, out, seen)
        if (item.contentItem)
            findAll(item.contentItem, pred, out, seen)
        return out
    }

    function named(root, name) {
        var found = findAll(root, function(x) { return x.objectName === name }, [])
        return found.length === 1 ? found[0] : null
    }

    function onTop(property) {
        var page = pageStack.currentPage
        return page && page[property] !== undefined && page.status === PageStatus.Active && !pageStack.busy ? page : null
    }

    function text(got, english, id) {
        return got === english || got === id
    }

    // Tiles in the order the grid shows them.
    function tiles() {
        var found = findAll(win.dialog, function(x) { return x.objectName === "pageTile" }, [])
        found.sort(function(a, b) { return a.y !== b.y ? a.y - b.y : a.x - b.x })
        return found
    }

    function ids() {
        var out = []
        for (var i = 0; i < scan.count; ++i)
            out.push(JSON.parse(scan.pageJson(i)).id)
        return JSON.stringify(out)
    }

    function numbers() {
        var out = []
        var all = tiles()
        for (var i = 0; i < all.length; ++i)
            out.push(named(all[i], "pageNumber").text)
        return JSON.stringify(out)
    }

    // Middle of the grid cell `index`, seen from `item`.
    function cell(item, index) {
        var grid = named(win.dialog, "pageGrid")
        return item.mapFromItem(grid, (index % 2 + 0.5) * grid.cellWidth, (Math.floor(index / 2) + 0.5) * grid.cellHeight)
    }

    function open(folder, name) {
        win.dialog = pageStack.push(Qt.resolvedUrl("../../qml/dialogs/ScanSaveDialog.qml"),
                                    { "parentUri": folder, "owner": owner, "scan": win.scan, "name": name })
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
            win.waited++
            // Hang guard: emulated target needs seconds per picture.
            if (win.waited > 600)
                return win.fail("step " + win.step + " timed out")
            var all, at, field, grid, header, content
            switch (win.step) {
            case 0:
                win.scan = maker.createObject(win)
                win.scan.add(win.sheet)
                win.scan.add(win.plain)
                win.scan.add(win.sheet)
                win.next()
                break
            case 1:
                if (win.scan.busy || docs.loading)
                    break
                win.open(win.folder, "")
                win.next()
                break
            case 2:
                if (!win.onTop("addPage") || tiles().length !== 3)
                    break
                header = findAll(win.dialog, function(x) { return x.defaultAcceptText !== undefined }, [])[0]
                if (!text(header.acceptText, "Save", "lautta-scan-save") || !text(header.title, "Save 3 pages as PDF", "lautta-scan-save-title"))
                    return win.fail("header says what is saved: " + header.acceptText + ", " + header.title)
                field = named(win.dialog, "nameField")
                if (!/^Scan \d{4}-\d{2}-\d{2}$/.test(field.text) && field.text !== "lautta-scan-default-name")
                    return win.fail("name starts as Scan and the date: " + field.text)
                if (!win.dialog.canAccept)
                    return win.fail("can be saved as it opens")
                if (numbers() !== '["1","2","3"]' || ids() !== "[1,2,3]")
                    return win.fail("pages numbered in order: " + numbers())
                grid = named(win.dialog, "pageGrid")
                all = tiles()
                if (grid.columns !== 2 || all[1].y !== all[0].y || all[1].x <= all[0].x || all[2].y <= all[0].y || all[2].x !== all[0].x)
                    return win.fail("two pages a row: " + all.map(function(t) { return t.x + "," + t.y + " " + t.width + "x" + t.height }).join("; ") + " grid " + grid.width + "x" + grid.height)
                var plus = named(win.dialog, "addPageTile")
                if (plus.y !== all[2].y || plus.x !== all[1].x)
                    return win.fail("plus tile takes the place after the last page")
                field.text = "a/b"
                if (win.dialog.canAccept || !field.errorHighlight)
                    return win.fail("a name with a slash is refused")
                field.text = ""
                if (win.dialog.canAccept)
                    return win.fail("no name: nothing to save as")
                field.text = "Contract"
                if (!win.dialog.canAccept || field.errorHighlight)
                    return win.fail("a plain name is taken")
                // A tap opens the page.
                tools.mouse(all[1], "press", all[1].width / 2, all[1].height / 2)
                tools.mouse(all[1], "release", all[1].width / 2, all[1].height / 2)
                win.next()
                break
            case 3:
                var pageView = win.onTop("colourText")
                if (!pageView)
                    break
                if (pageView.scan !== win.scan || pageView.index !== 1)
                    return win.fail("tap shows that page: " + pageView.index)
                pageStack.pop()
                win.next()
                break
            case 4:
                if (!win.onTop("addPage"))
                    break
                win.tile = tiles()[0]
                tools.mouse(win.tile, "press", win.tile.width / 2 + 4, win.tile.height / 2 + 6)
                win.next()
                break
            case 5:
                // Held for a second without moving: the page comes loose.
                if (!win.tile.held)
                    break
                if (win.waited < 9)
                    return win.fail("page came loose before a second was up: " + win.waited)
                grid = named(win.dialog, "pageGrid")
                content = win.tile.contentItem
                if (content.x !== 0 || content.y !== 0 || content.scale !== 1.05 || win.tile.z !== 1)
                    return win.fail("loose page lifts where it is")
                at = cell(win.tile, 2)
                tools.mouse(win.tile, "move", at.x, at.y)
                if (ids() !== "[2,3,1]")
                    return win.fail("page dropped into the third place, the others close up: " + ids())
                win.next()
                break
            case 6:
                // Cells settle after the move.
                grid = named(win.dialog, "pageGrid")
                if (win.tile.x !== 0 || win.tile.y !== grid.cellHeight)
                    break
                content = win.tile.contentItem
                // Finger still on the spot of the page it took.
                if (Math.abs(win.tile.x + content.x + win.tile.width / 2 + 4 - 0.5 * grid.cellWidth) > 1
                        || Math.abs(win.tile.y + content.y + win.tile.height / 2 + 6 - 1.5 * grid.cellHeight) > 1)
                    return win.fail("page stays under the finger: " + content.x + ", " + content.y)
                if (numbers() !== '["1","2","3"]')
                    return win.fail("numbers follow the places: " + numbers())
                // Over the plus tile: no place for a page.
                at = cell(win.tile, 3)
                tools.mouse(win.tile, "move", at.x, at.y)
                if (ids() !== "[2,3,1]")
                    return win.fail("plus tile is no place: " + ids())
                at = cell(win.tile, 1)
                tools.mouse(win.tile, "move", at.x, at.y)
                if (ids() !== "[2,1,3]")
                    return win.fail("on to the second place: " + ids())
                win.next()
                break
            case 7:
                grid = named(win.dialog, "pageGrid")
                if (win.tile.x !== grid.cellWidth || win.tile.y !== 0)
                    break
                // Let go inside its own cell, where a tap opens the page.
                tools.mouse(win.tile, "move", win.tile.width / 2, win.tile.height / 2)
                tools.mouse(win.tile, "release", win.tile.width / 2, win.tile.height / 2)
                if (win.tile.held || win.tile.contentItem.x !== 0 || win.tile.contentItem.y !== 0 || win.tile.contentItem.scale !== 1
                        || win.tile.z !== 0)
                    return win.fail("let go: page sits in its cell")
                win.next()
                break
            case 8:
                if (win.scan.busy || pageStack.busy)
                    break
                if (pageStack.currentPage !== win.dialog)
                    return win.fail("letting a dragged page go opens nothing")
                if (ids() !== "[2,1,3]")
                    return win.fail("order kept: " + ids())
                win.tile = tiles()[2]
                tools.mouse(win.tile, "press", win.tile.width / 2, win.tile.height / 2)
                tools.mouse(win.tile, "move", win.tile.width / 2 + Theme.iconSizeSmall + 2, win.tile.height / 2)
                win.next()
                break
            case 9:
                // Nothing may happen: finger moved before the second was up. Half a second longer.
                if (win.waited < 15)
                    break
                if (win.tile.held)
                    return win.fail("a moving finger takes no page")
                at = cell(win.tile, 0)
                tools.mouse(win.tile, "move", at.x, at.y)
                tools.mouse(win.tile, "release", at.x, at.y)
                if (ids() !== "[2,1,3]")
                    return win.fail("no page taken, none moved: " + ids())
                var plusTile = named(win.dialog, "addPageTile")
                tools.mouse(plusTile, "press", plusTile.width / 2, plusTile.height / 2)
                tools.mouse(plusTile, "release", plusTile.width / 2, plusTile.height / 2)
                win.next()
                break
            case 10:
                var camera = win.onTop("photoTaken")
                if (!camera)
                    break
                if (win.ended.length !== 0 || camera.scan !== win.scan || camera.name !== "Contract")
                    return win.fail("plus tile: camera for the same scan, name kept: " + camera.name)
                camera.finish()
                win.next()
                break
            case 11:
                win.dialog = win.onTop("addPage")
                if (!win.dialog || tiles().length !== 3)
                    break
                if (named(win.dialog, "nameField").text !== "Contract" || win.ended.length !== 0)
                    return win.fail("back with the name typed before: " + named(win.dialog, "nameField").text)
                win.next()
                break
            case 12:
                // Silica has the saving page ready before it lets the dialog go on.
                if (!win.dialog.acceptDestinationInstance || !win.dialog.canAccept)
                    break
                win.dialog.accept()
                win.next()
                break
            case 13:
                if (win.waited % 5 === 0)
                    docs.refresh()
                if (pageStack.busy || pageStack.depth !== 1 || win.scan !== null || docs.nameKind("Contract.pdf") !== "file")
                    break
                if (JSON.stringify(win.ended) !== "[true]")
                    return win.fail("scan ended as saved, once: " + JSON.stringify(win.ended))
                doc.uri = win.folder + "Contract.pdf"
                win.next()
                break
            case 14:
                if (doc.loading)
                    break
                if (!doc.loaded || doc.pageCount !== 3)
                    return win.fail("PDF holds the three pages: " + doc.pageCount)
                win.scan = maker.createObject(win)
                win.open(win.folder + "Gone/", "Lost")
                win.next()
                break
            case 15:
                if (!win.onTop("addPage"))
                    break
                if (win.dialog.canAccept || tiles().length !== 0)
                    return win.fail("no pages: nothing to save")
                win.scan.add(win.plain)
                win.next()
                break
            case 16:
                if (win.scan.busy)
                    break
                if (!win.dialog.canAccept || tiles().length !== 1)
                    return win.fail("page in: can be saved")
                win.scan.turn(0, 1)
                if (win.dialog.canAccept)
                    return win.fail("not while a page is worked on")
                win.next()
                break
            case 17:
                if (win.scan.busy || !win.dialog.acceptDestinationInstance || !win.dialog.canAccept)
                    break
                win.dialog.accept()
                win.next()
                break
            case 18:
                // Folder is not there: save fails, dialog comes back.
                var back = win.onTop("addPage")
                if (!back || back === win.dialog)
                    break
                win.dialog = back
                if (win.dialog.failure !== "NotFound")
                    return win.fail("says why it failed: " + win.dialog.failure)
                if (named(win.dialog, "nameField").text !== "Lost" || win.scan === null || win.scan.count !== 1 || win.scan.saving)
                    return win.fail("name and pages kept for another try")
                if (JSON.stringify(win.ended) !== "[true]" || pageStack.depth !== 2)
                    return win.fail("scan goes on: " + JSON.stringify(win.ended))
                pageStack.pop()
                win.next()
                break
            case 19:
                if (pageStack.busy || pageStack.depth !== 1 || win.scan !== null)
                    break
                if (JSON.stringify(win.ended) !== "[true,false]")
                    return win.fail("Cancel drops the scan, once: " + JSON.stringify(win.ended))
                win.reachedEnd = true
                win.scenarioRunning = false
                break
            }
        }
    }
}
