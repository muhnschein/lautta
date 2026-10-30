// SPDX-License-Identifier: LGPL-2.1-or-later
import QtQuick 2.6
import Sailfish.Silica 1.0
import Lautta 1.0
import "../../qml/viewers"

ApplicationWindow {
    id: window

    ViewerTools { id: tools }

    Component.onCompleted: {
        tools.installFixture(Qt.resolvedUrl("host/fixtures/notes-crlf.txt"), "lautta://user-documents/notes.txt")
        pageStack.push(pageComponent, {}, PageStackAction.Immediate)
        tools.pump(500)
    }

    Component {
        id: pageComponent
        TextViewer { uri: "lautta://user-documents/notes.txt" }
    }
}
