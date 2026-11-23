// SPDX-License-Identifier: LGPL-2.1-or-later
.pragma library

function shownWidth(widths, separator, dots, folded) {
    var total = folded > 0 ? dots + separator : 0
    for (var i = folded; i < widths.length; ++i)
        total += widths[i] + separator
    return total
}

function folded(widths, separator, dots, available) {
    for (var count = 0; count < widths.length; ++count) {
        if (shownWidth(widths, separator, dots, count) <= available)
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
    for (var i = folded - 1; i >= 0 && rows.length < most; --i)
        rows.push(i)
    return rows
}
