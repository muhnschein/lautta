// SPDX-License-Identifier: LGPL-2.1-or-later
.pragma library

// Name found by composing the line with a mark in its place.
function around(compose, name) {
    var mark = "\u0001"
    var marked = compose(mark)
    var at = marked.indexOf(mark)
    if (at < 0 || !name)
        return { "before": "", "rest": compose(name || ""), "named": false }
    return { "before": marked.substring(0, at), "rest": name + marked.substring(at + mark.length), "named": true }
}
