// SPDX-License-Identifier: LGPL-2.1-or-later Corners and turn of one scanned page. Works on its
// own copy: page changes on Done only.
import QtQuick 2.6
import Sailfish.Silica 1.0
import Lautta 1.0
import "../components/scan"
import "../components/scan/ScanMath.js" as ScanMath

Dialog {
    id: dialog

    property QtObject scan
    property int index
    property string picture
    // As ScanModel's `corners`, of the unturned photo.
    property var corners: ScanMath.whole
    property int turns
    property int startTurns
    property string startCorners
    // Corner under the finger, -1 none.
    property int grabbed: -1
    // Finger on the dialog.
    property real fingerX
    property real fingerY

    function load() {
        var info = scan ? JSON.parse(scan.pageJson(index)) : null
        if (!info)
            return
        picture = info.picture
        corners = info.quad
        startCorners = JSON.stringify(info.quad)
        turns = info.turns
        startTurns = info.turns
    }

    function turn(by) {
        turns = ScanMath.turned(turns, by)
    }

    function follow(item, x, y) {
        var at = item.mapToItem(dialog, x, y)
        fingerX = at.x
        fingerY = at.y
    }

    allowedOrientations: Orientation.All
    canAccept: picture.length > 0

    Component.onCompleted: load()
    onAccepted: {
        if (JSON.stringify(corners) !== startCorners)
            scan.setCorners(index, JSON.stringify(corners))
        if (turns !== startTurns)
            scan.turn(index, turns - startTurns)
    }

    DialogHeader {
        id: header

        //% "Done"
        acceptText: qsTrId("lautta-scan-adjust-done")
    }

    Item {
        id: stage

        anchors {
            top: header.bottom
            bottom: tools.top
            left: parent.left
            right: parent.right
            margins: Theme.paddingLarge
        }

        // Photo's own frame, turned as a whole: corners stay those of the unturned photo.
        Item {
            id: frame

            readonly property var size: ScanMath.fit(stage.width, stage.height, photo.implicitWidth,
                                                     photo.implicitHeight, dialog.turns)

            objectName: "frame"
            anchors.centerIn: parent
            width: size.width
            height: size.height
            rotation: dialog.turns * 90

            Image {
                id: photo

                anchors.fill: parent
                source: dialog.picture.length > 0 ? "file://" + dialog.picture : ""
                cache: false
            }

            QuadOutline {
                anchors.fill: parent
                corners: dialog.corners
            }

            Repeater {
                model: 4

                MouseArea {
                    id: handle

                    property real grabX
                    property real grabY

                    objectName: "handle" + index
                    width: Theme.itemSizeMedium
                    height: Theme.itemSizeMedium
                    x: dialog.corners[2 * index] * frame.width - width / 2
                    y: dialog.corners[2 * index + 1] * frame.height - height / 2
                    // Page swipes and Dialog's accept gesture stay away from a held corner.
                    preventStealing: true
                    onPressed: {
                        grabX = mouse.x - width / 2
                        grabY = mouse.y - height / 2
                        dialog.follow(handle, mouse.x, mouse.y)
                        dialog.grabbed = index
                    }
                    onReleased: dialog.grabbed = -1
                    onCanceled: dialog.grabbed = -1
                    onPositionChanged: {
                        var at = mapToItem(frame, mouse.x - grabX, mouse.y - grabY)
                        dialog.follow(handle, mouse.x, mouse.y)
                        dialog.corners = ScanMath.moved(dialog.corners, index, at.x / frame.width, at.y / frame.height)
                    }

                    Rectangle {
                        anchors.centerIn: parent
                        width: Theme.iconSizeSmall
                        height: Theme.iconSizeSmall
                        radius: width / 2
                        color: handle.pressed ? Theme.highlightColor : Theme.primaryColor
                        border {
                            width: Math.max(2, Math.round(Theme.paddingSmall / 2))
                            color: Theme.highlightColor
                        }
                    }
                }
            }
        }
    }

    // Held corner enlarged, round, above the finger so the thumb leaves it in sight.
    Item {
        id: loupe

        readonly property real size: Math.min(Theme.itemSizeHuge * 2, Math.min(dialog.width, dialog.height) / 2)
        readonly property var at: ScanMath.loupeAt(dialog.fingerX, dialog.fingerY, size, Theme.itemSizeLarge,
                                                   dialog.width, dialog.height)

        objectName: "loupe"
        visible: dialog.grabbed >= 0
        x: at.x
        y: at.y
        width: size
        height: size

        Rectangle {
            id: lens

            anchors.fill: parent
            color: "black"

            Item {
                anchors.centerIn: parent
                rotation: dialog.turns * 90

                Image {
                    objectName: "loupePhoto"
                    width: frame.width * 2
                    height: frame.height * 2
                    x: dialog.grabbed >= 0 ? -dialog.corners[2 * dialog.grabbed] * width : 0
                    y: dialog.grabbed >= 0 ? -dialog.corners[2 * dialog.grabbed + 1] * height : 0
                    source: loupe.visible ? photo.source : ""
                    cache: false
                }
            }
        }

        // Cuts the lens round; Qt 5.6 clips to rectangles only.
        ShaderEffect {
            anchors.fill: parent
            property variant source: ShaderEffectSource {
                sourceItem: lens
                hideSource: true
            }
            property real edge: 1.0 / Math.max(1, loupe.size)
            fragmentShader: "
                varying highp vec2 qt_TexCoord0;
                uniform sampler2D source;
                uniform lowp float qt_Opacity;
                uniform highp float edge;
                void main() {
                    highp float d = distance(qt_TexCoord0, vec2(0.5));
                    gl_FragColor = texture2D(source, qt_TexCoord0) * (1.0 - smoothstep(0.5 - 2.0 * edge, 0.5, d)) * qt_Opacity;
                }"
        }

        Rectangle {
            objectName: "loupeRing"
            anchors.fill: parent
            radius: width / 2
            color: "transparent"
            border {
                width: Math.max(2, Math.round(Theme.paddingSmall / 2))
                color: Theme.highlightColor
            }
        }

        Rectangle {
            anchors.centerIn: parent
            width: Theme.paddingLarge
            height: 1
            color: Theme.highlightColor
        }

        Rectangle {
            anchors.centerIn: parent
            width: 1
            height: Theme.paddingLarge
            color: Theme.highlightColor
        }
    }

    Row {
        id: tools

        anchors {
            horizontalCenter: parent.horizontalCenter
            bottom: parent.bottom
            bottomMargin: Theme.paddingLarge
        }
        spacing: Theme.paddingLarge * 2

        // The app's own icons: the theme's rotate arrows read as undo and redo.
        IconButton {
            objectName: "rotateLeftButton"
            icon.source: Qt.resolvedUrl("../icons/rotate-left.svg")
            icon.sourceSize: Qt.size(Theme.iconSizeMedium, Theme.iconSizeMedium)
            onClicked: dialog.turn(-1)
        }

        IconButton {
            objectName: "rotateRightButton"
            icon.source: Qt.resolvedUrl("../icons/rotate-right.svg")
            icon.sourceSize: Qt.size(Theme.iconSizeMedium, Theme.iconSizeMedium)
            onClicked: dialog.turn(1)
        }
    }

    BusyIndicator {
        anchors.centerIn: stage
        size: BusyIndicatorSize.Large
        running: dialog.scan !== null && dialog.scan.busy
    }
}
