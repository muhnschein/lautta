// SPDX-License-Identifier: LGPL-2.1-or-later
// Host test of the text viewer's syntax colours (PRV-13, SyntaxColours.js):
// every class gets a colour in the form TextDocument.styledLine takes, from
// the ambience's colours, with hues apart from the highlight colour's, made
// readable by the theme for its colour scheme.
import QtQuick 2.6
import "../../../qml/components/viewers/SyntaxColours.js" as SyntaxColours

Item {
    function check(cond, what) {
        if (!cond)
            console.error("FAILED: " + what)
    }

    property var schemes: []

    // Silica's Theme as far as the palette uses it; a `marking` theme makes
    // the colours it is asked to make readable translucent, to tell them.
    function theme(highlight, scheme, marking) {
        return {
            "highlightColor": highlight,
            "secondaryHighlightColor": Qt.rgba(0.5, 0.5, 1, 1),
            "secondaryColor": Qt.rgba(1, 1, 1, 0.6),
            "errorColor": Qt.rgba(1, 0.2, 0.2, 1),
            "colorScheme": scheme,
            "highlightFromColor": function(colour, forScheme) {
                schemes.push(forScheme)
                return marking ? Qt.rgba(colour.r, colour.g, colour.b, 0.4) : colour
            }
        }
    }

    function parse(palette) {
        var out = {}
        var items = palette.split(",")
        for (var i = 0; i < items.length; ++i) {
            var pair = items[i].split("=")
            check(out[pair[0]] === undefined, "one colour for " + pair[0])
            check(/^#([0-9a-f]{6}|[0-9a-f]{8})$/.test(pair[1]), pair[0] + " is a colour: " + pair[1])
            out[pair[0]] = pair[1]
        }
        return out
    }

    function hue(hex) {
        var n = parseInt(hex.slice(-6), 16)
        return SyntaxColours.hsl({ "r": (n >> 16) / 255, "g": ((n >> 8) & 255) / 255, "b": (n & 255) / 255 }).h
    }

    // Distance of two hues around the wheel, 0..0.5.
    function apart(a, b) {
        var d = Math.abs(a - b) % 1
        return Math.min(d, 1 - d)
    }

    Component.onCompleted: {
        check(SyntaxColours.hex(Qt.rgba(1, 0.5, 0, 1)) === "#ff8000", "opaque: " + SyntaxColours.hex(Qt.rgba(1, 0.5, 0, 1)))
        check(SyntaxColours.hex(Qt.rgba(1, 1, 1, 0.6)) === "#99ffffff", "translucent: " + SyntaxColours.hex(Qt.rgba(1, 1, 1, 0.6)))
        var red = SyntaxColours.hsl(Qt.rgba(1, 0, 0, 1))
        check(red.h === 0 && red.s === 1 && red.l === 0.5, "red " + JSON.stringify(red))
        check(Math.abs(SyntaxColours.hsl(Qt.rgba(0, 1, 0, 1)).h - 1 / 3) < 1e-9, "green")
        check(Math.abs(SyntaxColours.hsl(Qt.rgba(0, 0, 1, 1)).h - 2 / 3) < 1e-9, "blue")
        check(Math.abs(SyntaxColours.hsl(Qt.rgba(1, 0, 1, 1)).h - 5 / 6) < 1e-9, "magenta")
        check(SyntaxColours.hsl(Qt.rgba(0.5, 0.5, 0.5, 1)).s === 0, "grey")

        var light = Qt.rgba(0.4, 0.8, 1, 1)
        var p = parse(SyntaxColours.palette(theme(light, 0)))
        var names = ["comment", "keyword", "operator", "string", "number", "constant", "type", "function",
                     "variable", "heading", "markup", "link", "inserted", "deleted"]
        for (var i = 0; i < names.length; ++i)
            check(p[names[i]] !== undefined, names[i] + " has a colour")
        check(p.plain === undefined, "plain text keeps the text's colour")
        check(p.keyword === SyntaxColours.hex(light) && p.heading === p.keyword, "keywords: the highlight colour")
        check(p.comment === "#99ffffff" && p.deleted === "#ff3333" && p.operator === "#8080ff", "ambience colours")
        var keywordHue = hue(p.keyword)
        var turned = ["function", "string", "type", "variable", "number"]
        for (var a = 0; a < turned.length; ++a) {
            check(apart(hue(p[turned[a]]), keywordHue) > 0.1, turned[a] + " is apart from keywords: " + p[turned[a]])
            for (var b = a + 1; b < turned.length; ++b)
                check(apart(hue(p[turned[a]]), hue(p[turned[b]])) > 0.1, turned[a] + " is apart from " + turned[b])
        }
        check(schemes.length > 0 && schemes.every(function(s) { return s === 0 }), "made readable for the scheme")
        schemes = []
        var marked = parse(SyntaxColours.palette(theme(light, 1, true)))
        check(schemes.length > 0 && schemes.every(function(s) { return s === 1 }), "for a light ambience too")
        var made = ["function", "string", "type", "link", "variable", "number", "constant", "inserted"]
        for (var m = 0; m < made.length; ++m)
            check(marked[made[m]].length === 9, made[m] + " is made readable by the theme: " + marked[made[m]])
        // A grey highlight still gives colours.
        var grey = parse(SyntaxColours.palette(theme(Qt.rgba(0.5, 0.5, 0.5, 1), 0)))
        var g = parseInt(grey.string.slice(1), 16)
        check((g >> 16) !== (g & 255) || ((g >> 8) & 255) !== (g & 255), "strings are coloured: " + grey.string)
    }
}
