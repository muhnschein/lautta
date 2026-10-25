// SPDX-License-Identifier: LGPL-2.1-or-later
// The name under a grid cell at 3 per row (SPEC §15.3, BRW-4): at most two
// lines of the cell's width. A name that does not fit keeps its beginning on
// the first line and its end, with the extension, on the second behind an
// ellipsis in the middle. Qt 5.6 elides the middle of a single line only, so
// the cut is measured here: `measure(text)` gives the width of a line, a
// FontMetrics in the app and a fixed pitch in the host test.
.pragma library

var ELLIPSIS = "…"
// The longest extension, its dot included, that the cut keeps whole.
var EXTENSION_MAX = 8
// Characters kept before the extension.
var TAIL_CONTEXT = 3

// True when a cut in front of index `i` would split a surrogate pair or part
// a combining mark, a joiner or a variation selector from its letter.
function _splits(text, i) {
    if (i <= 0 || i >= text.length)
        return false
    var code = text.charCodeAt(i)
    return (code >= 0xDC00 && code <= 0xDFFF) || (code >= 0x0300 && code <= 0x036F)
           || code === 0x200D || code === 0xFE0F
}

// The nearest cut at or before `i` that splits nothing.
function _cutBefore(text, i) {
    while (i > 0 && _splits(text, i))
        --i
    return i
}

// The nearest cut at or after `i` that splits nothing.
function _cutAfter(text, i) {
    while (i < text.length && _splits(text, i))
        ++i
    return i
}

// The cut that fills the first line: the most characters of `text` whose
// width is at most `width`, one at least.
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

// True when a line may end at the cut `at`: before a space, or after a dash
// or an underscore.
function _isBreak(text, at) {
    if (at <= 0 || at >= text.length || _splits(text, at))
        return false
    return text.charAt(at) === " " || text.charAt(at - 1) === "-" || text.charAt(at - 1) === "_"
}

// The two sides of the cut `at`, without the spaces around it.
function _sides(text, at) {
    return [text.slice(0, at).replace(/\s+$/, ""), text.slice(at).replace(/^\s+|\s+$/g, "")]
}

// Where the first line ends: at a word break near the end of the line when
// there is one, or else after the most characters that fit. A break before
// the end is taken only when the second line still fits whole, or when it
// must be elided anyway; then the break may not cost more than a third of
// the line.
function _cutAt(text, fit, width, measure) {
    var restFits = measure(text.slice(fit)) <= width
    var floor = restFits ? 1 : Math.ceil(fit * 2 / 3)
    for (var at = fit; at >= floor; --at) {
        if (_isBreak(text, at))
            return !restFits || measure(_sides(text, at)[1]) <= width ? at : fit
    }
    return fit
}

// The length of the extension of `text`, its dot included: 0 for a name
// without one and for a hidden file.
function _extensionLength(text) {
    var dot = text.lastIndexOf(".")
    return dot > 0 && text.length - dot <= EXTENSION_MAX ? text.length - dot : 0
}

// `text` with the middle replaced by an ellipsis, `kept` characters left.
// The end gets the larger share, and at least the extension and a few
// characters before it.
function _elided(text, kept, tailMin) {
    var tail = Math.min(kept, Math.max(Math.ceil(kept / 2), tailMin))
    var head = _cutBefore(text, kept - tail)
    var start = _cutBefore(text, text.length - tail)
    return text.slice(0, head) + ELLIPSIS + text.slice(start)
}

// `text` shortened in the middle to fit `width`.
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

// The lines of `name` in a cell `width` wide: one line when the name fits,
// else two, the second elided in the middle when the rest does not fit.
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
