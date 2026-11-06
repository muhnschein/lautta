// SPDX-License-Identifier: LGPL-2.1-or-later
import QtQuick 2.6
import Sailfish.Silica 1.0
import Lautta 1.0
import "../../qml/viewers"

ApplicationWindow {
    id: window
    property bool reachedEnd

    ViewerTools { id: tools }
    PdfDocument { id: probe }

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

    function checkTop(name, view, when) {
        if (Math.abs(view.contentY - view.originY) > 0.5 || view.headerItem.y < view.contentY - 0.5)
            console.error("FAILED: " + name + " " + when + ": the list is not at its top: contentY "
                          + view.contentY + ", originY " + view.originY + ", header at " + view.headerItem.y
                          + ", " + view.headerItem.height + " high")
    }

    function install(fixture) {
        var uri = "lautta://user-documents/top-" + fixture
        if (!tools.installFixture(Qt.resolvedUrl("host/fixtures/" + fixture), uri))
            console.error("FAILED: fixture " + fixture + " not installed")
        return uri
    }

    function waitFor(done, what) {
        for (var i = 0; i < 200 && !done(); ++i)
            tools.pump(50)
        if (!done())
            console.error("FAILED: timed out waiting for " + what)
        return done()
    }

    // Every page image in view rendered: nothing left that loads.
    function rendered(viewer) {
        var pages = findAll(viewer, function(x) { return x.status !== undefined && String(x.source).indexOf("image://lautta-pdf/") === 0 }, [])
        return pages.length > 0 && pages.every(function(x) { return x.status === Image.Ready })
    }

    function show(uri) {
        var viewer = pageStack.push(pdfComponent, { "uri": uri }, PageStackAction.Immediate)
        waitFor(function() { return viewer.document.loaded }, uri)
        var view = findAll(viewer, function(x) { return x.headerItem !== undefined }, [])[0]
        view.forceLayout()
        return view
    }

    function open(fixture, pages) {
        var view = show(install(fixture))
        if (view.count !== pages || view.contentHeight <= view.height || view.headerItem.description.trim() === "")
            console.error("FAILED: " + fixture + ": " + view.count + " pages, " + view.contentHeight
                          + " high in a view " + view.height + " high, header '" + view.headerItem.description + "'")
        checkTop(fixture, view, "when loaded")
        waitFor(function() { return rendered(pageStack.currentPage) }, fixture + " rendered")
        checkTop(fixture, view, "once rendered")
        pageStack.pop(null, PageStackAction.Immediate)
    }

    function reopen() {
        var uri = install("strips.pdf")
        var view = show(uri)
        var viewer = pageStack.currentPage
        var first = viewer.document.heightFor(0, view.width)
        var middle = first + view.spacing + viewer.document.heightFor(1, view.width) / 2
        view.contentY = Math.max(view.originY + 1, middle - view.height / 2)
        if (viewer.currentPage !== 2 || view.contentY >= first)
            console.error("FAILED: strips.pdf: page " + viewer.currentPage + " in the middle, contentY "
                          + view.contentY + " with the first page " + first + " high")
        // Position is kept 1 s after the last scroll.
        tools.pump(1500)
        pageStack.pop(null, PageStackAction.Immediate)
        checkTop("strips.pdf", show(uri), "reopened")
        pageStack.pop(null, PageStackAction.Immediate)

        var reading = install("searchable.pdf")
        show(reading).positionViewAtIndex(1, ListView.Beginning)
        probe.uri = reading
        waitFor(function() {
            probe.reload()
            for (var i = 0; i < 200 && probe.loading; ++i)
                tools.pump(50)
            return probe.lastPage === 1
        }, "page 2 kept: " + probe.lastPage)
        pageStack.pop(null, PageStackAction.Immediate)
        var again = show(reading)
        var top = again.indexAt(again.width / 2, again.contentY + 1)
        if (top !== 1)
            console.error("FAILED: reopened with page " + (top + 1) + " at the top, not 2")
        pageStack.pop(null, PageStackAction.Immediate)
    }

    Component.onCompleted: {
        pageStack.push(emptyPage, {}, PageStackAction.Immediate)
        open("two-pages.pdf", 2)
        open("tall-page.pdf", 1)
        reopen()
        reachedEnd = true
    }

    Component {
        id: emptyPage
        Page { }
    }

    Component {
        id: pdfComponent
        PdfViewer { }
    }
}
