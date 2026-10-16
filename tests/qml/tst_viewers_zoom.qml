// SPDX-License-Identifier: LGPL-2.1-or-later
// ZoomImage on the target (PRV-4, real Silica): double tap zooms in at the
// tapped point and back out, and the image holds the gestures while zoomed
// so the slideshow does not swipe away.
import QtQuick 2.6
import Sailfish.Silica 1.0
import Lautta 1.0
import "../../qml/components/viewers"

ApplicationWindow {
    id: window

    function check(cond, what) {
        if (!cond)
            console.error("FAILED: " + what)
    }

    ViewerTools { id: tools }

    initialPage: Page {
        ZoomImage {
            id: zoom
            width: 1080
            height: 2000
            source: Qt.resolvedUrl("host/fixtures/photo.jpg")
        }
    }

    Component.onCompleted: {
        tools.pump(500)
        check(zoom.status === Image.Ready, "photo loaded: " + zoom.status)
        check(!zoom.zoomed && !zoom.holdsGestures && !zoom.interactive, "fitted: drags go to the slideshow")
        zoom.toggleZoom(540, 1000)
        check(zoom.zoom === 3 && zoom.zoomed && zoom.holdsGestures && zoom.interactive, "double tap zooms in")
        check(zoom.contentWidth > zoom.width, "zoomed content is wider: " + zoom.contentWidth)
        check(Math.abs(zoom.contentX - (zoom.contentWidth - zoom.width) / 2) < 1,
              "centre stays the centre: " + zoom.contentX + " of " + zoom.contentWidth)
        zoom.toggleZoom(100, 100)
        check(zoom.zoom === 1 && zoom.contentX === 0 && zoom.contentY === 0, "double tap zooms out")
        zoom.zoomAt(2, 540, 1000)
        zoom.source = ""
        check(zoom.zoom === 1, "a new source starts fitted")
    }
}
