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

    allowedOrientations: Orientation.All

    DirectoryView {
        anchors.fill: parent
        uri: page.uri
        onOpenFolder: pageStack.push(Qt.resolvedUrl("DirectoryPage.qml"), { "uri": uri })
    }
}
