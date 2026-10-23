// SPDX-License-Identifier: LGPL-2.1-or-later
// Lines that name a location (§15.5), split around the name for
// PlaceLabel, which marks a server's name with its icon. No Silica here, so
// that the host tests can check it.
.pragma library

// The line compose(name) makes, split around the name wherever its
// translation puts it: `before` is the text before the name, `rest` the
// name and what follows it, `named` whether the line names it at all. The
// name is found by composing the line with a mark in its place.
function around(compose, name) {
    var mark = "\u0001"
    var marked = compose(mark)
    var at = marked.indexOf(mark)
    if (at < 0 || !name)
        return { "before": "", "rest": compose(name || ""), "named": false }
    return { "before": marked.substring(0, at), "rest": name + marked.substring(at + mark.length), "named": true }
}
