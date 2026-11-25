// SPDX-License-Identifier: LGPL-2.1-or-later
.pragma library
.import "SyntaxColours.js" as SyntaxColours

function colours(theme) {
    var own = {
        "md-heading": theme.highlightColor,
        "md-accent": theme.highlightColor,
        "md-secondary": theme.secondaryColor,
        "md-tint": theme.rgba(theme.highlightBackgroundColor, theme.opacityFaint),
        "md-rule": theme.rgba(theme.primaryColor, theme.opacityFaint)
    }
    var out = [SyntaxColours.palette(theme)]
    for (var key in own)
        out.push(key + "=" + SyntaxColours.hex(own[key]))
    out.push("md-gap=" + Math.round(theme.paddingLarge))
    out.push("md-code=" + Math.round(theme.fontSizeSmall))
    return out.join(",")
}
