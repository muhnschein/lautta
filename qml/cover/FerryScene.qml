// SPDX-License-Identifier: LGPL-2.1-or-later
// The crossing at the foot of the cover (SPEC INT-3, board Covers): two
// banks joined by a cable over the water, and the ferry that carries the
// files. At rest the ferry waits at the left bank; while work runs it is
// `progress` of the way across, from `fromName` to `toName`. When the work
// waits for an answer, it stops with a flag up and the water calms.
import QtQuick 2.6
import Sailfish.Silica 1.0

Item {
    id: scene

    // 0 at the left bank, 1 at the right one.
    property real progress: 0
    // Bytes are moving: the ferry leaves a wake and the water ripples.
    property bool moving: false
    // The work waits for the user: a flag goes up.
    property bool waiting: false
    property string fromName
    property string toName

    // One unit of the 234 px wide board.
    readonly property real unit: width / 234
    readonly property real bankWidth: 12 * unit
    readonly property real travel: width - 2 * bankWidth - 2 * ferry.width

    implicitHeight: labels.height + water.y + water.height

    Item {
        id: labels

        width: parent.width
        height: fromLabel.text.length > 0 || toLabel.text.length > 0 ? fromLabel.height : 0
        visible: height > 0

        Label {
            id: fromLabel
            x: Theme.paddingSmall
            width: parent.width / 2 - Theme.paddingSmall
            truncationMode: TruncationMode.Fade
            font.pixelSize: Theme.fontSizeTiny
            color: Theme.secondaryColor
            text: scene.fromName
        }
        Label {
            id: toLabel
            anchors.right: parent.right
            anchors.rightMargin: Theme.paddingSmall
            width: parent.width / 2 - Theme.paddingSmall
            horizontalAlignment: Text.AlignRight
            truncationMode: TruncationMode.Fade
            font.pixelSize: Theme.fontSizeTiny
            color: Theme.secondaryColor
            text: scene.toName
        }
    }

    Item {
        id: river

        anchors.top: labels.bottom
        width: parent.width
        height: 84 * scene.unit

        Rectangle {
            width: scene.bankWidth
            y: 34 * scene.unit
            height: parent.height - y
            color: Theme.primaryColor
            opacity: 0.25
        }
        Rectangle {
            anchors.right: parent.right
            width: scene.bankWidth
            y: 34 * scene.unit
            height: parent.height - y
            color: Theme.primaryColor
            opacity: 0.25
        }
        // The cable the ferry runs along.
        Rectangle {
            x: scene.bankWidth
            y: 30 * scene.unit
            width: parent.width - 2 * scene.bankWidth
            height: Math.max(1, Math.round(scene.unit))
            color: Theme.primaryColor
            opacity: 0.25
        }

        Item {
            id: ferry

            objectName: "ferry"
            width: 44 * scene.unit
            height: 27 * scene.unit
            x: scene.bankWidth + 6 * scene.unit + Math.max(0, Math.min(1, scene.progress)) * scene.travel
            y: 30 * scene.unit

            Behavior on x {
                NumberAnimation { duration: 400; easing.type: Easing.InOutQuad }
            }

            // The wake behind a moving ferry.
            Rectangle {
                visible: scene.moving && !scene.waiting
                x: -30 * scene.unit
                y: parent.height - 2 * scene.unit
                width: 20 * scene.unit
                height: Math.max(1, 1.5 * scene.unit)
                radius: height / 2
                color: Theme.secondaryHighlightColor
                opacity: 0.6
            }
            HighlightImage {
                x: 4 * scene.unit
                width: 34 * scene.unit
                height: 16 * scene.unit
                sourceSize.width: width
                sourceSize.height: height
                source: Qt.resolvedUrl("images/ferry-cargo.svg")
                color: Theme.highlightColor
            }
            HighlightImage {
                y: 17 * scene.unit
                width: 44 * scene.unit
                height: 10 * scene.unit
                sourceSize.width: width
                sourceSize.height: height
                source: Qt.resolvedUrl("images/ferry-hull.svg")
                color: Theme.primaryColor
            }
            HighlightImage {
                objectName: "flag"
                visible: scene.waiting
                x: 33 * scene.unit
                y: -16 * scene.unit
                width: 14 * scene.unit
                height: 16 * scene.unit
                sourceSize.width: width
                sourceSize.height: height
                source: Qt.resolvedUrl("images/ferry-flag.svg")
                color: Theme.errorColor
            }
        }

        Column {
            id: water

            x: scene.bankWidth
            y: 59 * scene.unit
            width: parent.width - 2 * scene.bankWidth
            spacing: 2 * scene.unit
            clip: true

            Repeater {
                model: 2

                HighlightImage {
                    width: water.width
                    height: 10 * scene.unit
                    sourceSize.width: 36 * scene.unit
                    sourceSize.height: height
                    fillMode: Image.TileHorizontally
                    source: Qt.resolvedUrl(scene.waiting || !scene.moving ? "images/waves-calm.svg" : "images/waves.svg")
                    color: Theme.secondaryHighlightColor
                    opacity: index === 0 ? 0.55 : 0.28
                }
            }
        }
    }
}
