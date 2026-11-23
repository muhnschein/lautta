// SPDX-License-Identifier: LGPL-2.1-or-later
import QtQuick 2.6
import Sailfish.Silica 1.0
import Lautta 1.0
import "../"
import "../FileIcons.js" as FileIcons

Item {
    id: strip

    property var tiles: []
    property int heldId: -1
    readonly property int cells: 4
    readonly property real gap: Theme.paddingSmall
    readonly property real tileSize: Math.floor((Math.min(Screen.width, Screen.height) - 2 * Theme.horizontalPageMargin
                                                 - (cells - 1) * gap) / cells)
    readonly property real cellHeight: tileSize + Theme.paddingSmall / 2 + captionFont.lineSpacing

    signal touched(var tile)
    signal letGo()
    signal opened(var tile)
    signal held(var tile)
    signal allOpened()

    height: cellHeight + Theme.paddingMedium

    FontMetrics {
        id: captionFont

        font {
            family: Theme.fontFamily
            pixelSize: Theme.fontSizeExtraSmall
        }
    }

    MouseArea {
        anchors.fill: parent
    }

    Row {
        x: Theme.horizontalPageMargin
        spacing: strip.gap

        Repeater {
            model: strip.tiles.slice(0, strip.cells - 1)

            BackgroundItem {
                id: cell

                readonly property var tile: modelData
                readonly property bool lit: highlighted || strip.heldId === tile.id

                objectName: "recentTile"
                width: strip.tileSize
                height: strip.cellHeight
                onPressed: strip.touched(tile)
                onCanceled: strip.letGo()
                onClicked: strip.opened(tile)
                onPressAndHold: strip.held(tile)

                Rectangle {
                    width: strip.tileSize
                    height: width
                    visible: !tileIcon.thumbnailReady
                    color: Theme.rgba(Theme.highlightBackgroundColor, Theme.highlightBackgroundOpacity / 3)
                }
                TextThumb {
                    id: lines

                    uri: cell.tile.textThumb ? cell.tile.uri : ""
                }
                FileIcon {
                    id: tileIcon

                    size: strip.tileSize
                    glyphSize: FileIcons.snap(strip.tileSize / 2)
                    category: cell.tile.category
                    thumbnailSource: cell.tile.thumbnailSource
                    mimeType: cell.tile.mimeType
                    previewText: lines.text
                    previewMono: cell.tile.textMono
                    typeIcon: cell.tile.typeIcon
                    highlighted: cell.lit
                }
                Label {
                    objectName: "recentTileName"
                    y: strip.tileSize + Theme.paddingSmall / 2
                    width: strip.tileSize
                    horizontalAlignment: implicitWidth > width ? Text.AlignLeft : Text.AlignHCenter
                    truncationMode: TruncationMode.Fade
                    textFormat: Text.PlainText
                    text: cell.tile.name
                    font.pixelSize: Theme.fontSizeExtraSmall
                    color: cell.lit ? Theme.highlightColor : Theme.primaryColor
                }
            }
        }

        BackgroundItem {
            id: all

            objectName: "allRecents"
            width: strip.tileSize
            height: strip.cellHeight
            onClicked: strip.allOpened()

            Rectangle {
                width: strip.tileSize
                height: width
                color: Theme.rgba(Theme.highlightBackgroundColor, Theme.highlightBackgroundOpacity / 3)

                AppIcon {
                    anchors.centerIn: parent
                    icon: "image://theme/icon-m-right"
                    color: Theme.highlightColor
                    highlighted: all.highlighted
                }
            }
            Label {
                y: strip.tileSize + Theme.paddingSmall / 2
                width: strip.tileSize
                horizontalAlignment: implicitWidth > width ? Text.AlignLeft : Text.AlignHCenter
                truncationMode: TruncationMode.Fade
                //% "All recents"
                text: qsTrId("lautta-browse-all-recents")
                font.pixelSize: Theme.fontSizeExtraSmall
                color: Theme.highlightColor
            }
        }
    }
}
