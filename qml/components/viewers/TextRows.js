// SPDX-License-Identifier: LGPL-2.1-or-later
// The rows of the text viewer (PRV-4). A tab becomes the spaces up to the
// next multiple of `tabWidth` columns, so that in the monospace font a row
// is as wide as its characters. A line longer than `rowLength` characters
// goes on over further rows: a minified file is one line, and one Text item
// of a megabyte is slow to lay out and draw, and wider than glyphs are placed
// exactly.
.pragma library

var tabWidth = 4
var rowLength = 1000

// No text (yet): no rows.
var empty = { "rows": [], "lines": 0 }

// `line` with its tabs expanded to spaces.
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

// Where a row of `line` that would end at `end` ends: at the line's end at
// most, and never between the two halves of a surrogate pair.
function rowEnd(line, end) {
    if (end >= line.length)
        return line.length
    var c = line.charCodeAt(end)
    return c >= 0xDC00 && c <= 0xDFFF ? end - 1 : end
}

// `text` (lines separated by "\n") as { rows, lines }: rows are
// { number, text, line, start }, number the line's from 1 on its first row
// and 0 on the rows it goes on over, line the line's index (from 0) and
// start where the row starts in the line with its tabs expanded, for the
// line's colours (PRV-13); lines is the number of lines.
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
