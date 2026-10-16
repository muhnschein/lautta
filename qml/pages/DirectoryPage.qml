// SPDX-License-Identifier: LGPL-2.1-or-later
// A folder (SPEC §9, §15.3; boards Directory, DirectoryPulley,
// DirectoryContext, DirectorySelect, DirectoryGrid, DirectoryLandscape and
// the State* boards). All of it lives in components/directory/DirectoryView.
import QtQuick 2.6
import Sailfish.Silica 1.0
import "../components/directory"

Page {
    id: page

    property string uri
    // Shown once: later activations are a return to the folder.
    property bool shown

    allowedOrientations: Orientation.All

    // Coming back to a folder (from a subfolder, a viewer or another app)
    // lists it again when nothing watches it (BRW-11); there is no Refresh.
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
