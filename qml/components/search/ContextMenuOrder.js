// SPDX-License-Identifier: LGPL-2.1-or-later
// Reordering of the context menu actions (the `context_menu` setting): the
// first ICON_ROW entries are the icon row, the rest is the list below it.
.pragma library

var ICON_ROW = 5

// lautta_core::settings::DEFAULT_CONTEXT_MENU
var DEFAULT_ORDER = [
    "open_with", "share", "copy", "cut", "rename", "delete", "copy_to", "move_to", "download",
    "upload_to", "info", "compress", "extract", "tags", "favourite", "edit", "open_remote"
]

function _copy(list) {
    return list.slice(0)
}

function moveUp(list, index) {
    var out = _copy(list)
    if (index <= 0 || index >= out.length)
        return out
    var item = out[index]
    out[index] = out[index - 1]
    out[index - 1] = item
    return out
}

function moveDown(list, index) {
    var out = _copy(list)
    if (index < 0 || index >= out.length - 1)
        return out
    var item = out[index]
    out[index] = out[index + 1]
    out[index + 1] = item
    return out
}

// An entry of the list becomes the last icon in the row; the entry that was
// last in the row moves to the top of the list.
function toRow(list, index) {
    var out = _copy(list)
    if (index < ICON_ROW || index >= out.length)
        return out
    var item = out.splice(index, 1)[0]
    out.splice(ICON_ROW - 1, 0, item)
    return out
}

// An icon becomes the first entry of the list; the first list entry takes
// its place in the row.
function toList(list, index) {
    var out = _copy(list)
    if (index < 0 || index >= ICON_ROW || index >= out.length)
        return out
    var item = out.splice(index, 1)[0]
    out.splice(ICON_ROW, 0, item)
    return out
}

// The stored order, tolerant of text that is not a list.
function parse(text) {
    try {
        var list = JSON.parse(text)
        return Array.isArray(list) ? list : DEFAULT_ORDER.slice(0)
    } catch (e) {
        return DEFAULT_ORDER.slice(0)
    }
}
