// SPDX-License-Identifier: LGPL-2.1-or-later
// The app's own file-type glyphs (UI-2), qml/icons/file-<kind>-<set>.svg,
// shared by every place showing a kind of file: folder view, Trash, search,
// Recents, operations pages, More actions, Browse, cover. A glyph token is
// "file-<kind>" (the core's Browse rows give them too); the place showing it
// picks the set for its pixel size, so strokes land on whole pixels.
.pragma library

// Glyph kind of a FileCategory icon name (`App.categoryOf`, the model's
// `category`). Text, code, PDF, office files and unknown files share one
// document glyph.
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

// The set drawn for a side of `px` pixels: 28-unit set at multiples of 28
// (grid, list at scale 1.75), 32-unit set at multiples of 32 (list at 1.0,
// 1.5, 2.0), the 80 px set at 80 (list at 1.25). No set fits elsewhere: the
// 28-unit one then, with soft edges.
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

// `px` snapped to a multiple of 28, at least 28: the 28-unit set is sharp
// there (the grid's glyph, the cover).
function snap(px) {
    return Math.max(28, Math.round(px / 28) * 28)
}

// The largest side up to `px` that a set draws sharp, a multiple of 28 or
// of 32, at least 28: the video badge's (PRV-1).
function sharpAtMost(px) {
    var side = Math.floor(px)
    return Math.max(28, Math.floor(side / 28) * 28, Math.floor(side / 32) * 32)
}

// The AppIcon name of `icon` at `px`: a glyph token gets its set
// ("file-folder" → "file-folder-32"); anything else (a theme URL, another
// app icon) stays as it is.
function sized(icon, px) {
    return icon.indexOf("file-") === 0 ? icon + "-" + setFor(px) : icon
}

// The AppIcon name of the glyph of a file or folder at `px`.
function glyph(category, isDir, px) {
    return sized("file-" + kind(category, isDir), px)
}
