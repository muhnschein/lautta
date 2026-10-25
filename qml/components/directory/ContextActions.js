// SPDX-License-Identifier: LGPL-2.1-or-later
// Which actions an item offers and where (SPEC §15.3, OPS-12, board
// DirectoryContext): the long-press menu holds the everyday ones as plain
// Silica rows and ends with More actions, which opens a page of the rarer
// ones for this item (MoreActionsPage.qml), each row with its theme icon.
// Capabilities decide what is applicable.
// Copy to…/Move to… are gone on purpose: copy or cut, then paste where you
// are.
.pragma library
.import "../FileIcons.js" as FileIcons

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
    }
    return id
}

// The theme icon of a row of the More actions page; Compress shows the
// archive icon archives show (FileIcons.js). Rename is a row there only for
// the selection's one item (Selection.js).
function icon(id) {
    switch (id) {
    case "rename": return "image://theme/icon-m-edit"
    case "favourite": return "image://theme/icon-m-favorite"
    case "compress": return FileIcons.archive
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
    // The item itself, file or folder (ORG-1), not the folder it is in.
    case "favourite": return true
    case "compress": return true
    }
    return false
}

// More actions last, after Delete: it leads on rather than acting.
var menuOrder = ["share", "copy", "cut", "rename", "delete", "more"]
var moreOrder = ["favourite", "compress"]

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
