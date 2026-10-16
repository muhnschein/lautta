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

    Component.onCompleted: {
        tools.installFixture(Qt.resolvedUrl("host/fixtures/two-pages.pdf"), "lautta://user-documents/two-pages.pdf")
        tools.installFixture(Qt.resolvedUrl("host/fixtures/secret.pdf"), "lautta://user-documents/locked.pdf")
        pageStack.push(pageComponent, {}, PageStackAction.Immediate)
        // Let the asynchronous loading finish and the pages appear.
        tools.pump(800)
        var viewer = pageStack.currentPage
        viewer.zoomTo(2.5, 100, 100)
        tools.pump(800)
        viewer.zoomTo(1, 100, 100)
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
        tools.pump(800)
        // The Unlocking state while a password is tried.
        var locked = pageStack.currentPage
        locked.unlocking = true
        tools.pump(200)
        locked.unlocking = false
    }

    Component {
        id: pageComponent
        PdfViewer { uri: "lautta://user-documents/two-pages.pdf" }
    }

    Component {
        id: lockedComponent
        PdfViewer { uri: "lautta://user-documents/locked.pdf" }
    }
}
