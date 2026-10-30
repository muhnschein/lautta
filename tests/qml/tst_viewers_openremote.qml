// SPDX-License-Identifier: LGPL-2.1-or-later
import QtQuick 2.6
import Sailfish.Silica 1.0
import Lautta 1.0
import "../../qml/dialogs"

ApplicationWindow {
    id: window

    ViewerTools { id: tools }

    Component.onCompleted: {
        pageStack.push(pageComponent, {}, PageStackAction.Immediate)
        tools.pump(500)
    }

    Component {
        id: pageComponent
        OpenRemoteDialog { uri: "lautta://nv-nas/docs/notes.md" }
    }
}
