// SPDX-License-Identifier: LGPL-2.1-or-later
import QtQuick 2.6
import Sailfish.Silica 1.0
import Lautta 1.0
import ".."
import "../directory"

Item {
    id: itemTile

    property string label
    property string name
    property var captions: []
    property string note
    property bool isDir
    property string uri
    property var look: ({})
    property bool stacked
    property real tileSize: Theme.itemSizeHuge

    readonly property var shown: look || {}
    readonly property real tileTop: labelText.height + column.spacing

    implicitHeight: column.height

    Column {
        id: column

        width: itemTile.width
        spacing: Theme.paddingSmall

        Label {
            id: labelText

            objectName: "tileLabel"
            width: itemTile.width
            horizontalAlignment: Text.AlignHCenter
            truncationMode: TruncationMode.Fade
            color: Theme.secondaryColor
            font.pixelSize: Theme.fontSizeSmall
            text: itemTile.label
        }

        Item {
            width: itemTile.width
            height: itemTile.tileSize

            Item {
                id: tile

                anchors.horizontalCenter: parent.horizontalCenter
                width: itemTile.tileSize
                height: itemTile.tileSize

                Rectangle {
                    objectName: "tileStack"
                    visible: itemTile.stacked
                    x: Theme.paddingSmall
                    y: -Theme.paddingSmall
                    width: parent.width
                    height: parent.height
                    color: Theme.rgba(Theme.highlightBackgroundColor, Theme.highlightBackgroundOpacity / 4)
                }
                Rectangle {
                    anchors.fill: parent
                    visible: itemTile.stacked || !tileIcon.thumbnailReady
                    color: Theme.rgba(Theme.highlightBackgroundColor, Theme.highlightBackgroundOpacity / 3)
                }
                TextThumb {
                    id: lines

                    uri: itemTile.shown.textThumb === true ? itemTile.uri : ""
                }
                FileIcon {
                    id: tileIcon

                    anchors.fill: parent
                    size: tile.width
                    glyphSize: tileMetrics.glyphSize(tile.width)
                    isDir: itemTile.isDir
                    category: itemTile.shown.category || "file"
                    thumbnailSource: itemTile.isDir ? "" : (itemTile.shown.thumbnailSource || "")
                    mimeType: itemTile.shown.mimeType || ""
                    previewText: lines.text
                    previewMono: itemTile.shown.textMono === true
                    typeIcon: itemTile.shown.typeIcon || ""
                }
            }
        }

        GridCaption {
            objectName: "tileCaption"
            x: (itemTile.width - width) / 2
            width: itemTile.tileSize
            height: implicitHeight
            metrics: tileMetrics
            name: itemTile.name
            captions: itemTile.captions
        }

        Label {
            objectName: "tileNote"
            visible: itemTile.note.length > 0
            width: itemTile.width
            horizontalAlignment: Text.AlignHCenter
            color: Theme.highlightColor
            font.pixelSize: Theme.fontSizeTiny
            text: itemTile.note
        }
    }

    GridMetrics {
        id: tileMetrics
    }
}
