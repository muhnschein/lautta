// SPDX-License-Identifier: LGPL-2.1-or-later Qt 5.6 elides middle of a single line only: cut
// measured here. `measure(text)` = width of a line (FontMetrics in app, fixed pitch in host test).
.pragma library

var ELLIPSIS = "…"
var EXTENSION_MAX = 8
var TAIL_CONTEXT = 3

function _splits(text, i) {
    if (i <= 0 || i >= text.length)
        return false
    var code = text.charCodeAt(i)
    return (code >= 0xDC00 && code <= 0xDFFF) || (code >= 0x0300 && code <= 0x036F)
           || code === 0x200D || code === 0xFE0F
}

function _cutBefore(text, i) {
    while (i > 0 && _splits(text, i))
        --i
    return i
}

function _cutAfter(text, i) {
    while (i < text.length && _splits(text, i))
        ++i
    return i
}

function _fit(text, width, measure) {
    var low = 0
    var high = text.length
    while (low < high) {
        var mid = Math.ceil((low + high) / 2)
        if (measure(text.slice(0, mid)) <= width)
            low = mid
        else
            high = mid - 1
    }
    var cut = _cutBefore(text, low)
    return cut > 0 ? cut : _cutAfter(text, 1)
}

function _isBreak(text, at) {
    if (at <= 0 || at >= text.length || _splits(text, at))
        return false
    return text.charAt(at) === " " || text.charAt(at - 1) === "-" || text.charAt(at - 1) === "_"
}

function _sides(text, at) {
    return [text.slice(0, at).replace(/\s+$/, ""), text.slice(at).replace(/^\s+|\s+$/g, "")]
}

function _cutAt(text, fit, width, measure) {
    var restFits = measure(text.slice(fit)) <= width
    var floor = restFits ? 1 : Math.ceil(fit * 2 / 3)
    for (var at = fit; at >= floor; --at) {
        if (_isBreak(text, at))
            return !restFits || measure(_sides(text, at)[1]) <= width ? at : fit
    }
    return fit
}

function _extensionLength(text) {
    var dot = text.lastIndexOf(".")
    return dot > 0 && text.length - dot <= EXTENSION_MAX ? text.length - dot : 0
}

function _elided(text, kept, tailMin) {
    var tail = Math.min(kept, Math.max(Math.ceil(kept / 2), tailMin))
    var head = _cutBefore(text, kept - tail)
    var start = _cutBefore(text, text.length - tail)
    return text.slice(0, head) + ELLIPSIS + text.slice(start)
}

function _middle(text, width, measure) {
    var tailMin = Math.min(text.length - 1, _extensionLength(text) + TAIL_CONTEXT)
    var low = 0
    var high = text.length - 1
    while (low < high) {
        var mid = Math.ceil((low + high) / 2)
        if (measure(_elided(text, mid, tailMin)) <= width)
            low = mid
        else
            high = mid - 1
    }
    return _elided(text, low, tailMin)
}

function lines(name, width, measure) {
    if (name.length === 0 || measure(name) <= width)
        return [name]
    var sides = _sides(name, _cutAt(name, _fit(name, width, measure), width, measure))
    var out = []
    if (sides[0].length > 0)
        out.push(sides[0])
    if (sides[1].length > 0)
        out.push(measure(sides[1]) <= width ? sides[1] : _middle(sides[1], width, measure))
    return out.length > 0 ? out : [""]
}
