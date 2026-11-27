// SPDX-License-Identifier: LGPL-2.1-or-later
import QtQuick 2.6
import Sailfish.Silica 1.0

Rectangle {
    readonly property color ground: Theme.highlightDimmerColor

    objectName: "barBackground"
    color: Qt.rgba(ground.r, ground.g, ground.b, 1)

    PanelBackground {
        anchors.fill: parent
    }
}
