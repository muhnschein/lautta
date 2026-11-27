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
        for (var i = 0; i < 200 && !done(); ++i)
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
        var all = findAll(root, function (x) { return x.objectName === name }, [])
        expect(all.length === 1, "one " + name + ": " + all.length)
        return all[0]
    }

    function drag(item, fromX, fromY, toX, toY) {
        var steps = 24
        tools.mouse(item, "press", fromX, fromY)
        for (var i = 1; i <= steps; ++i)
            tools.mouse(item, "move", fromX + (toX - fromX) * i / steps, fromY + (toY - fromY) * i / steps)
        tools.mouse(item, "release", toX, toY)
    }

    function settle(viewer) {
        var flicks = findAll(viewer, function (x) { return x.cancelFlick !== undefined && x.moving !== undefined }, [])
        for (var i = 0; i < flicks.length; ++i)
            flicks[i].cancelFlick()
        waitFor(function () {
            for (var j = 0; j < flicks.length; ++j) {
                if (flicks[j].moving)
                    return false
            }
            return true
        }, "the pages to stop")
    }

    function sheetIn(view) {
        return view.itemAt(view.width / 2, view.contentY + view.height / 2)
    }

    function sheetLeft(viewer, sheet) {
        return sheet.mapToItem(viewer, 0, 0).x
    }

    function checkPanned(viewer, label) {
        var view = named(viewer, "pdfPages")
        if (!view)
            return
        viewer.zoomTo(2.5, view.width / 2, view.height / 2)
        settle(viewer)
        var sheet = sheetIn(view)
        var zoomedLeft = sheetLeft(viewer, sheet)
        if (!expect(sheet && sheet.width > 2 * view.width && zoomedLeft < -view.width / 2
                    && zoomedLeft + sheet.width > 1.5 * view.width,
                    label + ": zoomed around the middle, both page edges off screen: " + zoomedLeft + " " + sheet.width))
            return

        var top = view.contentY
        drag(view, view.width / 2, view.height * 0.7, view.width / 2 + 4, view.height * 0.3)
        settle(viewer)
        expect(view.contentY > top + view.height * 0.2,
               label + ": an upward drag turns the pages: " + view.contentY + " vs " + top)
        expect(Math.abs(sheetLeft(viewer, sheet) - zoomedLeft) < 0.5,
               label + ": an upward drag a few pixels aslant, as a finger makes, keeps the page where the zoom put it: " + sheetLeft(viewer, sheet) + " vs " + zoomedLeft)

        var before = sheetLeft(viewer, sheet)
        var reach = view.width / 3
        drag(view, view.width * 0.8, view.height / 2, view.width * 0.8 - reach, view.height / 2)
        settle(viewer)
        var moved = before - sheetLeft(viewer, sheet)
        expect(moved > reach / 2 && moved < reach + 0.5,
               label + ": a drag to the left moves the page left with the finger: " + moved + " of " + reach)

        before = sheetLeft(viewer, sheet)
        drag(view, view.width * 0.2, view.height / 2, view.width * 0.2 + reach, view.height / 2)
        settle(viewer)
        moved = sheetLeft(viewer, sheet) - before
        expect(moved > reach / 2 && moved < reach + 0.5,
               label + ": a drag to the right moves it back: " + moved + " of " + reach)

        drag(view, view.width * 0.05, view.height / 2, view.width * 0.95, view.height / 2)
        settle(viewer)
        expect(Math.abs(sheetLeft(viewer, sheet)) < 0.5,
               label + ": dragging right stops at the page's left edge: " + sheetLeft(viewer, sheet))

        for (var i = 0; i < 2; ++i) {
            drag(view, view.width * 0.95, view.height / 2, view.width * 0.05, view.height / 2)
            settle(viewer)
        }
        expect(Math.abs(sheetLeft(viewer, sheet) + sheet.width - view.width) < 0.5,
               label + ": dragging left stops at the page's right edge: " + (sheetLeft(viewer, sheet) + sheet.width))

        viewer.zoomTo(1, 0, 0)
        waitFor(function () { return viewer.renderZoom === 1 }, label + ": zoom back")
        sheet = sheetIn(view)
        expect(sheet && Math.abs(sheetLeft(viewer, sheet)) < 0.5 && Math.abs(sheet.width - view.width) < 0.5,
               label + ": zoomed out, the page fits again: " + (sheet ? sheetLeft(viewer, sheet) + " " + sheet.width : sheet))
    }

    Component.onCompleted: {
        if (!tools.installFixture(Qt.resolvedUrl("host/fixtures/twelve-pages.pdf"), "lautta://user-documents/pan.pdf"))
            console.error("FAILED: fixture not installed")
        pageStack.push(emptyPage, {}, PageStackAction.Immediate)
        var viewer = pageStack.push(viewerComponent, {}, PageStackAction.Immediate)
        waitFor(function () { return viewer.document.loaded && !pageStack.busy }, "the PDF")
        named(viewer, "pdfPages").forceLayout()
        named(viewer, "pdfPages").positionViewAtIndex(4, ListView.Beginning)
        checkPanned(viewer, "portrait")
        pageStack.pop(null, PageStackAction.Immediate)
        reachedEnd = true
    }

    Component {
        id: emptyPage
        Page { }
    }

    Component {
        id: viewerComponent
        PdfViewer { uri: "lautta://user-documents/pan.pdf" }
    }
}
