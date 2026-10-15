// SPDX-License-Identifier: LGPL-2.1-or-later
// Host test of the image viewer's zoom maths (PRV-4, Zoom.js): fitting,
// limits, and that zooming keeps the point under the fingers in place.
import QtQuick 2.6
import "../../../qml/components/viewers/Zoom.js" as Zoom

Item {
    function check(cond, what) {
        if (!cond)
            console.error("FAILED: " + what)
    }

    function near(a, b) {
        return Math.abs(a - b) < 0.001
    }

    // Where in the picture (0..1 on each axis) the view point (atX, atY) is.
    function placeAt(view, atX, atY) {
        var wide = view.fittedWidth * view.zoom
        var high = view.fittedHeight * view.zoom
        return {
            "x": (view.contentX + atX - Zoom.inset(wide, view.width)) / wide,
            "y": (view.contentY + atY - Zoom.inset(high, view.height)) / high
        }
    }

    function view(zoom, contentX, contentY) {
        // A 4:3 landscape photo fitted into a portrait phone screen.
        return { "zoom": zoom, "contentX": contentX, "contentY": contentY,
                 "fittedWidth": 1080, "fittedHeight": 810,
                 "width": 1080, "height": 2000, "maximum": Zoom.maximum }
    }

    function zoomed(from, target, atX, atY) {
        var next = Zoom.zoomAt(from, target, atX, atY)
        var after = view(next.zoom, next.contentX, next.contentY)
        return after
    }

    function keepsPoint(from, target, atX, atY, what) {
        var after = zoomed(from, target, atX, atY)
        var before = placeAt(from, atX, atY)
        var now = placeAt(after, atX, atY)
        check(near(before.x, now.x) && near(before.y, now.y),
              what + ": point moved from " + JSON.stringify(before) + " to " + JSON.stringify(now))
        return after
    }

    Component.onCompleted: {
        // Fitting keeps the shape and fills one side.
        var landscape = Zoom.fitted(4000, 3000, 1080, 2000)
        check(near(landscape.width, 1080) && near(landscape.height, 810), "landscape " + JSON.stringify(landscape))
        var portrait = Zoom.fitted(3000, 4000, 1080, 1000)
        check(near(portrait.width, 750) && near(portrait.height, 1000), "portrait " + JSON.stringify(portrait))
        var none = Zoom.fitted(0, 0, 1080, 2000)
        check(none.width === 1080 && none.height === 2000, "no picture yet " + JSON.stringify(none))
        var hidden = Zoom.fitted(4000, 3000, 0, 2000)
        check(hidden.width === 0 && hidden.height === 2000, "no view yet " + JSON.stringify(hidden))

        // Limits.
        check(Zoom.clampZoom(0.4, 5) === 1, "not below fitted")
        check(Zoom.clampZoom(9, 5) === 5, "not above the maximum")
        check(Zoom.clampZoom(2.5, 5) === 2.5, "in between kept")
        check(Zoom.within(-5, 10) === 0 && Zoom.within(15, 10) === 10 && Zoom.within(4, 10) === 4, "within")
        check(Zoom.within(4, -3) === 0, "within a negative limit")
        check(Zoom.inset(500, 1080) === 290 && Zoom.inset(2000, 1080) === 0, "inset")

        // Double tap in at the centre: the centre stays the centre.
        var centre = keepsPoint(view(1, 0, 0), Zoom.toggled(1), 540, 1000, "double tap at the centre")
        check(centre.zoom === Zoom.doubleTapZoom, "double tap zooms in: " + centre.zoom)
        check(near(centre.contentX, 1080) && near(centre.contentY, 215),
              "centre position " + centre.contentX + "," + centre.contentY)

        // Double tap off-centre: the tapped spot stays under the finger.
        var spot = keepsPoint(view(1, 0, 0), 3, 810, 1050, "double tap off-centre")
        check(spot.contentX > centre.contentX, "zoomed towards the right: " + spot.contentX)

        // Pinch further in around another point, then partly out.
        var further = keepsPoint(spot, 4.2, 300, 1200, "pinch in")
        keepsPoint(further, 3.5, 540, 1000, "pinch out")

        // Points on the edge cannot stay put past the picture's edge.
        var corner = zoomed(view(1, 0, 0), 3, 0, 0)
        check(corner.contentX === 0 && corner.contentY === 0, "top-left corner stays the origin")
        var far = zoomed(view(1, 0, 0), 3, 1080, 2000)
        check(near(far.contentX, 3240 - 1080) && near(far.contentY, 2430 - 2000),
              "bottom-right corner clamps: " + far.contentX + "," + far.contentY)

        // Out again: back to the fitted picture at the origin.
        var out = zoomed(centre, Zoom.toggled(centre.zoom), 200, 300)
        check(out.zoom === 1 && out.contentX === 0 && out.contentY === 0, "double tap out " + JSON.stringify(out))

        // Pinching past the limits stops at them.
        check(Zoom.zoomAt(view(1, 0, 0), 0.2, 540, 1000).zoom === 1, "pinch out stops at fitted")
        check(Zoom.zoomAt(view(4, 0, 0), 12, 540, 1000).zoom === Zoom.maximum, "pinch in stops at the maximum")

        // Before the picture has a size nothing is moved.
        var empty = view(1, 0, 0)
        empty.fittedWidth = 0
        var early = Zoom.zoomAt(empty, 3, 540, 1000)
        check(early.zoom === 3 && early.contentX === 0 && early.contentY === 0, "no size yet " + JSON.stringify(early))

        check(Zoom.toggled(1) === 3 && Zoom.toggled(1.2) === 1 && Zoom.toggled(5) === 1, "toggled")
    }
}
