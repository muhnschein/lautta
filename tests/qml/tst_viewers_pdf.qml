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

    // Runs the event loop until `done()` holds (the target is slow when
    // emulated: unlocking AES-256 takes seconds there).
    function waitFor(done) {
        for (var i = 0; i < 300 && !done(); ++i)
            tools.pump(100)
    }

    // Waits for the page on top to be shown and its PDF to be read.
    function shown() {
        waitFor(function() {
            var p = pageStack.currentPage
            return !pageStack.busy && p.loading !== undefined && !p.loading
        })
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
        pageStack.push(pageComponent, {}, PageStackAction.Immediate)
        var viewer = shown()
        viewer.zoomTo(2.5, 100, 100)
        tools.pump(800)
        viewer.zoomTo(1, 100, 100)
        // Contents without an outline, and "Go to page".
        viewer.showContents()
        waitFor(function() { return pageStack.currentPage.chosen !== undefined && !pageStack.busy })
        pageStack.currentPage.chosen(1)
        waitFor(function() { return pageStack.currentPage === viewer && !pageStack.busy })
        pageStack.push(linkedComponent, {}, PageStackAction.Immediate)
        var linked = shown()
        // Contents with an outline; a link within the PDF and back again.
        linked.showContents()
        waitFor(function() { return pageStack.currentPage.chosen !== undefined && !pageStack.busy })
        pageStack.currentPage.chosen(0)
        waitFor(function() { return pageStack.currentPage === linked && !pageStack.busy })
        linked.followLink({ "page": 1, "top": 0.5, "uri": "" })
        tools.pump(400)
        if (linked.backPage !== 1)
            console.error("FAILED: back to page " + linked.backPage)
        linked.followLink({ "page": -1, "top": -1, "uri": "https://example.org/x" })
        waitFor(function() { return pageStack.currentPage.url !== undefined && !pageStack.busy })
        pageStack.currentPage.reject()
        waitFor(function() { return pageStack.currentPage === linked && !pageStack.busy })
        pageStack.pop(viewer, PageStackAction.Immediate)
        waitFor(function() { return pageStack.currentPage === viewer && !pageStack.busy })
        pageStack.push(lockedComponent, {}, PageStackAction.Immediate)
        // The password dialog opens on its own; a wrong password asks again.
        var lockedViewer = pageStack.currentPage
        waitFor(function() { return pageStack.currentPage.wrong !== undefined })
        var dialog = pageStack.currentPage
        if (dialog.wrong === undefined) {
            console.error("FAILED: no password dialog; status " + lockedViewer.status
                          + ", wantPassword " + lockedViewer.wantPassword + ", asked " + lockedViewer.asked
                          + ", problem '" + lockedViewer.problem + "', loading " + lockedViewer.loading
                          + ", busy " + pageStack.busy + ", depth " + pageStack.depth)
            return
        }
        dialog.wrong = true
        tools.pump(200)
        dialog.reject()
        // Cancel leaves the viewer locked, on top again.
        waitFor(function() { return pageStack.currentPage === lockedViewer && !pageStack.busy })
        // The Unlocking state while a password is tried.
        lockedViewer.unlocking = true
        tools.pump(200)
        lockedViewer.unlocking = false
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
        id: lockedComponent
        PdfViewer { uri: "lautta://user-documents/locked.pdf" }
    }
}
