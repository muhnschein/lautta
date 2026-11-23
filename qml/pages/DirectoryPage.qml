// SPDX-License-Identifier: LGPL-2.1-or-later
import QtQuick 2.6
import Sailfish.Silica 1.0
import Lautta 1.0
import "../components/directory"

Page {
    id: page

    property string uri
    property bool shown
    readonly property string coverKind: "grid"
    readonly property string coverTitle: App.nameOf(uri)
    readonly property var coverModel: view.model

    allowedOrientations: Orientation.All

    Component.onCompleted: Operations.holdArchive(uri)
    Component.onDestruction: Operations.releaseArchive(uri)

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
