// SPDX-License-Identifier: LGPL-2.1-or-later
.pragma library

function plan(trails, target) {
    var lowest = 0
    for (var i = 0; i < trails.length; ++i) {
        var at = trails[i].indexOf(target)
        if (at < 0)
            break
        if (at === trails[i].length - 1)
            return { "pop": i }
        lowest = i
    }
    return { "replace": lowest }
}
