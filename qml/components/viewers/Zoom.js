// SPDX-License-Identifier: LGPL-2.1-or-later
// Zoom maths of ZoomImage (PRV-4), after Piirit's picture page: the picture
// is laid out at its fitted size times the zoom, so the flickable's content
// is the picture itself, and zooming moves the content so the point under
// the fingers (or the double tap) stays where it is.
.pragma library

// Largest zoom, as a multiple of the fitted size.
var maximum = 5
// Where a double tap at the fitted size goes.
var doubleTapZoom = 3

// `value` kept between 0 and `limit` (0 when the limit is negative).
function within(value, limit) {
    return Math.max(0, Math.min(limit, value))
}

// Offset that centres `size` in `viewport`; 0 once it is larger.
function inset(size, viewport) {
    return Math.max(0, (viewport - size) / 2)
}

// The zoom kept between 1 (fitted) and `largest`.
function clampZoom(target, largest) {
    return Math.max(1, Math.min(largest, target))
}

// Size of a picture of imageWidth x imageHeight fitted into the view,
// keeping its shape. The view's own size until the picture has one.
function fitted(imageWidth, imageHeight, viewWidth, viewHeight) {
    if (imageWidth <= 0 || imageHeight <= 0 || viewWidth <= 0 || viewHeight <= 0)
        return { "width": Math.max(0, viewWidth), "height": Math.max(0, viewHeight) }
    var width = Math.min(viewWidth, viewHeight * imageWidth / imageHeight)
    return { "width": width, "height": width * imageHeight / imageWidth }
}

// One axis of zoomAt: the new content position that keeps the view point
// `at` over the same place of the picture.
function position(fittedSize, zoom, next, content, at, viewport) {
    var before = fittedSize * zoom
    var after = fittedSize * next
    // The point's place in the picture, 0..1.
    var place = (content + at - inset(before, viewport)) / before
    return within(inset(after, viewport) + place * after - at, Math.max(after, viewport) - viewport)
}

// Zooming `view` to `target` around the view point (atX, atY). `view` holds
// zoom, contentX, contentY, fittedWidth, fittedHeight, width, height and
// maximum; the result holds the new zoom, contentX and contentY.
function zoomAt(view, target, atX, atY) {
    var next = clampZoom(target, view.maximum)
    if (view.fittedWidth <= 0 || view.fittedHeight <= 0 || view.zoom <= 0)
        return { "zoom": next, "contentX": 0, "contentY": 0 }
    return {
        "zoom": next,
        "contentX": position(view.fittedWidth, view.zoom, next, view.contentX, atX, view.width),
        "contentY": position(view.fittedHeight, view.zoom, next, view.contentY, atY, view.height)
    }
}

// Double tap: all the way in at the tapped point, or back to the fitted size.
function toggled(zoom) {
    return zoom > 1 ? 1 : doubleTapZoom
}
