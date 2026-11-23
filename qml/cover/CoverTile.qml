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
    // Set: a place, drawn with this glyph.
    property string placeIcon

    objectName: "coverTile"
    width: size
    height: size
    color: Theme.rgba(Theme.highlightBackgroundColor, Theme.highlightBackgroundOpacity / 3)

    FileIcon {
        visible: tile.placeIcon.length === 0
        size: tile.size
        glyphSize: FileIcons.snap(tile.size / 2)
        category: tile.category
        isDir: tile.isDir
        thumbnailSource: visible ? tile.thumbnailSource : ""
        mimeType: tile.mimeType
    }
    AppIcon {
        objectName: "placeGlyph"
        visible: tile.placeIcon.length > 0
        anchors.centerIn: parent
        iconSize: FileIcons.snap(tile.size / 2)
        icon: FileIcons.sized(tile.placeIcon, iconSize)
    }
}
