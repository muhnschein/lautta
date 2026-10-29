// SPDX-License-Identifier: LGPL-2.1-or-later
// The rows of the grid (SPEC §15.3, BRW-4): how many tiles a row holds,
// which item is under a point and how tall a row is. A row is as tall as its
// tallest tile only, so a row of one-line names with one caption line is
// short and one with a two-line name taller. No Silica: the host test
// checks it.
.pragma library

// Tiles per row in a view `width` wide: as many of `tileWidth` as fit best,
// `least` at the least (portrait's number; landscape keeps the tile size).
function columns(width, tileWidth, least) {
    return tileWidth > 0 ? Math.max(least, Math.round(width / tileWidth)) : least
}

// The rows that `count` items take at `columns` per row.
function rowCount(count, columns) {
    return count > 0 && columns > 0 ? Math.ceil(count / columns) : 0
}

// The row of item `index`.
function rowOf(index, columns) {
    return columns > 0 ? Math.floor(index / columns) : 0
}

// The item at `x` in row `row` of `columns` cells `cellWidth` wide, the
// folder having `count` items: -1 for no row, beside the cells or past the
// last item.
function indexAt(row, x, cellWidth, columns, count) {
    if (row < 0 || x < 0 || cellWidth <= 0)
        return -1
    var column = Math.floor(x / cellWidth)
    var index = row * columns + column
    return column < columns && index < count ? index : -1
}

// The implicit height of the tallest of `items`, the cells of a row: a
// row's height. Read in a binding, it follows every cell's.
function tallest(items) {
    var height = 0
    for (var i = 0; i < items.length; ++i)
        height = Math.max(height, items[i].implicitHeight)
    return height
}
