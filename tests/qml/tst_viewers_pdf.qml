// SPDX-License-Identifier: LGPL-2.1-or-later
// Instantiates PdfViewer with realistic properties in a window (real Silica,
// real Lautta types); the check fails on QML errors and warnings.
import QtQuick 2.6
import Sailfish.Silica 1.0
import Lautta 1.0
import "../../qml/viewers"

ApplicationWindow {
    id: window

    ViewerTools { id: tools }

    // Runs the event loop until `done()` holds, for at most 30 s: the target
    // is emulated and slow.
    function waitFor(done) {
        for (var i = 0; i < 300 && !done(); ++i)
            tools.pump(100)
    }

    // Waits for the page on top to be shown and its PDF to be read.
    function shown() {
        waitFor(function() {
            var p = pageStack.currentPage
            return !pageStack.busy && p.document !== undefined && !p.document.loading
        })
        return pageStack.currentPage
    }

    Component.onCompleted: {
        tools.installFixture(Qt.resolvedUrl("host/fixtures/two-pages.pdf"), "lautta://user-documents/two-pages.pdf")
        tools.installFixture(Qt.resolvedUrl("host/fixtures/secret.pdf"), "lautta://user-documents/locked.pdf")
        tools.installFixture(Qt.resolvedUrl("host/fixtures/searchable.pdf"), "lautta://user-documents/searchable.pdf")
        pageStack.push(pageComponent, {}, PageStackAction.Immediate)
        var viewer = shown()
        viewer.zoomTo(2.5, 100, 100)
        tools.pump(500)
        viewer.zoomTo(1, 100, 100)

        // Search mode: hits marked on the pages, stepped through, nothing
        // found, and closed again.
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

        // The password dialog opens on its own; a wrong password asks again.
        pageStack.push(lockedComponent, {}, PageStackAction.Immediate)
        var locked = pageStack.currentPage
        waitFor(function() { return pageStack.currentPage !== locked && !pageStack.busy })
        var dialog = pageStack.currentPage
        dialog.wrong = true
        tools.pump(200)
        dialog.reject()
        waitFor(function() { return pageStack.currentPage === locked && !pageStack.busy })
        // The Unlocking state while a password is tried.
        locked.unlocking = true
        tools.pump(200)
        locked.unlocking = false
    }

    Component {
        id: pageComponent
        PdfViewer { uri: "lautta://user-documents/two-pages.pdf" }
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
