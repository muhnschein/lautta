// SPDX-License-Identifier: LGPL-2.1-or-later
// Instantiates ImageViewer with realistic properties in a window (real Silica,
// real Lautta types); the check fails on QML errors and warnings, and when the
// viewer does not take up the folder listing that arrives after it opened.
import QtQuick 2.6
import Sailfish.Silica 1.0
import Lautta 1.0
import "../../qml/viewers"

ApplicationWindow {
    id: window

    ViewerTools { id: tools }

    Component.onCompleted: {
        tools.installFixture(Qt.resolvedUrl("host/fixtures/photo.jpg"), "lautta://user-pictures/photo.jpg")
        tools.installFixture(Qt.resolvedUrl("host/fixtures/photo.jpg"), "lautta://user-pictures/a.jpg")
        pageStack.push(pageComponent, {}, PageStackAction.Immediate)
        // Let the asynchronous loading finish and the delegates appear.
        tools.pump(500)
        // Swiping covers the whole folder once its listing arrives.
        var page = pageStack.currentPage
        if (!page.inFolder || page.total !== 2 || page.position < 1)
            console.error("the viewer must swipe through both images: " + page.position + " of " + page.total)
    }

    Component {
        id: pageComponent
        ImageViewer { uri: "lautta://user-pictures/photo.jpg"; folderUri: "lautta://user-pictures/" }
    }
}
