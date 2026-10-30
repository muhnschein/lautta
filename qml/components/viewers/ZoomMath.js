// SPDX-License-Identifier: LGPL-2.1-or-later Silica's ZoomableFlickable is private (not a Harbour
// import): rules written anew.
.pragma library

var doubleTapFactor = 3

function clamp(value, low, high) {
    return Math.max(low, Math.min(high, value))
}

function fitted(implicitWidth, implicitHeight, viewWidth, viewHeight, maximum) {
    if (implicitWidth <= 0 || implicitHeight <= 0)
        return 1
    return Math.min(maximum, Math.min(viewWidth / implicitWidth, viewHeight / implicitHeight))
}

function maximum(screenWidth, screenHeight, shownHeight, sharpHeight) {
    var gain = shownHeight > 0 ? Math.max(1, sharpHeight / shownHeight) : 1
    return Math.max(screenWidth, screenHeight) / 200 * gain
}

function follow(content, previous, center, oldSize, newSize, viewport) {
    var moved = content + previous - center
    if (newSize > viewport && oldSize > 0)
        moved += (newSize - oldSize) * previous / oldSize
    return moved
}

// `view`: zoom, zoomed, contentX/Y, contentWidth/Height, width, height, implicitWidth/Height,
// minimum, maximum. Below minimum nothing moves; `zoomed` stays until pinch ends (settled()).
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

function settled(zoom, minimum) {
    return zoom > minimum
}

function spot(point, contentSize, size, next, at, viewport) {
    var inset = Math.max(0, (contentSize - size) / 2)
    var place = size > 0 ? clamp((point - inset) / size, 0, 1) : 0.5
    return clamp(place * next - at, 0, Math.max(0, next - viewport))
}

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
