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

    function yOf(viewer, item, at) {
        return item.mapToItem(viewer, 0, at).y
    }

    function checkLayout(viewer, label) {
        var bar = named(viewer, "pdfSearchBar")
        var view = named(viewer, "pdfPages")
        var menus = findAll(viewer, function (x) { return x._isPullDownMenu === true }, [])
        var fields = findAll(viewer, function (x) { return x.placeholderText === qsTrId("lautta-viewers-pdf-search-placeholder") }, [])
        if (!expect(menus.length === 1 && fields.length === 1, label + ": one pulley, one search field: "
                    + menus.length + " " + fields.length) || !bar || !view)
            return
        var field = fields[0]
        var host = menus[0].flickable
        view.positionViewAtBeginning()
        expect(bar.visible && field.visible, label + ": the search bar shows")
        expect(Math.abs(yOf(viewer, bar, bar.height) - viewer.height) < 0.5,
               label + ": the search bar sits at the bottom: " + yOf(viewer, bar, bar.height) + " vs " + viewer.height)
        expect(Math.abs(yOf(viewer, view, view.height) - yOf(viewer, bar, 0)) < 0.5,
               label + ": the pages end where the search bar starts: " + yOf(viewer, view, view.height)
               + " vs " + yOf(viewer, bar, 0))
        expect(yOf(viewer, field, 0) >= yOf(viewer, bar, 0) - 0.5,
               label + ": the search field is in the bar: " + yOf(viewer, field, 0))
        expect(Math.abs(yOf(viewer, host, 0)) < 0.5,
               label + ": the pulley's flickable starts at the page's top edge: " + yOf(viewer, host, 0))
        expect(Math.abs(yOf(viewer, menus[0], menus[0].height)) < 1,
               label + ": the pulley comes out at the top edge, above the header: "
               + yOf(viewer, menus[0], menus[0].height))
    }

    Component.onCompleted: {
        if (!tools.installFixture(Qt.resolvedUrl("host/fixtures/searchable.pdf"), "lautta://user-documents/bar-search.pdf"))
            console.error("FAILED: fixture not installed")
        pageStack.push(emptyPage, {}, PageStackAction.Immediate)
        var viewer = pageStack.push(viewerComponent, {}, PageStackAction.Immediate)
        waitFor(function () { return viewer.document.loaded && !pageStack.busy }, "the PDF")
        var bar = named(viewer, "pdfSearchBar")
        var view = named(viewer, "pdfPages")
        expect(!bar.visible && Math.abs(yOf(viewer, view, view.height) - viewer.height) < 0.5,
               "no search: no bar, the pages reach the bottom: " + yOf(viewer, view, view.height))
        viewer.openSearch()
        viewer.searchText = "lautta"
        waitFor(function () { return viewer.document.currentHit > 0 && !viewer.document.searching }, "a hit")
        checkLayout(viewer, "portrait")
        window.deviceOrientation = Orientation.Landscape
        waitFor(function () { return viewer.isLandscape && !viewer.orientationTransitionRunning }, "landscape")
        checkLayout(viewer, "landscape")
        window.deviceOrientation = Orientation.Portrait
        waitFor(function () { return !viewer.isLandscape && !viewer.orientationTransitionRunning }, "portrait again")
        viewer.closeSearch()
        expect(!bar.visible && Math.abs(yOf(viewer, view, view.height) - viewer.height) < 0.5,
               "search closed: the bar goes, the pages reach the bottom again: " + yOf(viewer, view, view.height))
        reachedEnd = true
    }

    Component {
        id: emptyPage
        Page { }
    }

    Component {
        id: viewerComponent
        PdfViewer { uri: "lautta://user-documents/bar-search.pdf" }
    }
}
