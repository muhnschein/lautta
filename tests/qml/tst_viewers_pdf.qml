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

    // Target is emulated and slow: unlocking AES-256 takes seconds.
    function waitFor(done, what) {
        for (var i = 0; i < 300 && !done(); ++i)
            tools.pump(100)
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

    function shown() {
        waitFor(function() {
            var p = pageStack.currentPage
            return !pageStack.busy && p.document !== undefined && !p.document.loading
        }, "the viewer")
        return pageStack.currentPage
    }

    function back(p) {
        waitFor(function() { return pageStack.currentPage === p && !pageStack.busy }, "the viewer again")
    }

    function opened(property) {
        waitFor(function() { return pageStack.currentPage[property] !== undefined && !pageStack.busy }, "a page with " + property)
        return pageStack.currentPage
    }

    function install(fixture, uri) {
        if (!tools.installFixture(Qt.resolvedUrl("host/fixtures/" + fixture), uri))
            console.error("FAILED: fixture " + fixture + " not installed")
    }

    Component.onCompleted: {
        install("two-pages.pdf", "lautta://user-documents/two-pages.pdf")
        install("secret.pdf", "lautta://user-documents/locked.pdf")
        install("linked.pdf", "lautta://user-documents/linked.pdf")
        install("searchable.pdf", "lautta://user-documents/searchable.pdf")
        pageStack.push(pageComponent, {}, PageStackAction.Immediate)
        var viewer = shown()
        viewer.zoomTo(2.5, 100, 100)
        waitFor(function() { return viewer.zoom === 2.5 && viewer.renderZoom === 2.5 }, "pages drawn at 2.5 times")
        viewer.zoomTo(1, 100, 100)
        waitFor(function() { return viewer.zoom === 1 && viewer.renderZoom === 1 }, "pages drawn at their size again")

        viewer.showContents()
        opened("chosen").chosen(1)
        back(viewer)

        pageStack.push(linkedComponent, {}, PageStackAction.Immediate)
        var linked = shown()
        linked.showContents()
        opened("chosen").chosen(0)
        back(linked)
        linked.followLink({ "page": 1, "top": 0.5, "uri": "" })
        expect(linked.backPage === 1, "back to page " + linked.backPage)
        linked.followLink({ "page": -1, "top": -1, "uri": "https://example.org/x" })
        opened("url").reject()
        back(linked)
        pageStack.pop(viewer, PageStackAction.Immediate)
        back(viewer)

        pageStack.push(searchComponent, {}, PageStackAction.Immediate)
        var searching = shown()
        var doc = searching.document
        searching.openSearch()
        searching.searchText = "lautta"
        waitFor(function() { return doc.currentHit > 0 && !doc.searching }, "a hit")
        var first = doc.currentHit
        expect(doc.hitCount > 1, "more than one hit: " + doc.hitCount)
        searching.nextHit()
        expect(doc.currentHit === first % doc.hitCount + 1, "the next hit: " + first + " -> " + doc.currentHit)
        searching.previousHit()
        expect(doc.currentHit === first, "back to the first: " + doc.currentHit)
        searching.zoomTo(2.5, 100, 100)
        searching.nextHit()
        expect(doc.currentHit === first % doc.hitCount + 1, "the next hit, zoomed in: " + doc.currentHit)
        searching.searchText = "no such words"
        waitFor(function() { return doc.currentHit === 0 && !doc.searching }, "a search without hits")
        expect(doc.hitCount === 0, "no hits: " + doc.hitCount)
        searching.closeSearch()
        pageStack.pop(viewer, PageStackAction.Immediate)
        back(viewer)

        pageStack.push(lockedComponent, {}, PageStackAction.Immediate)
        var locked = pageStack.currentPage
        var dialog = opened("wrong")
        if (dialog.wrong === undefined) {
            console.error("FAILED: no password dialog; status " + locked.status
                          + ", wantPassword " + locked.wantPassword + ", asked " + locked.asked
                          + ", problem '" + locked.problem + "', loading " + locked.loading
                          + ", busy " + pageStack.busy + ", depth " + pageStack.depth)
            return
        }
        dialog.wrong = true
        var fields = findAll(dialog, function(x) { return x.errorHighlight !== undefined && x.label !== undefined }, [])
        expect(fields.length === 1 && fields[0].label === qsTrId("lautta-viewers-pdf-wrong-password") && fields[0].errorHighlight,
               "a wrong password is said in the field: " + (fields.length ? fields[0].label : fields.length))
        dialog.reject()
        back(locked)
        locked.unlocking = true
        var said = findAll(locked, function(x) { return x.text === qsTrId("lautta-viewers-pdf-unlocking") }, [])
        expect(said.length === 1 && said[0].parent.visible, "unlocking is said")
        locked.unlocking = false
        expect(!said[0].parent.visible, "and gone after")
        reachedEnd = true
    }

    Component {
        id: pageComponent
        PdfViewer { uri: "lautta://user-documents/two-pages.pdf" }
    }

    Component {
        id: linkedComponent
        PdfViewer { uri: "lautta://user-documents/linked.pdf" }
    }

    Component {
        id: searchComponent
        PdfViewer { uri: "lautta://user-documents/searchable.pdf" }
    }

    Component {
        id: lockedComponent
        PdfViewer { uri: "lautta://user-documents/locked.pdf" }
    }
}
