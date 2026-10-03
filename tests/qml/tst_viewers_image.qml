// SPDX-License-Identifier: LGPL-2.1-or-later
// Instantiates ImageViewer with realistic properties in a window (real Silica,
// real Lautta types); the check fails on QML errors and warnings.
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
    }

    Component {
        id: pageComponent
        ImageViewer { uri: "lautta://user-pictures/photo.jpg"; folderUri: "lautta://user-pictures/" }
    }
}
