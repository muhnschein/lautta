// SPDX-License-Identifier: LGPL-2.1-or-later
import QtQuick 2.6
import Sailfish.Silica 1.0
import "../components"
import "../components/FileIcons.js" as FileIcons

Item {
    id: root

    readonly property real unit: width / 234
    readonly property real step: 46 * unit
    readonly property real pitch: 44 * unit
    readonly property real glyphSize: Math.round(26 * unit)
    readonly property int lanes: 13
    readonly property int columns: 11
    readonly property var glyphs: ["file-database", "file-video", "file-document", "types/text", "types/markdown", "link",
        "types/json", "file-folder", "server", "home", "file-package", "lock", "types/shell", "dir-copy", "file-image",
        "file-audio", "file-archive", "types/rust"]

    function lit(lane) {
        return lane % 4 === 1
    }

    function glyphAt(lane, column) {
        if (lit(lane) && column % 4 === Math.floor(lane / 4) % 4)
            return "lautta"
        return FileIcons.sized(glyphs[(lane * columns + column) % glyphs.length], glyphSize)
    }

    function tint(lane) {
        if (lit(lane))
            return Theme.highlightColor
        return lane % 4 === 3 ? Theme.secondaryColor : Theme.primaryColor
    }

    objectName: "coverLanes"
    clip: true

    Item {
        objectName: "laneField"
        anchors.centerIn: parent
        width: 460 * root.unit
        height: 560 * root.unit
        rotation: -18

        Repeater {
            model: root.lanes

            Item {
                id: lane

                readonly property int lane: index

                objectName: "coverLane"
                x: (index % 2) * 24 * root.unit - 20 * root.unit
                y: index * root.pitch + 8 * root.unit
                width: root.columns * root.step
                height: root.glyphSize
                opacity: root.lit(index) ? 1 : (index % 4 === 3 ? 0.7 : 0.55)

                Repeater {
                    model: root.columns

                    AppIcon {
                        objectName: "laneGlyph"
                        x: index * root.step
                        iconSize: root.glyphSize
                        icon: root.glyphAt(lane.lane, index)
                        color: root.tint(lane.lane)
                    }
                }
            }
        }
    }
}
