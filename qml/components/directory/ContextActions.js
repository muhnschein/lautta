// SPDX-License-Identifier: LGPL-2.1-or-later
// Which actions the context menu offers and in what order (SPEC §15.3,
// boards DirectoryContext and ContextMenuOrder): the order is the
// `context_menu` setting, the five icon-row actions go to the icon row, the
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
    case "tags":
        //% "Tags"
        return qsTrId("lautta-dir-act-tags")
    case "favourite":
        //% "Favourite"
        return qsTrId("lautta-dir-act-favourite")
    case "edit":
        //% "Edit"
        return qsTrId("lautta-dir-act-edit")
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
    case "tags": return true
    case "favourite": return ctx.isDir
    case "edit": return ctx.writable && (ctx.category === "text" || ctx.category === "code" || ctx.category === "markdown")
    case "open_remote": return file && !ctx.isLocal
    }
    return false
}

// { row: [{ id, icon, text }], list: [{ id, text }] } for the setting's order.
function split(order, ctx) {
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

// The order from App.setting("context_menu") (JSON text); the default order
// when the setting is missing or unreadable.
function order(settingValue) {
    try {
        var parsed = JSON.parse(settingValue)
        if (parsed && parsed.length)
            return parsed
    } catch (e) {
    }
    return ["open_with", "share", "copy", "cut", "rename", "delete", "copy_to", "move_to",
            "download", "upload_to", "info", "compress", "extract", "tags", "favourite", "edit", "open_remote"]
}

// Actions of the selection panel's "more" row (board DirectorySelect).
function moreActions(single, hasPermissions) {
    var out = [
        { "id": "compress", "icon": "image://theme/icon-m-file-compressed", "text": text("compress") },
        //% "Rename all"
        { "id": "rename_all", "icon": "dir-rename-all", "text": qsTrId("lautta-dir-act-rename-all") },
        { "id": "tags", "icon": "dir-tag", "text": text("tags") }
    ]
    if (single && hasPermissions)
        //% "Permissions"
        out.push({ "id": "permissions", "icon": "dir-lock", "text": qsTrId("lautta-dir-act-permissions") })
    return out
}
