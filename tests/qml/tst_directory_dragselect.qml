// SPDX-License-Identifier: LGPL-2.1-or-later
// Drag selection on the target (real Silica, SPEC §15.3), with a finger
// sent through the window (ViewerTools.mouse). Grid view: holding an item
// starts selection with that item selected; moving on over other items
// selects the range from it in reading order, moving back shrinks it, and
// letting go keeps it. Holding near the bottom edge scrolls the grid on and
// the range follows; the held item keeps the finger after its row has
// scrolled far out of view. List view keeps its long-press menu outside
// selection and drags while selecting. Fails when holding does not select,
// a move does not extend or shrink the range, the edge does not scroll or
// the finger is lost.
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

    // The first child of the view that `pred` takes.
    function child(pred) {
        for (var i = 0; i < view.children.length; ++i) {
            if (pred(view.children[i]))
                return view.children[i]
        }
        return null
    }

    // The grid or the list, whichever is shown.
    function flick() {
        return child(function (c) { return c.indexAt !== undefined && c.visible })
    }

    function panelHeight() {
        var panel = child(function (c) { return c.dock !== undefined && c.open })
        return panel ? panel.height : 0
    }

    // The delegate of item `index`, laid out by hand: headless, no render
    // loop polishes the view.
    function cell(index) {
        var f = flick()
        f.forceLayout()
        var kids = f.contentItem.children
        for (var i = 0; i < kids.length; ++i) {
            var k = kids[i]
            if (k.width > 0 && k.visible && f.indexAt(k.x + k.width / 2, k.y + k.height / 2) === index
                    && f.itemAt(k.x + k.width / 2, k.y + k.height / 2) === k)
                return k
        }
        return null
    }

    // The indices selected, in order.
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

    // Moves the finger in two steps to the middle of item `index`. No event
    // loop runs in between: the checks' small screen is edge zones nearly
    // everywhere, and the view must not scroll on by itself here.
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
        // A second drag adds a range; starting on a selected item removes one.
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
        press(1)
        tools.pump(1200)
        var anchor = cell(1)
        // To the bottom of the items, above the panel: the grid scrolls on.
        var y = grid.height - panelHeight() - 4
        tools.mouse(anchor, "move", anchor.width / 2, anchor.height / 2 + 10)
        tools.pump(30)
        var p = anchor.mapFromItem(grid, grid.width / 2, y)
        tools.mouse(anchor, "move", p.x, p.y)
        var top = grid.contentY
        tools.pump(1500)
        expect(grid.contentY > top + grid.cellHeight * 2, "holding near the bottom scrolls: " + top + " -> " + grid.contentY)
        var picked = selected()
        expect(picked.length > 8 && picked[0] === 1 && picked[picked.length - 1] === picked.length,
               "the range follows the scrolling: " + JSON.stringify(picked))
        // The held item's row is far out of view now, past what the view keeps
        // ready (cacheBuffer); it still has the finger.
        expect(anchor.y + anchor.height < grid.contentY - grid.cacheBuffer,
               "scrolled past the cache: " + anchor.y + " " + grid.contentY + " " + grid.cacheBuffer)
        var middle = grid.indexAt(grid.width / 2, grid.contentY + grid.height / 3)
        p = anchor.mapFromItem(grid, grid.width / 2, grid.height / 3)
        tools.mouse(anchor, "move", p.x, p.y)
        tools.pump(100)
        same(selected(), range(1, middle), "the held item still follows the finger")
        release()
        view.endSelection()
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
            checkList()
        }
        App.setSetting("view_mode", JSON.stringify("list"))
    }

    Component {
        id: pageComponent

        Page {
            property alias view: folder

            // Taller than the page: the checks' small screen leaves no room for the
            // rows of tiles with captions between the header and the selection panel.
            DirectoryView {
                id: folder
                width: parent.width
                height: 1200
                uri: win.base
            }
        }
    }
}
