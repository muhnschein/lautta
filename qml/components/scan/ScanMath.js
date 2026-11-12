// SPDX-License-Identifier: LGPL-2.1-or-later Corners: flat [x0, y0, … x3, y3], 0..1 of the
// photo, from top left clockwise, as ScanModel takes them.
.pragma library

var whole = [0, 0, 1, 0, 1, 1, 0, 1]

// As lautta_core::scan::Quad::is_page.
var minArea = 0.0004

function isPage(c) {
    if (!c || c.length !== 8)
        return false
    var area = 0
    for (var i = 0; i < 4; ++i) {
        var a = 2 * i, b = 2 * ((i + 1) % 4), n = 2 * ((i + 2) % 4)
        if ((c[b] - c[a]) * (c[n + 1] - c[b + 1]) - (c[b + 1] - c[a + 1]) * (c[n] - c[b]) <= 0)
            return false
        area += c[a] * c[b + 1] - c[b] * c[a + 1]
    }
    return area / 2 >= minArea
}

// Corner `index` put at x, y (kept inside the photo); corners unchanged where that makes no page.
function moved(corners, index, x, y) {
    var next = corners.slice()
    next[2 * index] = Math.max(0, Math.min(1, x))
    next[2 * index + 1] = Math.max(0, Math.min(1, y))
    return isPage(next) ? next : corners
}

function turned(quarters, by) {
    return (((quarters + by) % 4) + 4) % 4
}

// Size at which a picture, turned by quarter turns, fills most of a box.
function fit(boxWidth, boxHeight, pictureWidth, pictureHeight, quarters) {
    if (!(pictureWidth > 0) || !(pictureHeight > 0) || !(boxWidth > 0) || !(boxHeight > 0))
        return { "width": 0, "height": 0 }
    var across = quarters % 2 !== 0
    var scale = Math.min(boxWidth / (across ? pictureHeight : pictureWidth),
                         boxHeight / (across ? pictureWidth : pictureHeight))
    return { "width": pictureWidth * scale, "height": pictureHeight * scale }
}

// Line from corner `index` to the next, for a Rectangle turned around its left end.
function edge(corners, index, width, height) {
    var a = 2 * index, b = 2 * ((index + 1) % 4)
    var dx = (corners[b] - corners[a]) * width
    var dy = (corners[b + 1] - corners[a + 1]) * height
    return {
        "x": corners[a] * width,
        "y": corners[a + 1] * height,
        "length": Math.sqrt(dx * dx + dy * dy),
        "angle": Math.atan2(dy, dx) * 180 / Math.PI
    }
}

// Page under a point of the grid, -1 outside the pages.
function cellAt(x, y, cellWidth, cellHeight, columns, count) {
    if (x < 0 || y < 0 || !(cellWidth > 0) || !(cellHeight > 0))
        return -1
    var column = Math.floor(x / cellWidth)
    var index = Math.floor(y / cellHeight) * columns + column
    return column < columns && index < count ? index : -1
}
