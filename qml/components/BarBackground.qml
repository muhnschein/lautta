// SPDX-License-Identifier: LGPL-2.1-or-later Opaque: PanelBackground gradient over the ambience's
// dimmer highlight colour, as Silica's Drawer.
import QtQuick 2.6
import Sailfish.Silica 1.0

Rectangle {
    readonly property color ground: Theme.highlightDimmerColor

    objectName: "barBackground"
    // Opaque even if the ambience colour is translucent.
    color: Qt.rgba(ground.r, ground.g, ground.b, 1)

    PanelBackground {
        anchors.fill: parent
    }
}
