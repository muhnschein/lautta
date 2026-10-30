// SPDX-License-Identifier: LGPL-2.1-or-later
.pragma library

function columns(width, tileWidth, least) {
    return tileWidth > 0 ? Math.max(least, Math.round(width / tileWidth)) : least
}

function rowCount(count, columns) {
    return count > 0 && columns > 0 ? Math.ceil(count / columns) : 0
}

function rowOf(index, columns) {
    return columns > 0 ? Math.floor(index / columns) : 0
}

function indexAt(row, x, cellWidth, columns, count) {
    if (row < 0 || x < 0 || cellWidth <= 0)
        return -1
    var column = Math.floor(x / cellWidth)
    var index = row * columns + column
    return column < columns && index < count ? index : -1
}

function tallest(items) {
    var height = 0
    for (var i = 0; i < items.length; ++i)
        height = Math.max(height, items[i].implicitHeight)
    return height
}
