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
        pageStack.push(pageComponent, {}, PageStackAction.Immediate)
        // Let the asynchronous loading finish and the pages appear.
        tools.pump(800)
        var viewer = pageStack.currentPage
        viewer.zoomTo(2.5, 100, 100)
        tools.pump(800)
        viewer.zoomTo(1, 100, 100)
        pageStack.push(lockedComponent, {}, PageStackAction.Immediate)
        // The password dialog opens on its own; a wrong password asks again.
        tools.pump(800)
        var dialog = pageStack.currentPage
        dialog.wrong = true
        tools.pump(200)
        dialog.reject()
        tools.pump(800)
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
