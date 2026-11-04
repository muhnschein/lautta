// SPDX-License-Identifier: LGPL-2.1-or-later
.pragma library

var SEPARATOR = " › "

// Location first, then as many of the nearest folders as fit; the middle folds into "…".
// Down to location and parent, which then fades.
function fold(names, fits) {
    var full = names.join(SEPARATOR)
    if (names.length <= 2 || fits(full))
        return full
    for (var keep = names.length - 2; keep > 1; --keep) {
        var folded = [names[0], "…"].concat(names.slice(names.length - keep)).join(SEPARATOR)
        if (fits(folded))
            return folded
    }
    return [names[0], "…", names[names.length - 1]].join(SEPARATOR)
}

// Down columns, then across: [[a, b, c], [d, e]].
function columns(items, count) {
    var per = Math.ceil(items.length / Math.max(1, count))
    var out = []
    for (var i = 0; i < items.length; i += per)
        out.push(items.slice(i, i + per))
    return out
}
