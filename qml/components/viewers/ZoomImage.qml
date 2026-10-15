// SPDX-License-Identifier: LGPL-2.1-or-later
// One image with pinch and double-tap zoom (PRV-4), after Piirit's picture
// page: the picture is laid out at its fitted size times the zoom, and
// zooming keeps the point under the fingers (or the double tap) in place
// (Zoom.js). At zoom 1 the content is the view, so horizontal drags go to
// the slideshow that holds it; zoomed in, they pan.
import QtQuick 2.6
import Sailfish.Silica 1.0
import "Zoom.js" as Zoom

Flickable {
    id: flick

    property alias source: image.source
    property alias status: image.status
    property alias paintedWidth: image.paintedWidth
    property alias paintedHeight: image.paintedHeight
    // Largest zoom as a multiple of the fitted size.
    property real maxZoom: Zoom.maximum
    // Draw scale against the fitted size; 1 fits.
    property real zoom: 1
    readonly property bool zoomed: zoom > 1
    // Zoomed in or pinching: the gesture is this image's, not the slideshow's.
    readonly property bool holdsGestures: zoomed || pincher.pinch.active
    readonly property var fittedSize: Zoom.fitted(image.implicitWidth, image.implicitHeight, width, height)

    signal tapped()

    // Zoom to `target`, keeping the view point (atX, atY) still.
    function zoomAt(target, atX, atY) {
        var next = Zoom.zoomAt({
            "zoom": zoom, "contentX": contentX, "contentY": contentY,
            "fittedWidth": fittedSize.width, "fittedHeight": fittedSize.height,
            "width": width, "height": height, "maximum": maxZoom
        }, target, atX, atY)
        zoom = next.zoom
        contentX = next.contentX
        contentY = next.contentY
    }

    // Double tap: all the way in at the tapped point, or all the way out.
    function toggleZoom(atX, atY) {
        zoomAt(Zoom.toggled(zoom), atX, atY)
    }

    function resetZoom() {
        zoom = 1
        contentX = 0
        contentY = 0
    }

    clip: true
    contentWidth: Math.max(width, frame.width)
    contentHeight: Math.max(height, frame.height)
    boundsBehavior: Flickable.StopAtBounds
    interactive: zoomed
    onWidthChanged: resetZoom()
    onHeightChanged: resetZoom()
    onSourceChanged: resetZoom()

    PinchArea {
        id: pincher

        // Zoom at finger-down: the pinch scale is relative to its start.
        property real startZoom: 1
        // View point at finger-down: the picture stays still under it.
        property real focusX: 0
        property real focusY: 0

        width: flick.contentWidth
        height: flick.contentHeight

        onPinchStarted: {
            startZoom = flick.zoom
            // Content coordinates; zoomAt takes view coordinates.
            focusX = pinch.startCenter.x - flick.contentX
            focusY = pinch.startCenter.y - flick.contentY
        }
        onPinchUpdated: flick.zoomAt(startZoom * pinch.scale, focusX, focusY)
        onPinchFinished: flick.returnToBounds()

        Item {
            id: frame
            width: flick.fittedSize.width * flick.zoom
            height: flick.fittedSize.height * flick.zoom
            // Centred while smaller than the view.
            x: Zoom.inset(width, flick.width)
            y: Zoom.inset(height, flick.height)

            Image {
                id: image
                anchors.fill: parent
                fillMode: Image.PreserveAspectFit
                asynchronous: true
                cache: false
                autoTransform: true
                // Large enough for zooming, small enough for the phone's memory.
                sourceSize.width: 4096
                sourceSize.height: 4096
            }
        }

        MouseArea {
            id: tap
            anchors.fill: parent
            // A single tap waits out the double-tap time, so a double tap
            // zooms without also hiding the controls.
            onClicked: tapTimer.restart()
            onDoubleClicked: {
                tapTimer.stop()
                var point = mapToItem(flick, mouse.x, mouse.y)
                flick.toggleZoom(point.x, point.y)
            }
        }
    }

    Timer {
        id: tapTimer
        interval: Qt.styleHints.mouseDoubleClickInterval
        onTriggered: flick.tapped()
    }

    BusyIndicator {
        parent: flick
        anchors.centerIn: parent
        size: BusyIndicatorSize.Large
        running: image.status === Image.Loading
    }

    Label {
        parent: flick
        anchors.centerIn: parent
        visible: image.status === Image.Error
        color: Theme.secondaryHighlightColor
        //% "Can't show this image"
        text: qsTrId("lautta-viewers-image-error")
    }
}
