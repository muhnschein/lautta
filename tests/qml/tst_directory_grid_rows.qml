// SPDX-License-Identifier: LGPL-2.1-or-later
import QtQuick 2.6
import Sailfish.Silica 1.0
import Lautta 1.0
import "../../qml/components/directory"

ApplicationWindow {
    id: win

    readonly property string base: "lautta://user-documents/GridRows/"
    readonly property string longName: "abcdefghijklmnopqrstuvwxyz-abcdefghijklmnopqrstuvwxyz.txt"
    readonly property var names: ["A", "B", "C", longName, "e", "f", "g", "h", "i"]
    property var view: pageStack.currentPage ? pageStack.currentPage.view : null

    ViewerTools { id: tools }
    GridMetrics { id: metrics }
    DirectoryModel { id: documents; uri: "lautta://user-documents/" }
    DirectoryModel { id: folder }

    function expect(ok, what) {
        if (!ok)
            console.error("FAILED: " + what)
        return ok
    }

    function same(a, b, what) {
        return expect(JSON.stringify(a) === JSON.stringify(b), what + ": " + JSON.stringify(a) + " != " + JSON.stringify(b))
    }

    function waitFor(done, what) {
        for (var i = 0; i < 200 && !done(); ++i)
            tools.pump(50)
        return expect(done(), "timed out waiting for " + what)
    }

    function find(item, pred) {
        if (!item)
            return null
        if (pred(item))
            return item
        var kids = item.children || []
        for (var i = 0; i < kids.length; ++i) {
            var found = find(kids[i], pred)
            if (found)
                return found
        }
        return null
    }

    function named(item, name) {
        return find(item, function (x) { return x.objectName === name })
    }

    function makeFolder() {
        documents.createFolder("GridRows")
        waitFor(function () { return documents.indexOf(base.replace(/\/$/, "")) >= 0 }, "the folder")
        folder.uri = base
        for (var i = 0; i < 3; ++i)
            folder.createFolder(names[i])
        waitFor(function () { return folder.count === 3 }, "its folders")
        for (var j = 3; j < names.length; ++j)
            tools.installFixture(Qt.resolvedUrl("host/fixtures/doc.md"), base + names[j])
        folder.refresh()
        waitFor(function () { return folder.count === names.length }, "its files")
    }

    // No render loop headless: lay out by hand.
    function rowsOf(flick) {
        flick.forceLayout()
        var out = []
        var kids = flick.contentItem.children
        for (var i = 0; i < kids.length; ++i) {
            if (kids[i].regroup !== undefined)
                out.push(kids[i])
        }
        return out.sort(function (a, b) { return a.first - b.first })
    }

    function cellsOf(row) {
        var out = []
        for (var i = 0; i < row.children.length; ++i) {
            if (row.children[i].captions !== undefined)
                out.push(row.children[i])
        }
        return out.sort(function (a, b) { return a.x - b.x })
    }

    function namesIn(rows) {
        return rows.map(function (row) {
            return cellsOf(row).map(function (c) { return c.itemName !== undefined ? c.itemName : c.name })
        })
    }

    function cellHeight(width, nameLines, captionLines) {
        return metrics.cellHeight(width, nameLines * metrics.nameLineHeight + captionLines * metrics.captionHeight)
    }

    function checkTight(cell, name) {
        var label = named(cell, "gridName")
        var lines = metrics.nameLines(name, cell.width - 2 * cell.gap)
        expect(Math.abs(label.height - lines.length * metrics.nameLineHeight) < 0.5
               && label.contentHeight <= label.height + 0.5,
               name + ": the name is as high as its " + lines.length + " lines: " + label.height)
        var bottom = label.mapToItem(cell, 0, label.height).y
        for (var i = 0; i < cell.captions.length; ++i) {
            var line = named(cell, "gridCaption" + i)
            var top = line ? line.mapToItem(cell, 0, 0).y : -1
            expect(Math.abs(top - bottom) < 0.5, name + ": caption line " + i + " right under the line before: "
                   + top + " vs " + bottom)
            if (line)
                bottom = top + line.height
        }
        expect(named(cell, "gridCaption" + cell.captions.length) === null, name + ": no empty line under its captions")
        expect(Math.abs(cell.implicitHeight - (bottom + cell.gap + Theme.paddingSmall / 2)) < 0.5,
               name + ": the cell ends a gap and half a padding under its last line: " + cell.implicitHeight + " vs " + bottom)
    }

    function checkRows(rows, label) {
        for (var r = 0; r < rows.length; ++r) {
            var cells = cellsOf(rows[r])
            var tallest = 0
            for (var i = 0; i < cells.length; ++i) {
                tallest = Math.max(tallest, cells[i].implicitHeight)
                expect(cells[i].height === rows[r].height, label + ": a cell is as tall as its row " + r)
                checkTight(cells[i], cells[i].itemName !== undefined ? cells[i].itemName : cells[i].name)
            }
            expect(rows[r].height === tallest, label + ": row " + r + " is as tall as its tallest cell: "
                   + rows[r].height + " vs " + tallest)
        }
    }

    function counted(rows) {
        return cellsOf(rows[0]).every(function (c) { return c.captions.length === 1 })
    }

    function checkPortrait(grid) {
        var tile = grid.cellWidth - 2 * metrics.gap
        expect(metrics.nameLines("A", tile).length === 1 && metrics.nameLines("e", tile).length === 1
               && metrics.nameLines(longName, tile).length === 2, "short names take one line, the long one two")
        var rows = rowsOf(grid)
        same(namesIn(rows), [names.slice(0, 3), names.slice(3, 6), names.slice(6, 9)], "3 per row in reading order")
        if (rows.length !== 3)
            return
        var folders = cellsOf(rows[0])
        expect(folders.every(function (c) { return c.counting && find(c, function (x) { return x.minLines === 1 }) }),
               "a folder's count has its line while it comes")
        waitFor(function () { return counted(rows) }, "the counts")
        same([rows[0].height, rows[1].height, rows[2].height],
             [cellHeight(grid.cellWidth, 1, 1), cellHeight(grid.cellWidth, 2, 2), cellHeight(grid.cellWidth, 1, 2)],
             "each row only as tall as what its tallest tile shows")
        expect(rows[0].height < rows[1].height, "a row of one-line names with a count is shorter than one with a two-line name")
        checkRows(rows, "portrait")
    }

    function checkWidths(view) {
        var grid = view.flickable
        var narrow = view.width
        view.width = 5 * grid.tileWidth
        tools.pump(100)
        expect(grid.columns === 5, "a wider view has 5 per row: " + grid.columns)
        var rows = rowsOf(grid)
        same(namesIn(rows), [names.slice(0, 5), names.slice(5, 9)], "the items regrouped")
        checkRows(rows, "wide")
        view.width = narrow
        tools.pump(100)
        expect(grid.columns === 3, "3 per row again")
        same(namesIn(rowsOf(grid)), [names.slice(0, 3), names.slice(3, 6), names.slice(6, 9)], "and the rows of 3 again")
    }

    function cellNamed(grid, name) {
        var rows = rowsOf(grid)
        for (var r = 0; r < rows.length; ++r) {
            var cells = cellsOf(rows[r])
            for (var i = 0; i < cells.length; ++i) {
                if (cells[i].itemName === name)
                    return cells[i]
            }
        }
        expect(false, "no cell called " + name)
        return null
    }

    function moveTo(grid, index) {
        var c = cellNamed(grid, names[index])
        if (!c)
            return
        tools.mouse(c, "move", c.width / 2 - 4, c.height / 2 - 4)
        tools.mouse(c, "move", c.width / 2, c.height / 2)
    }

    function selected() {
        return view.model.selectedUris().map(function (u) { return view.model.indexOf(u) })
    }

    function range(first, last) {
        var out = []
        for (var i = first; i <= last; ++i)
            out.push(i)
        return out
    }

    function checkDrag(grid) {
        var held = cellNamed(grid, "B")
        if (!held)
            return
        tools.mouse(held, "press", held.width / 2, held.height / 2)
        tools.pump(1200)
        same(selected(), [1], "holding selects")
        moveTo(grid, 4)
        same(selected(), range(1, 4), "past the end of the row into the taller one")
        moveTo(grid, 7)
        same(selected(), range(1, 7), "on into the next row")
        moveTo(grid, 5)
        same(selected(), range(1, 5), "back up the rows")
        moveTo(grid, 0)
        same(selected(), [0, 1], "and back to the start")
        tools.mouse(grid, "release", 0, 0)
        tools.pump(50)
        same(selected(), [0, 1], "letting go keeps the range")
        var p = grid.contentItem.mapFromItem(cellNamed(grid, "e"), 0, 0)
        same(grid.itemIndexAt(p.x + 1, p.y + 1), 4, "the item at a point of a taller row")
        p = grid.contentItem.mapFromItem(cellNamed(grid, "i"), 0, 0)
        same(grid.itemIndexAt(p.x + grid.cellWidth + 1, p.y + 1), -1, "nothing past the last item")
        view.endSelection()
    }

    function checkPreview(width) {
        var preview = previewComponent.createObject(win.contentItem, { "screenWidth": width, "screenHeight": 960 })
        tools.pump(300)
        var rows = findRows(preview)
        expect(preview.gridMode && rows.length === 2, "the preview has two rows of 3: " + rows.length)
        same(namesIn(rows).map(function (r) { return r.length }), [3, 3], "3 tiles a row")
        var tiles = preview.tiles()
        for (var i = 0; i < tiles.length; ++i) {
            var t = tiles[i]
            var lines = metrics.nameLines(t.name, t.width - 2 * t.gap).length
            same(t.implicitHeight, cellHeight(preview.cellWidth, lines, t.isDir ? 1 : 2),
                 t.name + ": its name and caption lines only")
        }
        checkRows(rows, "preview")
        preview.destroy()
    }

    function checkInsert(view) {
        var grid = view.flickable
        var tall = view.height
        view.height = 300
        tools.pump(100)
        grid.contentY = grid.originY + 120
        tools.pump(100)
        var top = grid.contentY
        tools.installFixture(Qt.resolvedUrl("host/fixtures/doc.md"), base + "d")
        view.model.refresh()
        waitFor(function () { return view.model.count === names.length + 1 }, "the new item")
        tools.pump(100)
        var rows = rowsOf(grid)
        same(namesIn(rows), [names.slice(0, 3), [longName, "d", "e"], ["f", "g", "h"], ["i"]], "the rows after it regrouped")
        checkRows(rows, "with the new item")
        expect(Math.abs(grid.contentY - top) < 0.5, "the view keeps its position: " + grid.contentY + " vs " + top)
        view.height = tall
    }

    function findRows(item) {
        var out = []
        var walk = function (x) {
            if (x.tiles !== undefined && x.first !== undefined)
                out.push(x)
            var kids = x.children || []
            for (var i = 0; i < kids.length; ++i)
                walk(kids[i])
        }
        walk(item)
        return out
    }

    Component.onCompleted: {
        App.setSetting("view_mode", JSON.stringify("grid"))
        App.setSetting("show_hidden", JSON.stringify(false))
        makeFolder()
        pageStack.push(pageComponent, {}, PageStackAction.Immediate)
        waitFor(function () { return view && !view.model.loading && view.model.count === names.length }, "the grid")
        if (expect(view.gridMode && view.flickable.columns === 3, "a grid of 3 per row")) {
            checkPortrait(view.flickable)
            checkWidths(view)
            checkDrag(view.flickable)
            checkPreview(view.width)
            checkInsert(view)
        }
        App.setSetting("view_mode", JSON.stringify("list"))
    }

    Component {
        id: pageComponent

        Page {
            property alias view: grid

            DirectoryView {
                id: grid

                width: parent.width
                height: 1200
                uri: win.base
            }
        }
    }

    Component {
        id: previewComponent

        ViewPreview { }
    }
}
