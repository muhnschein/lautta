// SPDX-License-Identifier: LGPL-2.1-or-later
// Which actions the context menu offers and in what order (SPEC §15.3,
// board DirectoryContext): the five icon-row actions go to the icon row, the
// rest to the list below it, and capabilities decide what is applicable.
.pragma library

var iconRowIds = ["share", "copy", "cut", "rename", "delete"]

var icons = {
    "share": "image://theme/icon-m-share",
    "copy": "dir-copy",
    "cut": "dir-cut",
    "rename": "image://theme/icon-m-edit",
    "delete": "image://theme/icon-m-delete"
}

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
    case "copy_to":
        //% "Copy to…"
        return qsTrId("lautta-dir-act-copy-to")
    case "move_to":
        //% "Move to…"
        return qsTrId("lautta-dir-act-move-to")
    case "download":
        //% "Download to…"
        return qsTrId("lautta-dir-act-download")
    case "upload_to":
        //% "Upload to…"
        return qsTrId("lautta-dir-act-upload")
    case "info":
        //% "Info"
        return qsTrId("lautta-dir-act-info")
    case "compress":
        //% "Compress"
        return qsTrId("lautta-dir-act-compress")
    case "extract":
        //% "Extract"
        return qsTrId("lautta-dir-act-extract")
    case "favourite":
        //% "Favourite"
        return qsTrId("lautta-dir-act-favourite")
    case "open_remote":
        //% "Open copy"
        return qsTrId("lautta-dir-act-open-remote")
    }
    return id
}

// ctx: { isDir, isLocal, writable, hasRemote, category }
function applicable(id, ctx) {
    var file = !ctx.isDir
    switch (id) {
    case "open_with": return file
    case "share": return file
    case "copy": return true
    case "cut": return ctx.writable
    case "rename": return ctx.writable
    case "delete": return ctx.writable
    case "copy_to": return true
    case "move_to": return ctx.writable
    case "download": return !ctx.isLocal && ctx.hasRemote
    case "upload_to": return ctx.isLocal && ctx.hasRemote
    case "info": return true
    case "compress": return true
    case "extract": return ctx.category === "archive"
    case "favourite": return ctx.isDir
    case "open_remote": return file && !ctx.isLocal
    }
    return false
}

// The context menu's actions in display order.
var order = ["open_with", "share", "copy", "cut", "rename", "delete", "copy_to", "move_to",
             "download", "upload_to", "info", "compress", "extract", "favourite", "open_remote"]

// { row: [{ id, icon, text }], list: [{ id, text }] } of the actions that
// apply, in `order`.
function split(ctx) {
    var row = []
    var list = []
    for (var i = 0; i < order.length; ++i) {
        var id = order[i]
        if (!applicable(id, ctx))
            continue
        if (iconRowIds.indexOf(id) >= 0)
            row.push({ "id": id, "icon": icons[id], "text": text(id) })
        else
            list.push({ "id": id, "text": text(id) })
    }
    return { "row": row, "list": list }
}
