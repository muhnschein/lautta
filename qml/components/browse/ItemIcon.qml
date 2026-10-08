// SPDX-License-Identifier: LGPL-2.1-or-later
// The icon of a file in the lists of this area (recents): a theme icon for
// the file's category (UI-2).
import QtQuick 2.6
import Sailfish.Silica 1.0
import Lautta 1.0

HighlightImage {
    id: icon

    property string name
    property bool isDir
    property bool highlightedItem

    readonly property string category: isDir ? "folder" : App.categoryOf(name)

    source: {
        switch (category) {
        case "folder":
            return "image://theme/icon-m-file-folder"
        case "image":
            return "image://theme/icon-m-file-image"
        case "video":
            return "image://theme/icon-m-file-video"
        case "audio":
            return "image://theme/icon-m-file-audio"
        case "archive":
            return "image://theme/icon-m-file-archive-folder"
        case "pdf":
            return "image://theme/icon-m-file-download-as-pdf"
        case "package":
            return "image://theme/icon-m-file-rpm"
        default:
            return "image://theme/icon-m-document"
        }
    }
    width: Theme.iconSizeMedium
    height: width
    sourceSize.width: width
    sourceSize.height: height
    // Tinted like AppIcon: an uncoloured theme glyph vanishes on a light ambience.
    color: Theme.primaryColor
    highlighted: highlightedItem
    highlightColor: Theme.highlightColor
}
