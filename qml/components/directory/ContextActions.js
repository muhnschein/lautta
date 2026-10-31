// SPDX-License-Identifier: LGPL-2.1-or-later
.pragma library

function text(id) {
    switch (id) {
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
    case "more":
        //% "More actions"
        return qsTrId("lautta-dir-act-more")
    case "favourite":
        //% "Add to favourites"
        return qsTrId("lautta-dir-act-favourite")
    case "compress":
        //% "Compress"
        return qsTrId("lautta-dir-act-compress")
    case "moveToNewFolder":
        //% "Move to new folder"
        return qsTrId("lautta-dir-act-move-new-folder")
    }
    return id
}

function icon(id) {
    switch (id) {
    case "rename": return "image://theme/icon-m-edit"
    case "favourite": return "image://theme/icon-m-favorite"
    case "compress": return "file-archive"
    case "moveToNewFolder": return "file-folder"
    }
    return ""
}

// ctx: { isDir, writable }
function applicable(id, ctx) {
    switch (id) {
    case "share": return !ctx.isDir
    case "copy": return true
    case "cut": return ctx.writable
    case "rename": return ctx.writable
    case "delete": return ctx.writable
    case "more": return more(ctx).length > 0
    case "favourite": return true
    // Viewers pass no `writable`; there a read-only folder fails with its message.
    case "compress": return ctx.writable !== false
    case "moveToNewFolder": return ctx.writable === true
    }
    return false
}

var menuOrder = ["share", "copy", "cut", "rename", "delete", "more"]
var moreOrder = ["favourite", "compress", "moveToNewFolder"]

function pick(order, ctx) {
    var out = []
    for (var i = 0; i < order.length; ++i) {
        if (applicable(order[i], ctx))
            out.push({ "id": order[i], "text": text(order[i]), "icon": icon(order[i]) })
    }
    return out
}

// [{ id, text, icon }] of the long-press menu.
function menu(ctx) {
    return pick(menuOrder, ctx)
}

// [{ id, text, icon }] of the More actions page.
function more(ctx) {
    return pick(moreOrder, ctx)
}
