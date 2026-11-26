// SPDX-License-Identifier: LGPL-2.1-or-later
.pragma library

function escaped(s) {
    return s.replace(/&/g, "&amp;").replace(/</g, "&lt;").replace(/>/g, "&gt;")
}

function marked(text, needle, color) {
    var term = needle ? String(needle).toLowerCase() : ""
    if (!text || term.length === 0)
        return ""
    var folded = text.toLowerCase()
    if (folded.length !== text.length)
        return ""
    var at = folded.indexOf(term)
    if (at < 0)
        return ""
    var out = ""
    var from = 0
    while (at >= 0) {
        out += escaped(text.substring(from, at))
            + "<b><font color=\"" + color + "\">" + escaped(text.substring(at, at + term.length)) + "</font></b>"
        from = at + term.length
        at = folded.indexOf(term, from)
    }
    return out + escaped(text.substring(from))
}

function needle(name, start, length) {
    return length > 0 ? name.substr(start, length) : ""
}
