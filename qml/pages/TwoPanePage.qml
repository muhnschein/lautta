// SPDX-License-Identifier: LGPL-2.1-or-later
// Two folders side by side in landscape (board TwoPane): copy or move
// between them from the context menu or the selection panel. In portrait
// only the left folder is shown.
import QtQuick 2.6
import Sailfish.Silica 1.0
import "../components/directory"

Page {
    id: page

    property string leftUri
    property string rightUri

    allowedOrientations: Orientation.All

    Row {
        anchors.fill: parent

        DirectoryView {
            id: left

            width: page.isLandscape ? parent.width / 2 : parent.width
            height: parent.height
            compact: page.isLandscape
            uri: page.leftUri
            otherUri: page.isLandscape ? right.uri : ""
            onOpenFolder: left.uri = uri
        }

        DirectoryView {
            id: right

            visible: page.isLandscape
            width: parent.width / 2
            height: parent.height
            compact: true
            uri: page.rightUri
            otherUri: left.uri
            onOpenFolder: right.uri = uri
        }
    }

    Rectangle {
        visible: page.isLandscape
        x: page.width / 2
        width: 1
        height: page.height
        color: Theme.rgba(Theme.highlightColor, 0.3)
    }
}
