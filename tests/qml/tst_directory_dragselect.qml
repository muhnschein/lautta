// SPDX-License-Identifier: LGPL-2.1-or-later
import QtQuick 2.6
import Sailfish.Silica 1.0
import Lautta 1.0
import "../../qml/components/directory"

ApplicationWindow {
    id: win

    readonly property string base: "lautta://user-documents/DragSelect/"
    readonly property int files: 200
    property var view: pageStack.currentPage ? pageStack.currentPage.view : null

    ViewerTools { id: tools }

    DirectoryModel {
        id: documents
        uri: "lautta://user-documents/"
    }

    function expect(ok, what) {
        if (!ok)
            console.error("FAILED: " + what)
        return ok
    }

    function nameOf(i) {
        return "f" + (i < 10 ? "00" : (i < 100 ? "0" : "")) + i + ".md"
    }

    function makeFolder() {
        documents.createFolder("DragSelect")
        tools.pump(500)
        for (var i = 0; i < files; ++i)
            tools.installFixture(Qt.resolvedUrl("host/fixtures/doc.md"), base + nameOf(i))
    }

    function child(pred) {
        for (var i = 0; i < view.children.length; ++i) {
            if (pred(view.children[i]))
                return view.children[i]
        }
        return null
    }

    function flick() {
        return view.flickable
    }

    function panelHeight() {
        var panel = child(function (c) { return c.dock !== undefined && c.open })
        return panel ? panel.height : 0
    }

    function delegates(f) {
        var out = []
        var kids = f.contentItem.children
        for (var i = 0; i < kids.length; ++i) {
            var cells = kids[i].regroup !== undefined ? kids[i].children : [kids[i]]
            for (var j = 0; j < cells.length; ++j)
                out.push(cells[j])
        }
        return out
    }

    function cell(index) {
        var f = flick()
        f.forceLayout()
        var kids = delegates(f)
        for (var i = 0; i < kids.length; ++i) {
            var k = kids[i]
            var p = k.mapToItem(f.contentItem, k.width / 2, k.height / 2)
            if (k.width > 0 && k.visible && f.itemIndexAt(p.x, p.y) === index
                    && (k.itemName === undefined || k.itemName === nameOf(index)))
                return k
        }
        return null
    }

    function bottomOf(f, item) {
        return item.mapToItem(f.contentItem, 0, item.height).y
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

    function same(a, b, what) {
        return expect(JSON.stringify(a) === JSON.stringify(b), what + ": " + JSON.stringify(a) + " != " + JSON.stringify(b))
    }

    function press(index) {
        var c = cell(index)
        tools.mouse(c, "press", c.width / 2, c.height / 2)
    }

    function moveTo(index) {
        var c = cell(index)
        if (!expect(c !== null, "item " + index + " is on screen"))
            return
        tools.mouse(c, "move", c.width / 2 - 4, c.height / 2 - 4)
        tools.mouse(c, "move", c.width / 2, c.height / 2)
    }

    function release() {
        tools.mouse(flick(), "release", 0, 0)
        tools.pump(50)
    }

    function checkHoldAndDrag() {
        var grid = flick()
        expect(view.gridMode && grid.columns === 3, "a grid of 3 per row")
        expect(!view.selecting, "not selecting at first")
        press(2)
        tools.pump(1200)
        expect(view.selecting, "holding an item starts selection")
        same(selected(), [2], "holding an item selects it")
        moveTo(3)
        same(selected(), [2, 3], "moving on selects the next item")
        moveTo(5)
        same(selected(), range(2, 5), "the range follows the finger in reading order, into the next row")
        moveTo(3)
        same(selected(), [2, 3], "moving back shrinks the range")
        moveTo(0)
        same(selected(), [0, 1, 2], "passing the held item turns the range round")
        release()
        same(selected(), [0, 1, 2], "letting go keeps the range")
        expect(view.selecting, "still selecting")
        press(9)
        tools.pump(1200)
        moveTo(10)
        release()
        same(selected(), [0, 1, 2, 9, 10], "a second range adds to the selection")
        press(1)
        tools.pump(1200)
        moveTo(0)
        release()
        same(selected(), [2, 9, 10], "a drag started on a selected item deselects")
        view.endSelection()
    }

    function checkEdgeScroll() {
        var grid = flick()
        grid.contentY = grid.originY
        var held = 4
        press(held)
        tools.pump(1200)
        var anchor = cell(held)
        var y = grid.height - panelHeight() - 4
        tools.mouse(anchor, "move", anchor.width / 2, anchor.height / 2 + 10)
        tools.pump(30)
        var p = anchor.mapFromItem(grid, grid.width / 2, y)
        tools.mouse(anchor, "move", p.x, p.y)
        var top = grid.contentY
        var gone = function () { return bottomOf(grid, anchor) < grid.contentY - grid.cacheBuffer }
        for (var i = 0; i < 200 && !gone(); ++i)
            tools.pump(50)
        expect(grid.contentY > top + anchor.height * 2, "holding near the bottom scrolls: " + top + " -> " + grid.contentY)
        var picked = selected()
        expect(picked.length > 8 && picked[0] === held && picked[picked.length - 1] === picked.length + held - 1,
               "the range follows the scrolling: " + JSON.stringify(picked))
        expect(gone(), "scrolled past the cache: " + bottomOf(grid, anchor) + " " + grid.contentY + " " + grid.cacheBuffer)
        var middle = grid.itemIndexAt(grid.width / 2, grid.contentY + grid.height / 3)
        p = anchor.mapFromItem(grid, grid.width / 2, grid.height / 3)
        tools.mouse(anchor, "move", p.x, p.y)
        tools.pump(100)
        same(selected(), range(held, middle), "the held item still follows the finger")
        release()
        view.endSelection()
        grid.contentY = grid.originY
    }

    function checkReveal() {
        var grid = flick()
        grid.contentY = grid.originY
        var index = 150
        view.renamed = { "uri": view.model.uriAt(index), "oldUri": "", "isDir": false, "ends": 0, "revealed": false }
        view.revealRenamed()
        var c = cell(index)
        var top = c ? c.mapToItem(grid, 0, 0).y : -1
        expect(c !== null && top >= 0 && top + c.height <= grid.height, "the renamed item is in view: " + top)
        view.renamed = null
        grid.contentY = grid.originY
    }

    function checkList() {
        App.setSetting("view_mode", JSON.stringify("list"))
        tools.pump(300)
        expect(!view.gridMode, "list view")
        press(1)
        tools.pump(1200)
        release()
        expect(!view.selecting && selected().length === 0, "outside selection holding a row selects nothing")
        expect(cell(1).menuOpen, "it opens the row's menu")
        cell(1).closeMenu()
        tools.pump(500)
        view.startSelection()
        press(1)
        tools.pump(1200)
        same(selected(), [1], "while selecting, holding a row selects it")
        moveTo(3)
        same(selected(), [1, 2, 3], "and a drag selects the rows on")
        release()
        view.endSelection()
    }

    Component.onCompleted: {
        makeFolder()
        App.setSetting("view_mode", JSON.stringify("grid"))
        pageStack.push(pageComponent, {}, PageStackAction.Immediate)
        tools.pump(800)
        if (expect(view.model.count === files, "the folder lists its files: " + view.model.count)) {
            checkHoldAndDrag()
            checkEdgeScroll()
            checkReveal()
            checkList()
        }
        App.setSetting("view_mode", JSON.stringify("list"))
    }

    Component {
        id: pageComponent

        Page {
            property alias view: folder

            DirectoryView {
                id: folder
                width: parent.width
                height: 1200
                uri: win.base
            }
        }
    }
}
