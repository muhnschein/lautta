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
    property real fingerX

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

    allowedOrientations: Orientation.All
    canAccept: picture.length > 0

    Component.onCompleted: load()
    onAccepted: {
        if (JSON.stringify(corners) !== startCorners)
            scan.setCorners(index, JSON.stringify(corners))
        if (turns !== startTurns)
            scan.turn(index, turns - startTurns)
    }

    Connections {
        target: dialog.scan
        onEdgesSearched: {
            if (index !== dialog.index)
                return
            if (corners.length > 0) {
                dialog.corners = JSON.parse(corners)
                return
            }
            //% "No page edges found"
            notice.text = qsTrId("lautta-scan-no-edges")
            notice.show()
        }
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
                        dialog.fingerX = mapToItem(stage, mouse.x, mouse.y).x
                        dialog.grabbed = index
                    }
                    onReleased: dialog.grabbed = -1
                    onCanceled: dialog.grabbed = -1
                    onPositionChanged: {
                        var at = mapToItem(frame, mouse.x - grabX, mouse.y - grabY)
                        dialog.fingerX = mapToItem(stage, mouse.x, mouse.y).x
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

        // Held corner enlarged, on the side away from the finger.
        Rectangle {
            id: loupe

            objectName: "loupe"
            visible: dialog.grabbed >= 0
            width: Theme.itemSizeHuge
            height: Theme.itemSizeHuge
            x: dialog.fingerX > stage.width / 2 ? 0 : stage.width - width
            color: "black"
            border {
                width: Math.max(2, Math.round(Theme.paddingSmall / 2))
                color: Theme.highlightColor
            }
            clip: true

            Item {
                anchors.centerIn: parent
                rotation: dialog.turns * 90

                Image {
                    width: frame.width * 2
                    height: frame.height * 2
                    x: dialog.grabbed >= 0 ? -dialog.corners[2 * dialog.grabbed] * width : 0
                    y: dialog.grabbed >= 0 ? -dialog.corners[2 * dialog.grabbed + 1] * height : 0
                    source: loupe.visible ? photo.source : ""
                    cache: false
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
    }

    Row {
        id: tools

        anchors {
            horizontalCenter: parent.horizontalCenter
            bottom: parent.bottom
            bottomMargin: Theme.paddingLarge
        }
        spacing: Theme.paddingLarge

        IconButton {
            objectName: "rotateLeftButton"
            icon.source: "image://theme/icon-m-rotate-left"
            onClicked: dialog.turn(-1)
        }

        IconButton {
            objectName: "rotateRightButton"
            icon.source: "image://theme/icon-m-rotate-right"
            onClicked: dialog.turn(1)
        }

        IconButton {
            objectName: "findEdgesButton"
            icon.source: "image://theme/icon-m-crop"
            enabled: dialog.scan !== null && !dialog.scan.busy
            onClicked: dialog.scan.findEdges(dialog.index)
        }

        IconButton {
            objectName: "wholePictureButton"
            icon.source: "image://theme/icon-m-image"
            onClicked: dialog.corners = ScanMath.whole
        }
    }

    BusyIndicator {
        anchors.centerIn: stage
        size: BusyIndicatorSize.Large
        running: dialog.scan !== null && dialog.scan.busy
    }

    Notice { id: notice }
}
