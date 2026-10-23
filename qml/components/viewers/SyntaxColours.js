// SPDX-License-Identifier: LGPL-2.1-or-later
// Colours of the text viewer's syntax classes (PRV-13), in the form
// TextDocument.styledLine takes them: "class=#rrggbb,…". They come from the
// ambience, so they read on light and dark ambiences alike: keywords and
// headings take the highlight colour, operators and Markdown markup the
// secondary highlight colour, comments the secondary colour, removed diff
// lines the error colour, and the other classes hues turned away from the
// highlight colour's, made readable for the colour scheme by the theme.
.pragma library

// Turns of the colour wheel from the highlight colour's hue.
var turns = {
    "function": 1 / 6,
    "string": 1 / 3,
    "type": 1 / 2,
    "link": 1 / 2,
    "variable": 2 / 3,
    "number": 5 / 6,
    "constant": 5 / 6
}

// Hue, saturation and lightness (0..1) of a colour's r, g, b.
function hsl(c) {
    var high = Math.max(c.r, c.g, c.b)
    var low = Math.min(c.r, c.g, c.b)
    var lightness = (high + low) / 2
    var range = high - low
    if (range === 0)
        return { "h": 0, "s": 0, "l": lightness }
    var saturation = range / (1 - Math.abs(2 * lightness - 1))
    var hue
    if (high === c.r)
        hue = ((c.g - c.b) / range + 6) % 6
    else if (high === c.g)
        hue = (c.b - c.r) / range + 2
    else
        hue = (c.r - c.g) / range + 4
    return { "h": hue / 6, "s": saturation, "l": lightness }
}

function _byte(part) {
    var text = Math.round(Math.max(0, Math.min(1, part)) * 255).toString(16)
    return text.length < 2 ? "0" + text : text
}

// "#rrggbb", or "#aarrggbb" for a colour that is not opaque.
function hex(c) {
    var rgb = _byte(c.r) + _byte(c.g) + _byte(c.b)
    return "#" + (c.a < 1 ? _byte(c.a) : "") + rgb
}

// The highlight colour turned by `turn`; a grey highlight gets full colours.
function _turned(theme, base, turn) {
    var saturation = Math.max(base.s, 0.6)
    var lightness = Math.min(Math.max(base.l, 0.35), 0.75)
    return theme.highlightFromColor(Qt.hsla((base.h + turn) % 1, saturation, lightness, 1), theme.colorScheme)
}

// The palette for `theme` (Silica's Theme).
function palette(theme) {
    var colours = {
        "comment": theme.secondaryColor,
        "keyword": theme.highlightColor,
        "heading": theme.highlightColor,
        "operator": theme.secondaryHighlightColor,
        "markup": theme.secondaryHighlightColor,
        "deleted": theme.errorColor,
        "inserted": theme.highlightFromColor(Qt.hsla(1 / 3, 0.8, 0.5, 1), theme.colorScheme)
    }
    var base = hsl(theme.highlightColor)
    for (var name in turns)
        colours[name] = _turned(theme, base, turns[name])
    var out = []
    for (var key in colours)
        out.push(key + "=" + hex(colours[key]))
    return out.join(",")
}
