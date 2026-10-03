// SPDX-License-Identifier: LGPL-2.1-or-later
// The glyph of a file or folder in the operations pages (theme icons, UI-2).
import QtQuick 2.6
import ".."

AppIcon {
    // FileCategory icon name from App.categoryOf / the "category" of Info.
    property string category: "file"
    property bool isDir

    function themeIcon(name) {
        switch (name) {
        case "folder": return "image://theme/icon-m-file-folder"
        case "image": return "image://theme/icon-m-file-image"
        case "video": return "image://theme/icon-m-file-video"
        case "audio": return "image://theme/icon-m-file-audio"
        case "archive": return "image://theme/icon-m-file-compressed"
        case "pdf": return "image://theme/icon-m-file-download-as-pdf"
        case "package": return "image://theme/icon-m-file-apk"
        default: return "image://theme/icon-m-document"
        }
    }

    icon: themeIcon(isDir ? "folder" : category)
}
