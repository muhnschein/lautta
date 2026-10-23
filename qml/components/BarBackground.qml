// SPDX-License-Identifier: LGPL-2.1-or-later
// The backdrop of the bars at the bottom of a page (UI-9): the selection and
// paste panels, the activity bar, the undo notice and the search progress.
// Opaque, so what scrolls under a bar never shows through its text and
// icons: Silica's own docked panel gradient (PanelBackground) laid on the
// ambience's dimmer highlight colour, the opaque ground of Silica's Drawer.
// Both follow the ambience, so a bar reads on light and dark ones alike.
import QtQuick 2.6
import Sailfish.Silica 1.0

Rectangle {
    readonly property color ground: Theme.highlightDimmerColor

    objectName: "barBackground"
    // Fully opaque even if an ambience gave its colour some transparency.
    color: Qt.rgba(ground.r, ground.g, ground.b, 1)

    PanelBackground {
        anchors.fill: parent
    }
}
