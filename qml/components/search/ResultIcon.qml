// SPDX-License-Identifier: LGPL-2.1-or-later
// The icon of a search or compare row: a theme file icon by category
// (the core's FileCategory icon name, `App.categoryOf`).
import QtQuick 2.6
import "../"

AppIcon {
    property string category
    property bool isDir

    function iconFor(name) {
        switch (name) {
        case "folder":
            return "image://theme/icon-m-file-folder"
        case "image":
            return "image://theme/icon-m-file-image"
        case "video":
            return "image://theme/icon-m-file-video"
        case "audio":
            return "image://theme/icon-m-file-audio"
        case "archive":
            return "image://theme/icon-m-file-compressed"
        case "package":
            return "image://theme/icon-m-file-rpm"
        case "pdf":
            return "image://theme/icon-m-file-download-as-pdf"
        default:
            return "image://theme/icon-m-document"
        }
    }

    icon: iconFor(isDir ? "folder" : category)
}
