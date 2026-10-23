// SPDX-License-Identifier: LGPL-2.1-or-later
// Pinch zoom for the image viewer (PRV-4) with the feel of Jolla Gallery's
// viewer. The Gallery zooms with Silica's ZoomableFlickable, a private
// Silica type (not a Harbour import, and its drag detector is private C++),
// so this is that behaviour written anew, rules in ZoomMath.js: the content
// fits at first and again after the phone turns, a pinch zooms around the
// fingers and moves with them, zoomOut() animates back to the fitted size.
// lautta keeps its double tap: closer around the tapped spot, or back out.
// Zoomed in, drags pan; fitted, the flickable takes none, so they go to the
// views around it (swipes between images, the page's vertical navigation).
// A tap is `clicked` once the double-tap time has passed without a second
// one; taps near the edges are left to the swipes, as in the Gallery.
import QtQuick 2.6
import Sailfish.Silica 1.0
import "ZoomMath.js" as ZoomMath

SilicaFlickable {
    id: flickable

    // Scale of the content's implicit size; the fitted zoom while not zoomed.
    property real zoom: 1
    // Zoomed in by a pinch or a double tap, until zoomOut() or resetZoom().
    property bool zoomed
    property real maximumZoom: 2.5
    readonly property real fittedZoom: ZoomMath.fitted(implicitContentWidth, implicitContentHeight,
                                                      width, height, maximumZoom)
    property real minimumZoom: fittedZoom
    property bool zoomEnabled: true
    // The views around are moving (a swipe): no pinching meanwhile.
    property bool viewMoving
    property alias implicitContentWidth: content.implicitWidth
    property alias implicitContentHeight: content.implicitHeight
    property alias scrollDecoratorColor: decorator.color
    default property alias contentData: content.data

    signal aboutToZoom
    signal zoomFinished
    signal clicked

    // Fitted again at once (the image was swiped away).
    function resetZoom() {
        animation.stop()
        if (zoomed) {
            zoom = fittedZoom
            zoomed = false
        }
        contentX = 0
        contentY = 0
    }

    function zoomOut() {
        animate(fittedZoom, 0, 0, false)
    }

    // One pinch step, see ZoomMath.pinch().
    function zoomIn(factor, center, previous) {
        aboutToZoom()
        var next = ZoomMath.pinch(view(), factor, center, previous)
        zoom = next.zoom
        contentX = next.contentX
        contentY = next.contentY
        zoomed = next.zoomed
    }

    // Double tap at the view point (atX, atY): closer around it, or back
    // to the fitted size.
    function toggleZoom(atX, atY) {
        if (zoomed) {
            zoomOut()
        } else if (zoomEnabled && minimumZoom < maximumZoom) {
            aboutToZoom()
            var next = ZoomMath.doubleTap(view(), atX, atY)
            animate(next.zoom, next.contentX, next.contentY, true)
        }
    }

    // What ZoomMath works on.
    function view() {
        return {
            "zoom": zoom, "zoomed": zoomed, "contentX": contentX, "contentY": contentY,
            "contentWidth": contentWidth, "contentHeight": contentHeight, "width": width, "height": height,
            "implicitWidth": implicitContentWidth, "implicitHeight": implicitContentHeight,
            "minimum": minimumZoom, "maximum": maximumZoom
        }
    }

    function animate(toZoom, toX, toY, staysZoomed) {
        animation.stop()
        zoomTo.to = toZoom
        panX.to = toX
        panY.to = toY
        animation.staysZoomed = staysZoomed
        zoomed = true
        animation.start()
    }

    // Silica's own: a press delay would keep touches from the pinch area.
    pressDelay: 0
    contentWidth: Math.max(width, content.width)
    contentHeight: Math.max(height, content.height)
    flickableDirection: Flickable.HorizontalAndVerticalFlick
    interactive: zoomed
    enabled: !animation.running

    // The fitted zoom follows the view (the phone turned) while not zoomed.
    Binding {
        target: flickable
        property: "zoom"
        value: flickable.fittedZoom
        when: !flickable.zoomed
    }

    ScrollDecorator {
        id: decorator
        flickable: flickable
    }

    PinchArea {
        id: pinchArea

        parent: flickable.contentItem
        width: flickable.contentWidth
        height: flickable.contentHeight
        // A pinch before the content has a size would go nowhere.
        enabled: flickable.implicitContentWidth > 0 && flickable.implicitContentHeight > 0

        onPinchUpdated: {
            if (flickable.zoomEnabled && !flickable.viewMoving && flickable.minimumZoom < flickable.maximumZoom)
                flickable.zoomIn(1.0 + pinch.scale - pinch.previousScale, pinch.center, pinch.previousCenter)
        }
        onPinchFinished: {
            if (flickable.zoomed && !ZoomMath.settled(flickable.zoom, flickable.minimumZoom))
                flickable.zoomed = false
            flickable.zoomFinished()
            flickable.returnToBounds()
        }

        Item {
            id: content

            anchors.centerIn: parent
            implicitWidth: flickable.width
            implicitHeight: flickable.height
            width: Math.ceil(implicitWidth * flickable.zoom)
            height: Math.ceil(implicitHeight * flickable.zoom)
        }

        // Over the view wherever the content is, less a margin; inside the
        // pinch area, which takes the second finger from it.
        MouseArea {
            id: tapArea

            objectName: "tapArea"
            x: flickable.contentX + Theme.paddingLarge
            y: flickable.contentY + Theme.paddingLarge
            width: flickable.width - 2 * Theme.paddingLarge
            height: flickable.height - 2 * Theme.paddingLarge
            onClicked: tapTimer.restart()
            onDoubleClicked: {
                tapTimer.stop()
                flickable.toggleZoom(x + mouse.x - flickable.contentX, y + mouse.y - flickable.contentY)
            }
        }
    }

    Timer {
        id: tapTimer
        interval: Qt.styleHints.mouseDoubleClickInterval
        onTriggered: flickable.clicked()
    }

    SequentialAnimation {
        id: animation

        // Zoomed at the end (a double tap in) or fitted (zoomOut()).
        property bool staysZoomed

        ParallelAnimation {
            NumberAnimation {
                id: zoomTo
                target: flickable
                property: "zoom"
                easing.type: Easing.InOutQuad
                duration: 200
            }
            NumberAnimation {
                id: panX
                target: flickable
                property: "contentX"
                easing.type: Easing.InOutQuad
                duration: 200
            }
            NumberAnimation {
                id: panY
                target: flickable
                property: "contentY"
                easing.type: Easing.InOutQuad
                duration: 200
            }
        }
        ScriptAction {
            script: flickable.zoomed = animation.staysZoomed
        }
    }
}
