// SPDX-License-Identifier: LGPL-2.1-or-later
import QtQuick 2.6
import Sailfish.Silica 1.0
import "../components/directory"

Page {
    id: page

    property string uri
    property bool shown

    allowedOrientations: Orientation.All

    onStatusChanged: {
        if (status !== PageStatus.Active)
            return
        if (shown)
            view.model.revisit()
        shown = true
    }

    Connections {
        target: Qt.application
        onStateChanged: {
            if (Qt.application.state === Qt.ApplicationActive && page.status === PageStatus.Active)
                view.model.revisit()
        }
    }

    DirectoryView {
        id: view

        anchors.fill: parent
        uri: page.uri
        onOpenFolder: pageStack.push(Qt.resolvedUrl("DirectoryPage.qml"), { "uri": uri })
    }
}
