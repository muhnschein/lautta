// SPDX-License-Identifier: LGPL-2.1-or-later
import QtQuick 2.6
import Sailfish.Silica 1.0
import Lautta 1.0
import "../../qml/viewers"

ApplicationWindow {
    id: window

    ViewerTools { id: tools }

    Component.onCompleted: {
        tools.installFixture(Qt.resolvedUrl("host/fixtures/data.bin"), "lautta://user-documents/clip.mp4")
        pageStack.push(pageComponent, {}, PageStackAction.Immediate)
        tools.pump(500)
    }

    Component {
        id: pageComponent
        MediaPlayer { uri: "lautta://user-documents/clip.mp4"; video: true }
    }
}
