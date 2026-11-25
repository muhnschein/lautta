// SPDX-License-Identifier: LGPL-2.1-or-later
import QtQuick 2.6
import Sailfish.Silica 1.0
import Lautta 1.0
import "../../qml/viewers"

ApplicationWindow {
    id: window
    property bool reachedEnd

    ViewerTools { id: tools }

    function expect(ok, what) {
        if (!ok)
            console.error("FAILED: " + what)
        return ok
    }

    function waitFor(done, what) {
        for (var i = 0; i < 300 && !done(); ++i)
            tools.pump(50)
        return expect(done(), "timed out waiting for " + what)
    }

    function findAll(item, pred, out) {
        if (!item)
            return out
        if (pred(item) && out.indexOf(item) < 0)
            out.push(item)
        var kids = item.children || []
        for (var i = 0; i < kids.length; ++i)
            findAll(kids[i], pred, out)
        return out
    }

    function named(root, name) {
        return findAll(root, function (x) { return x.objectName === name }, [])
    }

    function thumbs(root) {
        return named(root, "pdfPageThumb").filter(function (x) {
            return String(x.source).indexOf("image://lautta-pdf-thumb/") === 0
        })
    }

    function tileOf(grid, index) {
        var tiles = named(grid, "pdfPageTile").filter(function (x) { return x.current !== undefined && x.visible })
        for (var i = 0; i < tiles.length; ++i) {
            var n = named(tiles[i], "pdfPageNumber")[0]
            if (n && n.text === String(index + 1))
                return tiles[i]
        }
        return null
    }

    function isInside(ancestor, item) {
        for (var p = item; p; p = p.parent) {
            if (p === ancestor)
                return true
        }
        return false
    }

    function onScreen(grid, item) {
        var top = item.mapToItem(grid, 0, 0).y
        return top >= 0 && top + item.height <= grid.height
    }

    function checkMenu(viewer) {
        var entry = findAll(viewer, function (x) { return x.text === qsTrId("lautta-viewers-pdf-pages-menu") }, [])
        var menus = findAll(viewer, function (x) { return x._isPullDownMenu === true }, [])
        expect(entry.length === 1 && menus.length === 1 && isInside(menus[0], entry[0]),
               "the pulley has Pages: " + entry.length + " " + menus.length)
    }

    function checkPages(viewer, label) {
        var view = named(viewer, "pdfPages")[0]
        view.positionViewAtIndex(29, ListView.Beginning)
        waitFor(function () { return viewer.currentPage === 30 }, label + ": page 30 in the viewer")
        viewer.showPages()
        waitFor(function () { return pageStack.currentPage !== viewer && !pageStack.busy }, label + ": the Pages page")
        var pages = pageStack.currentPage
        var grid = named(pages, "pdfPagesGrid")[0]
        if (!expect(grid && pages.pageCount === 60 && pages.currentPage === 29,
                    label + ": Pages knows 60 pages and page 30: " + pages.pageCount + " " + pages.currentPage))
            return
        expect(grid.count === 60, label + ": a tile per page: " + grid.count)
        var asked = thumbs(grid)
        expect(asked.length > 0 && asked.length < 60,
               label + ": only tiles near the screen ask for a thumbnail: " + asked.length + " of 60")
        var current = tileOf(grid, 29)
        if (!expect(current && onScreen(grid, current), label + ": page 30's tile is on screen"))
            return
        var frames = findAll(grid, function (x) { return x.border !== undefined && x.visible && x.border.color === Theme.highlightColor }, [])
        expect(frames.length === 1 && current.current && isInside(current, frames[0]),
               label + ": page 30 and only it is marked: " + frames.length)
        var image = named(current, "pdfPageThumb")[0]
        waitFor(function () { return image.status === Image.Ready }, label + ": page 30's thumbnail")
        expect(image.width <= grid.sheetWidth + 0.5 && image.height <= Math.ceil(grid.sheetWidth * 1.5) + 0.5
               && image.width > 0 && image.sourceSize.width === grid.sheetWidth,
               label + ": the thumbnail fits its tile: " + image.width + "x" + image.height + " in " + grid.sheetWidth)
        expect(image.implicitWidth <= grid.sheetWidth, label + ": drawn at tile width, not page width: " + image.implicitWidth)

        grid.positionViewAtEnd()
        var last = null
        waitFor(function () { last = tileOf(grid, 59); return last !== null }, label + ": the last tile")
        tools.mouse(last, "press", last.width / 2, last.height / 2)
        tools.mouse(last, "release", last.width / 2, last.height / 2)
        waitFor(function () { return pageStack.currentPage === viewer && !pageStack.busy }, label + ": back in the viewer")
        expect(viewer.topPage === 59 || viewer.currentPage === 60,
               label + ": a tap went to page 60: top " + viewer.topPage + ", current " + viewer.currentPage)
    }

    Component.onCompleted: {
        if (!tools.installFixture(Qt.resolvedUrl("host/fixtures/sixty-pages.pdf"), "lautta://user-documents/pages-sixty.pdf"))
            console.error("FAILED: fixture not installed")
        pageStack.push(emptyPage, {}, PageStackAction.Immediate)
        var viewer = pageStack.push(viewerComponent, {}, PageStackAction.Immediate)
        waitFor(function () { return viewer.document.loaded && !pageStack.busy }, "the PDF")
        checkMenu(viewer)
        checkPages(viewer, "portrait")
        window.deviceOrientation = Orientation.Landscape
        waitFor(function () { return viewer.isLandscape && !viewer.orientationTransitionRunning }, "landscape")
        checkPages(viewer, "landscape")
        window.deviceOrientation = Orientation.Portrait
        waitFor(function () { return !viewer.isLandscape && !viewer.orientationTransitionRunning }, "portrait again")
        reachedEnd = true
    }

    Component {
        id: emptyPage
        Page { }
    }

    Component {
        id: viewerComponent
        PdfViewer { uri: "lautta://user-documents/pages-sixty.pdf" }
    }
}
