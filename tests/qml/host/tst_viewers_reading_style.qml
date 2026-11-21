// SPDX-License-Identifier: LGPL-2.1-or-later
import QtQuick 2.6
import "../../../qml/components/viewers/ReadingStyle.js" as ReadingStyle
import "../../../qml/components/viewers/SyntaxColours.js" as SyntaxColours

Item {
    property bool reachedEnd
    function check(cond, what) {
        if (!cond)
            console.error("FAILED: " + what)
    }

    // Theme stub: distinct colours, so each key shows where it came from.
    readonly property var theme: ({
        "highlightColor": Qt.rgba(0.4, 0.8, 1, 1),
        "highlightBackgroundColor": Qt.rgba(0, 0, 1, 1),
        "primaryColor": Qt.rgba(1, 1, 1, 1),
        "secondaryColor": Qt.rgba(1, 1, 1, 0.6),
        "secondaryHighlightColor": Qt.rgba(0.5, 0.5, 1, 1),
        "errorColor": Qt.rgba(1, 0.2, 0.2, 1),
        "colorScheme": 0,
        "opacityFaint": 0.2,
        "paddingLarge": 24.4,
        "fontSizeSmall": 27.6,
        "rgba": function(c, a) { return Qt.rgba(c.r, c.g, c.b, a) },
        "highlightFromColor": function(c) { return c }
    })

    Component.onCompleted: {
        var spec = ReadingStyle.colours(theme)
        var got = {}
        spec.split(",").forEach(function(item) {
            var pair = item.split("=")
            check(got[pair[0]] === undefined, "one value for " + pair[0])
            got[pair[0]] = pair[1]
        })
        check(got["md-heading"] === "#66ccff" && got["md-accent"] === "#66ccff", "headings and accents: highlight " + spec)
        check(got["md-secondary"] === "#99ffffff", "secondary text " + got["md-secondary"])
        check(got["md-tint"] === "#330000ff", "tint: faint highlight background " + got["md-tint"])
        check(got["md-rule"] === "#33ffffff", "rule: faint primary " + got["md-rule"])
        check(got["md-gap"] === "24" && got["md-code"] === "28", "sizes in whole px: " + got["md-gap"] + " " + got["md-code"])
        check(spec.indexOf(SyntaxColours.palette(theme)) === 0, "syntax palette for fenced code: " + spec)
        reachedEnd = true
    }
}
