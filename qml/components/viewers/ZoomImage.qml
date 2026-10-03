// SPDX-License-Identifier: LGPL-2.1-or-later
// One image with pinch and double-tap zoom (PRV-4). At scale 1 the content
// is not larger than the view, so horizontal drags go to the slideshow that
// holds it; zoomed in, they pan.
import QtQuick 2.6
import Sailfish.Silica 1.0

Flickable {
    id: flick

    property alias source: image.source
    property alias status: image.status
    property alias paintedWidth: image.paintedWidth
    property alias paintedHeight: image.paintedHeight
    // Largest zoom as a multiple of the fitted size.
    property real maxZoom: 5
    readonly property bool zoomed: image.scale > 1.01

    signal tapped()

    function resetZoom() {
        image.scale = 1
        contentX = 0
        contentY = 0
    }

    clip: true
    contentWidth: Math.max(width, width * image.scale)
    contentHeight: Math.max(height, height * image.scale)
    boundsBehavior: Flickable.StopAtBounds
    interactive: zoomed
    onWidthChanged: resetZoom()

    Item {
        width: flick.contentWidth
        height: flick.contentHeight

        Image {
            id: image
            anchors.centerIn: parent
            width: flick.width
            height: flick.height
            fillMode: Image.PreserveAspectFit
            asynchronous: true
            cache: false
            autoTransform: true
            // Large enough for zooming, small enough for the phone's memory.
            sourceSize.width: 4096
            sourceSize.height: 4096
            transformOrigin: Item.Center
        }

        PinchArea {
            anchors.fill: parent
            property real startScale: 1
            onPinchStarted: startScale = image.scale
            onPinchUpdated: image.scale = Math.max(1, Math.min(flick.maxZoom, startScale * pinch.scale))
            onPinchFinished: flick.returnToBounds()

            MouseArea {
                anchors.fill: parent
                onClicked: flick.tapped()
                onDoubleClicked: {
                    if (flick.zoomed)
                        flick.resetZoom()
                    else
                        image.scale = 2.5
                }
            }
        }
    }

    BusyIndicator {
        anchors.centerIn: parent
        size: BusyIndicatorSize.Large
        running: image.status === Image.Loading
    }

    Label {
        anchors.centerIn: parent
        visible: image.status === Image.Error
        color: Theme.secondaryHighlightColor
        //% "Can't show this image"
        text: qsTrId("lautta-viewers-image-error")
    }
}
