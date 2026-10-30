// SPDX-License-Identifier: LGPL-2.1-or-later
import QtQuick 2.6
import Sailfish.Silica 1.0
import Lautta 1.0
import "../../qml/viewers"

ApplicationWindow {
    id: window

    ViewerTools { id: tools }

    // Target is emulated and slow: unlocking AES-256 takes seconds.
    function waitFor(done) {
        for (var i = 0; i < 300 && !done(); ++i)
            tools.pump(100)
    }

    function shown() {
        waitFor(function() {
            var p = pageStack.currentPage
            return !pageStack.busy && p.document !== undefined && !p.document.loading
        })
        return pageStack.currentPage
    }

    function back(p) {
        waitFor(function() { return pageStack.currentPage === p && !pageStack.busy })
    }

    function opened(property) {
        waitFor(function() { return pageStack.currentPage[property] !== undefined && !pageStack.busy })
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
        tools.pump(500)
        viewer.zoomTo(1, 100, 100)

        viewer.showContents()
        opened("chosen").chosen(1)
        back(viewer)

        pageStack.push(linkedComponent, {}, PageStackAction.Immediate)
        var linked = shown()
        linked.showContents()
        opened("chosen").chosen(0)
        back(linked)
        linked.followLink({ "page": 1, "top": 0.5, "uri": "" })
        tools.pump(400)
        if (linked.backPage !== 1)
            console.error("FAILED: back to page " + linked.backPage)
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
        waitFor(function() { return doc.currentHit > 0 && !doc.searching })
        searching.nextHit()
        searching.previousHit()
        searching.zoomTo(2.5, 100, 100)
        searching.nextHit()
        searching.searchText = "no such words"
        waitFor(function() { return doc.currentHit === 0 && !doc.searching })
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
        tools.pump(200)
        dialog.reject()
        back(locked)
        locked.unlocking = true
        tools.pump(200)
        locked.unlocking = false
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
