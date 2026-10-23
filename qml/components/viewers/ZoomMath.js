// SPDX-License-Identifier: LGPL-2.1-or-later
// Zoom rules of ZoomableFlickable (PRV-4): those of Silica's own
// ZoomableFlickable, which Jolla Gallery's viewer zooms with, written anew
// because that one is a private Silica type (not a Harbour import). The
// zoom is the scale of the content's implicit size; the fitted zoom shows
// all of it. Pinching scales by the width in a portrait view and by the
// height in a landscape one, moves the content with the fingers and keeps
// the point under them in place.
.pragma library

// A double tap at the fitted size goes this many times closer (the zoom
// lautta's viewer always had; the Gallery has no double tap).
var doubleTapFactor = 3

// `value` kept between `low` and `high` (`low` when they cross).
function clamp(value, low, high) {
    return Math.max(low, Math.min(high, value))
}

// The zoom that shows all of content implicitWidth x implicitHeight in a
// view of viewWidth x viewHeight, `maximum` at most; 1 until the content
// has a size.
function fitted(implicitWidth, implicitHeight, viewWidth, viewHeight, maximum) {
    if (implicitWidth <= 0 || implicitHeight <= 0)
        return 1
    return Math.min(maximum, Math.min(viewWidth / implicitWidth, viewHeight / implicitHeight))
}

// The Gallery's deepest zoom: the screen's long side over 200, times how
// much taller the sharp copy decoded for zooming is than the shown one.
function maximum(screenWidth, screenHeight, shownHeight, sharpHeight) {
    var gain = shownHeight > 0 ? Math.max(1, sharpHeight / shownHeight) : 1
    return Math.max(screenWidth, screenHeight) / 200 * gain
}

// One axis of a pinch step: the content follows the fingers from
// `previous` to `center` (content coordinates) and, once larger than the
// view, grows around `previous` so the point under the fingers stays.
function follow(content, previous, center, oldSize, newSize, viewport) {
    var moved = content + previous - center
    if (newSize > viewport && oldSize > 0)
        moved += (newSize - oldSize) * previous / oldSize
    return moved
}

// One pinch update. `view` holds zoom, zoomed, contentX, contentY,
// contentWidth, contentHeight, width, height, implicitWidth,
// implicitHeight, minimum and maximum; `factor` is 1 plus the change of
// the pinch scale since the last update; `center` and `previous` are the
// pinch centre now and then, in content coordinates. Below the minimum the
// zoom stops there and nothing moves; `zoomed` stays as it was until the
// pinch ends (settled()), so the controls do not flicker on and off.
function pinch(view, factor, center, previous) {
    var byWidth = view.width < view.height
    var implicitSide = byWidth ? view.implicitWidth : view.implicitHeight
    var side = (byWidth ? view.contentWidth : view.contentHeight) * factor
    if (implicitSide <= 0 || side <= implicitSide * view.minimum)
        return { "zoom": view.minimum, "zoomed": view.zoomed, "contentX": view.contentX, "contentY": view.contentY }
    var zoom = Math.min(side, implicitSide * view.maximum) / implicitSide
    return {
        "zoom": zoom,
        "zoomed": true,
        "contentX": follow(view.contentX, previous.x, center.x, view.contentWidth,
                           view.implicitWidth * zoom, view.width),
        "contentY": follow(view.contentY, previous.y, center.y, view.contentHeight,
                           view.implicitHeight * zoom, view.height)
    }
}

// Whether a pinch that ended at `zoom` leaves the content zoomed in: back
// at the fitted size it is not, and swipes and page navigation return.
function settled(zoom, minimum) {
    return zoom > minimum
}

// One axis of a double tap: where the content starts so that the spot at
// `point` (content coordinates) of a picture `size` long, centred in
// content `contentSize` long, stays at `at` in the view once the picture
// is `next` long.
function spot(point, contentSize, size, next, at, viewport) {
    var inset = Math.max(0, (contentSize - size) / 2)
    var place = size > 0 ? clamp((point - inset) / size, 0, 1) : 0.5
    return clamp(place * next - at, 0, Math.max(0, next - viewport))
}

// A double tap at (atX, atY) in the view (`view` as for pinch()): the zoom
// doubleTapFactor times the minimum (the maximum at most) and the content
// position that keeps the tapped spot under the finger.
function doubleTap(view, atX, atY) {
    var zoom = Math.min(view.maximum, view.minimum * doubleTapFactor)
    return {
        "zoom": zoom,
        "contentX": spot(view.contentX + atX, view.contentWidth, view.implicitWidth * view.zoom,
                         view.implicitWidth * zoom, atX, view.width),
        "contentY": spot(view.contentY + atY, view.contentHeight, view.implicitHeight * view.zoom,
                         view.implicitHeight * zoom, atY, view.height)
    }
}
