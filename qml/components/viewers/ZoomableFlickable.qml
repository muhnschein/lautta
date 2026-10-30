// SPDX-License-Identifier: LGPL-2.1-or-later Silica's ZoomableFlickable is private (not a Harbour
// import); behaviour written anew, rules in ZoomMath.js. Fitted: flickable takes no drags, so
// swipes reach the views around. Taps near edges left to swipes.
import QtQuick 2.6
import Sailfish.Silica 1.0
import "ZoomMath.js" as ZoomMath

SilicaFlickable {
    id: flickable

    property real zoom: 1
    property bool zoomed
    property real maximumZoom: 2.5
    readonly property real fittedZoom: ZoomMath.fitted(implicitContentWidth, implicitContentHeight,
                                                      width, height, maximumZoom)
    property real minimumZoom: fittedZoom
    property bool zoomEnabled: true
    property bool viewMoving
    property alias implicitContentWidth: content.implicitWidth
    property alias implicitContentHeight: content.implicitHeight
    property alias scrollDecoratorColor: decorator.color
    default property alias contentData: content.data

    signal aboutToZoom
    signal zoomFinished
    signal clicked

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

    function zoomIn(factor, center, previous) {
        aboutToZoom()
        var next = ZoomMath.pinch(view(), factor, center, previous)
        zoom = next.zoom
        contentX = next.contentX
        contentY = next.contentY
        zoomed = next.zoomed
    }

    function toggleZoom(atX, atY) {
        if (zoomed) {
            zoomOut()
        } else if (zoomEnabled && minimumZoom < maximumZoom) {
            aboutToZoom()
            var next = ZoomMath.doubleTap(view(), atX, atY)
            animate(next.zoom, next.contentX, next.contentY, true)
        }
    }

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

    // No press delay: it would keep touches from pinch area.
    pressDelay: 0
    contentWidth: Math.max(width, content.width)
    contentHeight: Math.max(height, content.height)
    flickableDirection: Flickable.HorizontalAndVerticalFlick
    interactive: zoomed
    enabled: !animation.running

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

        MouseArea {
            id: tapArea

            objectName: "tapArea"
            x: flickable.contentX + Theme.paddingLarge
            y: flickable.contentY + Theme.paddingLarge
            width: flickable.width - 2 * Theme.paddingLarge
            height: flickable.height - 2 * Theme.paddingLarge
            // Zoom on finger lift: zooming disables flickable, and mid-press it leaves area taking
            // next tap for this one's end (Qt 5.6).
            property bool doubleTapped
            property real tappedX
            property real tappedY

            onClicked: tapTimer.restart()
            onDoubleClicked: {
                tapTimer.stop()
                tappedX = x + mouse.x - flickable.contentX
                tappedY = y + mouse.y - flickable.contentY
                doubleTapped = true
            }
            onReleased: {
                if (doubleTapped) {
                    doubleTapped = false
                    flickable.toggleZoom(tappedX, tappedY)
                }
            }
            onCanceled: doubleTapped = false
        }
    }

    Timer {
        id: tapTimer
        interval: Qt.styleHints.mouseDoubleClickInterval
        onTriggered: flickable.clicked()
    }

    SequentialAnimation {
        id: animation

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
