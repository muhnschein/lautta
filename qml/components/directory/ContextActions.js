// SPDX-License-Identifier: LGPL-2.1-or-later
.pragma library

function text(id, ctx) {
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
        if (ctx && ctx.favourite)
            //% "Edit favourite"
            return qsTrId("lautta-dir-act-edit-favourite")
        //% "Add to favourites"
        return qsTrId("lautta-dir-act-favourite")
    case "extract":
        //% "Extract"
        return qsTrId("lautta-dir-act-extract")
    case "compress":
        //% "Compress"
        return qsTrId("lautta-dir-act-compress")
    case "checksums":
        //% "Checksums"
        return qsTrId("lautta-dir-act-checksums")
    case "verify":
        //% "Verify files"
        return qsTrId("lautta-dir-act-verify")
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
    case "extract": return "file-folder"
    case "compress": return "file-archive"
    case "checksums": return "checksum"
    case "verify": return "image://theme/icon-m-acknowledge"
    case "moveToNewFolder": return "file-folder"
    }
    return ""
}

function applicable(id, ctx) {
    var readable = ctx.readable !== false
    switch (id) {
    case "share": return !ctx.isDir && readable
    case "copy": return readable
    case "cut": return ctx.changeable === true
    case "rename": return ctx.changeable === true
    case "delete": return ctx.changeable === true
    case "more": return more(ctx).length > 0
    case "favourite": return true
    case "extract": return readable && ctx.archive === true && ctx.writable === true
    case "compress": return readable && ctx.writable === true
    case "checksums": return readable && ctx.isDir === false
    case "verify": return readable && ctx.checksumList === true
    case "moveToNewFolder": return ctx.changeable === true
    }
    return false
}

var menuOrder = ["share", "copy", "cut", "rename", "delete", "more"]
var moreOrder = ["verify", "favourite", "extract", "compress", "checksums", "moveToNewFolder"]

function pick(order, ctx) {
    var out = []
    for (var i = 0; i < order.length; ++i) {
        if (applicable(order[i], ctx))
            out.push({ "id": order[i], "text": text(order[i], ctx), "icon": icon(order[i]) })
    }
    return out
}

function menu(ctx) {
    return pick(menuOrder, ctx)
}

function more(ctx) {
    return pick(moreOrder, ctx)
}
