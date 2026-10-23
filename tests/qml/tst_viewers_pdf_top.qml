// SPDX-License-Identifier: LGPL-2.1-or-later
// A PDF opened for the first time starts at its top (PRV-12): once the
// pages are there the whole page header shows, with the page count, and the
// list stays there until the user scrolls. Two pages taller than the screen
// together, and one page six times as tall as it is wide. A PDF left at its
// top opens there again, also when the middle of the view is on a later page
// (pages a sixth as tall as they are wide); one left at its second page
// opens with that page at the top.
import QtQuick 2.6
import Sailfish.Silica 1.0
import Lautta 1.0
import "../../qml/viewers"

ApplicationWindow {
    id: window

    ViewerTools { id: tools }

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

    // The header's top is the list's top and in view.
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

    // Pushes the viewer and waits for its pages; returns its list.
    function show(uri) {
        var viewer = pageStack.push(pdfComponent, { "uri": uri }, PageStackAction.Immediate)
        for (var i = 0; i < 300 && !viewer.document.loaded; ++i)
            tools.pump(20)
        // A remembered page is shown once the pages are laid out.
        tools.pump(100)
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
        // The pages are rendered meanwhile.
        tools.pump(1500)
        checkTop(fixture, view, "a moment later")
        pageStack.pop(null, PageStackAction.Immediate)
    }

    // Scrolled a little, the first page still at the top and the second in
    // the middle of the view: reopened, the PDF is at its top again.
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
        // Reading is remembered once scrolling has stopped for a moment.
        tools.pump(1500)
        pageStack.pop(null, PageStackAction.Immediate)
        tools.pump(300)
        checkTop("strips.pdf", show(uri), "reopened")
        pageStack.pop(null, PageStackAction.Immediate)

        var reading = install("searchable.pdf")
        show(reading).positionViewAtIndex(1, ListView.Beginning)
        tools.pump(1500)
        pageStack.pop(null, PageStackAction.Immediate)
        tools.pump(300)
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
