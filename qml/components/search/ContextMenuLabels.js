// SPDX-License-Identifier: LGPL-2.1-or-later
// Names and icons of the context menu entries (ids of
// lautta_core::settings::DEFAULT_CONTEXT_MENU) for Settings and the order
// page. Icons are theme icons (UI-2).
.pragma library

function label(id) {
    switch (id) {
    case "open_with":
        //% "Open with"
        return qsTrId("lautta-ctx-open-with")
    case "share":
        //% "Share"
        return qsTrId("lautta-ctx-share")
    case "copy":
        //% "Copy"
        return qsTrId("lautta-ctx-copy")
    case "cut":
        //% "Cut"
        return qsTrId("lautta-ctx-cut")
    case "rename":
        //% "Rename"
        return qsTrId("lautta-ctx-rename")
    case "delete":
        //% "Delete"
        return qsTrId("lautta-ctx-delete")
    case "copy_to":
        //% "Copy to…"
        return qsTrId("lautta-ctx-copy-to")
    case "move_to":
        //% "Move to…"
        return qsTrId("lautta-ctx-move-to")
    case "download":
        //% "Download"
        return qsTrId("lautta-ctx-download")
    case "upload_to":
        //% "Upload to…"
        return qsTrId("lautta-ctx-upload-to")
    case "info":
        //% "Info"
        return qsTrId("lautta-ctx-info")
    case "compress":
        //% "Compress"
        return qsTrId("lautta-ctx-compress")
    case "extract":
        //% "Extract"
        return qsTrId("lautta-ctx-extract")
    case "tags":
        //% "Tags"
        return qsTrId("lautta-ctx-tags")
    case "favourite":
        //% "Add to favourites"
        return qsTrId("lautta-ctx-favourite")
    case "edit":
        //% "Edit"
        return qsTrId("lautta-ctx-edit")
    case "open_remote":
        //% "Open a copy"
        return qsTrId("lautta-ctx-open-remote")
    default:
        // An entry from a newer version: show its id.
        return id
    }
}

function icon(id) {
    var icons = {
        "open_with": "icon-m-redirect",
        "share": "icon-m-share",
        "copy": "icon-m-clipboard",
        "cut": "icon-m-crop",
        "rename": "icon-m-edit",
        "delete": "icon-m-delete",
        "copy_to": "icon-m-add-to-grid",
        "move_to": "icon-m-right",
        "download": "icon-m-cloud-download",
        "upload_to": "icon-m-cloud-upload",
        "info": "icon-m-about",
        "compress": "icon-m-file-compressed",
        "extract": "icon-m-file-archive-folder",
        "tags": "icon-m-annotation",
        "favourite": "icon-m-favorite",
        "edit": "icon-m-note",
        "open_remote": "icon-m-link"
    }
    return "image://theme/" + (icons[id] || "icon-m-other")
}
