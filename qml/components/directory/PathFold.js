// SPDX-License-Identifier: LGPL-2.1-or-later
.pragma library

function tailWidth(widths, separator, from) {
    var total = 0
    for (var i = from; i < widths.length; ++i)
        total += widths[i] + separator
    return total
}

function homeFits(widths, separator, home, room) {
    return home + separator + tailWidth(widths, separator, 0) <= room
}

function folded(widths, separator, room) {
    for (var count = 0; count < widths.length; ++count) {
        if (tailWidth(widths, separator, count) <= room)
            return count
    }
    return widths.length
}

function offsets(widths, separator, folded) {
    var out = []
    var x = 0
    for (var i = 0; i < widths.length; ++i) {
        out.push(x)
        if (i >= folded)
            x += widths[i] + separator
    }
    return out
}

function menuRows(folded, most) {
    var rows = []
    for (var i = folded - 1; i >= 0 && rows.length < most - 1; --i)
        rows.push(i)
    if (most > 0)
        rows.push(-1)
    return rows
}
