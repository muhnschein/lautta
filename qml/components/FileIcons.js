// SPDX-License-Identifier: LGPL-2.1-or-later
.pragma library

function kind(category, isDir) {
    if (isDir)
        return "folder"
    switch (category) {
    case "folder":
    case "image":
    case "video":
    case "audio":
    case "archive":
    case "package":
    case "database":
        return category
    }
    return "document"
}

// 28-unit set at multiples of 28, 32-unit at multiples of 32, 80 px set at 80; elsewhere 28-unit,
// soft edges.
function setFor(px) {
    var side = Math.round(px)
    if (side % 28 === 0)
        return 28
    if (side % 32 === 0)
        return 32
    if (side % 80 === 0)
        return 80
    return 28
}

function snap(px) {
    return Math.max(28, Math.round(px / 28) * 28)
}

function sized(icon, px) {
    return icon.indexOf("file-") === 0 ? icon + "-" + setFor(px) : icon
}

function glyph(category, isDir, px) {
    return sized("file-" + kind(category, isDir), px)
}
