// SPDX-License-Identifier: LGPL-2.1-or-later One Text item of a megabyte is slow to lay out and
// draw: a line over `rowLength` characters goes on over further rows.
.pragma library

var tabWidth = 4
var rowLength = 1000

var empty = { "rows": [], "lines": 0 }

function expandTabs(line) {
    var parts = line.split("\t")
    if (parts.length === 1)
        return line
    var pieces = [parts[0]]
    var column = parts[0].length
    for (var i = 1; i < parts.length; ++i) {
        var pad = tabWidth - column % tabWidth
        pieces.push(new Array(pad + 1).join(" "), parts[i])
        column += pad + parts[i].length
    }
    return pieces.join("")
}

function rowEnd(line, end) {
    if (end >= line.length)
        return line.length
    var c = line.charCodeAt(end)
    return c >= 0xDC00 && c <= 0xDFFF ? end - 1 : end
}

function rows(text) {
    var lines = text.split("\n")
    var out = []
    for (var i = 0; i < lines.length; ++i) {
        var line = expandTabs(lines[i])
        var start = 0
        do {
            var end = rowEnd(line, start + rowLength)
            out.push({ "number": start === 0 ? i + 1 : 0, "text": line.substring(start, end), "line": i, "start": start })
            start = end
        } while (start < line.length)
    }
    return { "rows": out, "lines": lines.length }
}
