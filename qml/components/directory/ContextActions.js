// SPDX-License-Identifier: LGPL-2.1-or-later
// Which actions an item offers and where (SPEC §15.3, board
// DirectoryContext): the long-press menu holds the everyday ones as plain
// Silica rows, Details' pulley the rest. Capabilities decide what is
// applicable. Copy to…/Move to… are gone on purpose: copy or cut, then
// paste where you are.
.pragma library

function text(id) {
    switch (id) {
    case "open_with":
        //% "Open with"
        return qsTrId("lautta-dir-act-open-with")
    case "share":
        //% "Share"
        return qsTrId("lautta-dir-act-share")
    case "copy":
        //% "Copy"
        return qsTrId("lautta-dir-act-copy")
    case "cut":
        //% "Cut"
        return qsTrId("lautta-dir-act-cut")
    case "rename":
        //% "Rename"
        return qsTrId("lautta-dir-act-rename")
    case "delete":
        //% "Delete"
        return qsTrId("lautta-dir-act-delete")
    case "info":
        //% "Details"
        return qsTrId("lautta-dir-act-details")
    case "compress":
        //% "Compress"
        return qsTrId("lautta-dir-act-compress")
    }
    return id
}

// ctx: { isDir, writable }
function applicable(id, ctx) {
    var file = !ctx.isDir
    switch (id) {
    case "open_with": return file
    case "share": return file
    case "copy": return true
    case "cut": return ctx.writable
    case "rename": return ctx.writable
    case "delete": return ctx.writable
    case "info": return true
    case "compress": return true
    }
    return false
}

// Delete last, as in Silica's own apps.
var menuOrder = ["share", "copy", "cut", "rename", "info", "delete"]
var detailsOrder = ["open_with", "compress"]

function pick(order, ctx) {
    var out = []
    for (var i = 0; i < order.length; ++i) {
        if (applicable(order[i], ctx))
            out.push({ "id": order[i], "text": text(order[i]) })
    }
    return out
}

// [{ id, text }] of the long-press menu.
function menu(ctx) {
    return pick(menuOrder, ctx)
}

// [{ id, text }] of the Details page's pulley.
function details(ctx) {
    return pick(detailsOrder, ctx)
}
