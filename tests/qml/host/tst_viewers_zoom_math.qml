// SPDX-License-Identifier: LGPL-2.1-or-later
// Host test of the image viewer's zoom rules (PRV-4, ZoomMath.js, the
// behaviour of Silica's ZoomableFlickable that Jolla Gallery zooms with):
// fitting, the Gallery's deepest zoom, pinch steps that follow the fingers
// and keep the point under them, the minimum, and the double tap.
import QtQuick 2.6
import "../../../qml/components/viewers/ZoomMath.js" as ZoomMath

Item {
    function check(cond, what) {
        if (!cond)
            console.error("FAILED: " + what)
    }

    function near(a, b) {
        return Math.abs(a - b) < 0.01
    }

    // A 2340 x 1755 decode of a 4:3 photo in a portrait 1080 x 2340 view,
    // fitted (zoom 1080 / 2340) or zoomed to `zoom`; the content is the
    // picture once larger than the view, centred in the view otherwise.
    function view(zoom, contentX, contentY, zoomed) {
        var picWidth = 2340 * zoom
        var picHeight = 1755 * zoom
        return {
            "zoom": zoom, "zoomed": zoomed === true, "contentX": contentX, "contentY": contentY,
            "contentWidth": Math.max(1080, picWidth), "contentHeight": Math.max(2340, picHeight),
            "width": 1080, "height": 2340, "implicitWidth": 2340, "implicitHeight": 1755,
            "minimum": 1080 / 2340, "maximum": 11.7
        }
    }

    function after(from, next) {
        return view(next.zoom, next.contentX, next.contentY, next.zoomed)
    }

    // Where in the picture (0..1 on each axis) the view point (atX, atY) is.
    function placeAt(v, atX, atY) {
        var picWidth = v.implicitWidth * v.zoom
        var picHeight = v.implicitHeight * v.zoom
        return {
            "x": (v.contentX + atX - Math.max(0, (v.contentWidth - picWidth) / 2)) / picWidth,
            "y": (v.contentY + atY - Math.max(0, (v.contentHeight - picHeight) / 2)) / picHeight
        }
    }

    // The point under (atX, atY) stays put; across only (`acrossOnly`)
    // while the picture is shorter than the view, which keeps it centred.
    function keeps(before, now, atX, atY, what, acrossOnly) {
        var a = placeAt(before, atX, atY)
        var b = placeAt(now, atX, atY)
        check(near(a.x, b.x) && (acrossOnly || near(a.y, b.y)),
              what + ": point moved from " + JSON.stringify(a) + " to " + JSON.stringify(b))
    }

    Component.onCompleted: {
        // Fitting shows all of the picture, never above the maximum.
        check(near(ZoomMath.fitted(2340, 1755, 1080, 2340, 11.7), 1080 / 2340), "landscape photo, portrait view")
        check(near(ZoomMath.fitted(1755, 2340, 2340, 1080, 11.7), 1080 / 2340), "portrait photo, landscape view")
        check(ZoomMath.fitted(100, 50, 1080, 2340, 2.5) === 2.5, "a small picture fits at the maximum at most")
        check(ZoomMath.fitted(0, 0, 1080, 2340, 11.7) === 1, "no picture yet: 1")

        // The Gallery's deepest zoom: long screen side / 200, more with a sharper copy.
        check(near(ZoomMath.maximum(1080, 2340, 1755, 0), 11.7), "no sharp copy yet")
        check(near(ZoomMath.maximum(2340, 1080, 1755, 3510), 23.4), "a copy twice as tall doubles it")
        check(near(ZoomMath.maximum(1080, 2340, 1755, 1000), 11.7), "a smaller copy does not lower it")
        check(near(ZoomMath.maximum(1080, 2340, 0, 3264), 11.7), "no shown picture yet")

        check(ZoomMath.clamp(-3, 0, 10) === 0 && ZoomMath.clamp(13, 0, 10) === 10 && ZoomMath.clamp(4, 0, 10) === 4, "clamp")
        check(ZoomMath.clamp(5, 0, -1) === 0, "clamp with crossed limits")

        // A pinch out from fitted: the width scales by the factor (portrait
        // view), the point under the fingers stays put.
        var fitted = view(1080 / 2340, 0, 0)
        var centre = { "x": 540, "y": 1170 }
        var first = ZoomMath.pinch(fitted, 1.5, centre, centre)
        check(first.zoomed, "a pinch zooms in")
        check(near(first.zoom, 1.5 * 1080 / 2340), "zoom by the width: " + first.zoom)
        keeps(fitted, after(fitted, first), 540, 1170, "pinch at the centre")

        // Further in around another point.
        var zoomed = after(fitted, first)
        var spot = { "x": zoomed.contentX + 300, "y": zoomed.contentY + 900 }
        var second = ZoomMath.pinch(zoomed, 1.4, spot, spot)
        keeps(zoomed, after(zoomed, second), 300, 900, "pinch off-centre", true)
        check(second.zoom > first.zoom, "further in: " + second.zoom)
        check(second.contentY === 0, "shorter than the view: centred, not moved down " + second.contentY)

        // The fingers move 40 px right and 25 down: the content follows them.
        var still = after(zoomed, second)
        var moved = ZoomMath.pinch(still, 1.0, { "x": 640, "y": 1025 }, { "x": 600, "y": 1000 })
        check(near(moved.contentX, still.contentX - 40) && near(moved.contentY, still.contentY - 25),
              "panned with the fingers: " + moved.contentX + "," + moved.contentY)

        // Not past the maximum.
        var deep = ZoomMath.pinch(still, 100, centre, centre)
        check(near(deep.zoom, 11.7), "stops at the maximum: " + deep.zoom)

        // Below the minimum the zoom stops there, nothing moves, and the
        // zoomed state waits for the end of the pinch.
        var under = ZoomMath.pinch(view(0.6, 30, 40, true), 0.2, centre, centre)
        check(near(under.zoom, 1080 / 2340) && under.contentX === 30 && under.contentY === 40 && under.zoomed,
              "back at the minimum: " + JSON.stringify(under))
        check(!ZoomMath.pinch(fitted, 0.9, centre, centre).zoomed, "a pinch in at fitted stays fitted")
        check(!ZoomMath.settled(1080 / 2340, 1080 / 2340) && ZoomMath.settled(0.6, 1080 / 2340), "settled")

        // A landscape view scales by the height.
        var wide = { "zoom": 1, "zoomed": false, "contentX": 0, "contentY": 0, "contentWidth": 2340,
                     "contentHeight": 1080, "width": 2340, "height": 1080, "implicitWidth": 1440,
                     "implicitHeight": 1080, "minimum": 1, "maximum": 11.7 }
        check(near(ZoomMath.pinch(wide, 2, { "x": 1170, "y": 540 }, { "x": 1170, "y": 540 }).zoom, 2),
              "zoom by the height in landscape")

        // A double tap goes three times closer around the tapped spot.
        var tap = ZoomMath.doubleTap(fitted, 810, 1170)
        check(near(tap.zoom, 3 * 1080 / 2340), "double tap zoom: " + tap.zoom)
        keeps(fitted, view(tap.zoom, tap.contentX, tap.contentY, true), 810, 1170, "double tap off-centre")
        var middle = ZoomMath.doubleTap(fitted, 540, 1170)
        check(near(middle.contentX, (3240 - 1080) / 2), "double tap at the centre stays centred: " + middle.contentX)

        // Spots near the corners clamp to the picture's edges.
        var corner = ZoomMath.doubleTap(fitted, 0, 0)
        check(corner.contentX === 0 && corner.contentY === 0, "top-left: the origin " + JSON.stringify(corner))
        var far = ZoomMath.doubleTap(fitted, 1080, 2340)
        check(near(far.contentX, 3240 - 1080) && near(far.contentY, 2430 - 2340),
              "bottom-right: the picture's far edges " + JSON.stringify(far))

        // Never past the maximum.
        var small = view(1080 / 2340, 0, 0)
        small.maximum = 1
        check(ZoomMath.doubleTap(small, 540, 1170).zoom === 1, "double tap stops at the maximum")

        // The point under a finger in a picture with no size yet: the middle.
        check(ZoomMath.spot(100, 1080, 0, 3000, 540, 1080) === 960, "no size yet: middle")
        check(ZoomMath.follow(10, 500, 500, 0, 3000, 1080) === 10, "no old size: no growth")
    }
}
