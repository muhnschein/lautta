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

    Component.onCompleted: {
        tools.installFixture(Qt.resolvedUrl("host/fixtures/two-pages.pdf"), "lautta://user-documents/two-pages.pdf")
        tools.installFixture(Qt.resolvedUrl("host/fixtures/secret.pdf"), "lautta://user-documents/locked.pdf")
        tools.installFixture(Qt.resolvedUrl("host/fixtures/linked.pdf"), "lautta://user-documents/linked.pdf")
        pageStack.push(pageComponent, {}, PageStackAction.Immediate)
        // Let the asynchronous loading finish and the pages appear.
        tools.pump(800)
        var viewer = pageStack.currentPage
        viewer.zoomTo(2.5, 100, 100)
        tools.pump(800)
        viewer.zoomTo(1, 100, 100)
        // Contents without an outline, and "Go to page".
        viewer.showContents()
        tools.pump(800)
        pageStack.currentPage.chosen(1)
        tools.pump(800)
        pageStack.push(linkedComponent, {}, PageStackAction.Immediate)
        tools.pump(800)
        var linked = pageStack.currentPage
        // Contents with an outline; a link within the PDF and back again.
        linked.showContents()
        tools.pump(800)
        pageStack.currentPage.chosen(0)
        tools.pump(800)
        linked.followLink({ "page": 1, "top": 0.5, "uri": "" })
        tools.pump(400)
        if (linked.backPage !== 1)
            console.error("FAILED: back to page " + linked.backPage)
        linked.followLink({ "page": -1, "top": -1, "uri": "https://example.org/x" })
        tools.pump(800)
        pageStack.currentPage.reject()
        tools.pump(800)
        pageStack.pop(viewer, PageStackAction.Immediate)
        tools.pump(200)
        pageStack.push(lockedComponent, {}, PageStackAction.Immediate)
        // The password dialog opens on its own; a wrong password asks again.
        tools.pump(800)
        var dialog = pageStack.currentPage
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
        id: linkedComponent
        PdfViewer { uri: "lautta://user-documents/linked.pdf" }
    }

    Component {
        id: lockedComponent
        PdfViewer { uri: "lautta://user-documents/locked.pdf" }
    }
}
