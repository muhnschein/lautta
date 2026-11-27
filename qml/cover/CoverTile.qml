// SPDX-License-Identifier: LGPL-2.1-or-later
import QtQuick 2.6
import Sailfish.Silica 1.0
import "../components"
import "../components/FileIcons.js" as FileIcons

Rectangle {
    id: tile

    property real size
    property string category: "file"
    property bool isDir
    property string thumbnailSource
    property string mimeType
    property string previewText
    property bool previewMono
    property string typeIcon

    objectName: "coverTile"
    width: size
    height: size
    color: Theme.rgba(Theme.highlightBackgroundColor, Theme.highlightBackgroundOpacity / 3)

    FileIcon {
        size: tile.size
        glyphSize: FileIcons.snap(tile.size / 2)
        category: tile.category
        isDir: tile.isDir
        thumbnailSource: tile.thumbnailSource
        mimeType: tile.mimeType
        previewText: tile.previewText
        previewMono: tile.previewMono
        typeIcon: tile.typeIcon
    }
}
