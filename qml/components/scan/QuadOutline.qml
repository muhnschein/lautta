// SPDX-License-Identifier: LGPL-2.1-or-later Page edges drawn over a picture of the same size.
import QtQuick 2.6
import Sailfish.Silica 1.0
import "ScanMath.js" as ScanMath

Item {
    id: outline

    // As ScanModel's `corners`.
    property var corners: ScanMath.whole
    property color color: Theme.highlightColor
    property real lineWidth: Math.max(2, Math.round(Theme.paddingSmall / 2))

    Repeater {
        model: 4

        Rectangle {
            readonly property var line: ScanMath.edge(outline.corners, index, outline.width, outline.height)

            x: line.x
            y: line.y - height / 2
            width: line.length
            height: outline.lineWidth
            color: outline.color
            antialiasing: true
            transformOrigin: Item.Left
            rotation: line.angle
        }
    }
}
