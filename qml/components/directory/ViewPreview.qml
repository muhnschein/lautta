// SPDX-License-Identifier: LGPL-2.1-or-later
import QtQuick 2.6
import Sailfish.Silica 1.0
import Lautta 1.0
import "../"
import "DirFormat.js" as DirFormat
import "GridRows.js" as GridRows

Item {
    id: preview

    property real screenWidth
    property real screenHeight
    readonly property real miniature: 0.5
    readonly property string sortKey: setting("sort_key")
    readonly property bool descending: setting("sort_descending") === true
    readonly property bool foldersFirst: setting("folders_first") === true
    readonly property bool showHidden: setting("show_hidden") === true
    readonly property bool gridMode: setting("view_mode") === "grid"
    readonly property bool thumbnails: setting("thumbnails_local") === true
    readonly property real tileWidth: Math.min(Screen.width, Screen.height) / gridMetrics.portraitColumns
    readonly property int columns: GridRows.columns(screenWidth, tileWidth, gridMetrics.portraitColumns)
    readonly property real cellWidth: screenWidth / columns
    readonly property var sampleEntries: {
        var now = Date.now()
        var minute = 60 * 1000
        var day = 24 * 60 * minute
        return [
            //% "Photos"
            { "name": qsTrId("lautta-view-preview-photos"), "dir": true, "items": 24, "modified": now - 2 * day },
            //% "Work"
            { "name": qsTrId("lautta-view-preview-work"), "dir": true, "items": 6, "modified": now - 190 * minute },
            //% "beach"
            { "name": qsTrId("lautta-view-preview-photo") + ".jpg", "size": 2400000, "modified": now - day - 30 * minute },
            //% "notes"
            { "name": qsTrId("lautta-view-preview-notes") + ".txt", "size": 3200, "modified": now - 20 * minute },
            //% "report"
            { "name": qsTrId("lautta-view-preview-report") + ".pdf", "size": 845000, "modified": now - 8 * day },
            //% "song"
            { "name": qsTrId("lautta-view-preview-song") + ".mp3", "size": 5100000, "modified": now - 30 * day },
            //% "backup"
            { "name": "." + qsTrId("lautta-view-preview-hidden"), "size": 12000, "modified": now - 3 * day }
        ]
    }
    readonly property var rows: App.previewRows(JSON.stringify(sampleEntries), sortKey, descending,
                                                foldersFirst, showHidden)
    readonly property int count: rows.length

    function setting(key) {
        var dependency = App.settingsJson
        return App.setting(key)
    }

    function itemsOf(name) {
        for (var i = 0; i < sampleEntries.length; ++i) {
            if (sampleEntries[i].name === name && sampleEntries[i].dir)
                return sampleEntries[i].items
        }
        return -1
    }

    function shown() {
        var drawn = gridMode ? tiles() : rowsOf(listRows)
        return drawn.map(function (row) {
            return { "name": row.name, "isDir": row.isDir, "size": row.size, "photo": row.photo,
                     "text": !!row.previewText, "captions": row.captions }
        })
    }

    function rowsOf(repeater) {
        var out = []
        for (var i = 0; i < repeater.count; ++i)
            out.push(repeater.itemAt(i))
        return out
    }

    function tiles() {
        return rowsOf(gridRows).reduce(function (all, row) { return all.concat(row.tiles()) }, [])
    }

    height: frame.height

    GridMetrics {
        id: gridMetrics
    }

    Component {
        id: photoComponent

        Rectangle {
            clip: true
            gradient: Gradient {
                GradientStop { position: 0; color: "#5d9bd5" }
                GradientStop { position: 1; color: "#cfe3f5" }
            }

            Rectangle {
                x: parent.width * 0.6
                y: parent.height * 0.16
                width: parent.width * 0.22
                height: width
                radius: width / 2
                color: "#ffe08a"
            }
            Rectangle {
                x: -parent.width * 0.3
                y: parent.height * 0.6
                width: parent.width * 1.6
                height: width
                radius: width / 2
                color: "#5f9e4a"
            }
        }
    }

    Component {
        id: listRow

        Item {
            readonly property string name: modelData.name
            readonly property bool isDir: modelData.isDir
            readonly property real size: modelData.size
            readonly property bool photo: rowIcon.photo
            readonly property var captions: []

            width: folder.width
            height: Theme.itemSizeMedium

            Item {
                id: rowIcon

                readonly property bool photo: preview.thumbnails && modelData.category === "image"

                x: Theme.horizontalPageMargin
                anchors.verticalCenter: parent.verticalCenter
                width: Theme.iconSizeMedium
                height: width

                FileIcon {
                    visible: !rowIcon.photo
                    size: parent.width
                    category: modelData.category
                    isDir: modelData.isDir
                }
                Loader {
                    anchors.fill: parent
                    active: rowIcon.photo
                    sourceComponent: photoComponent
                }
            }

            Column {
                anchors {
                    left: rowIcon.right
                    leftMargin: Theme.paddingLarge
                    right: parent.right
                    rightMargin: Theme.horizontalPageMargin
                    verticalCenter: parent.verticalCenter
                }

                Label {
                    width: parent.width
                    text: modelData.name
                    truncationMode: TruncationMode.Fade
                    font.pixelSize: Theme.fontSizeMedium
                }
                Label {
                    width: parent.width
                    text: DirFormat.subtitle(modelData.isDir, false, modelData.size, modelData.modified)
                    truncationMode: TruncationMode.Fade
                    font.pixelSize: Theme.fontSizeExtraSmall
                    color: Theme.secondaryColor
                }
            }
        }
    }

    Component {
        id: gridTile

        Item {
            id: cell

            readonly property string name: modelData.name
            readonly property bool isDir: modelData.isDir
            readonly property real size: modelData.size
            readonly property bool photo: preview.thumbnails && modelData.category === "image"
            readonly property string previewText: preview.thumbnails && modelData.category === "text"
                                                  //% "Ferry at 06:00\nSauna wood: 2 sacks\nCabin keys from Anni"
                                                  ? qsTrId("lautta-view-preview-notes-text") : ""
            readonly property real gap: gridMetrics.gap
            readonly property var captions: DirFormat.gridCaptions(modelData.isDir, modelData.size, modelData.modified,
                                                                   true, preview.itemsOf(modelData.name), false)

            x: index * width
            width: preview.cellWidth
            height: parent ? parent.height : implicitHeight
            implicitHeight: gridMetrics.cellHeight(width, caption.implicitHeight)

            Item {
                id: tile

                x: cell.gap
                y: cell.gap
                width: preview.cellWidth - 2 * cell.gap
                height: width

                Rectangle {
                    anchors.fill: parent
                    visible: !cell.photo
                    color: Theme.rgba(Theme.highlightBackgroundColor, Theme.highlightBackgroundOpacity / 3)
                }
                FileIcon {
                    anchors.fill: parent
                    visible: !cell.photo
                    size: tile.width
                    glyphSize: gridMetrics.glyphSize(tile.width)
                    category: modelData.category
                    isDir: modelData.isDir
                    previewText: cell.previewText
                    typeIcon: "types/text"
                }
                Loader {
                    anchors.fill: parent
                    active: cell.photo
                    sourceComponent: photoComponent
                }
            }

            GridCaption {
                id: caption

                anchors {
                    top: tile.bottom
                    topMargin: Theme.paddingSmall / 2
                    left: tile.left
                    right: tile.right
                }
                metrics: gridMetrics
                name: modelData.name
                captions: cell.captions
            }
        }
    }

    Component {
        id: gridRow

        Item {
            id: previewRow

            readonly property int first: index * preview.columns

            function tiles() {
                return preview.rowsOf(rowTiles)
            }

            width: preview.screenWidth
            height: GridRows.tallest(children)

            Repeater {
                id: rowTiles

                model: preview.rows.slice(previewRow.first, previewRow.first + preview.columns)
                delegate: gridTile
            }
        }
    }

    // Size follows page's alone, never folder drawn: no view setting makes preview grow or shrink.
    Rectangle {
        id: frame

        anchors.horizontalCenter: parent.horizontalCenter
        width: preview.screenWidth * preview.miniature
        height: preview.screenHeight * preview.miniature
        radius: Theme.paddingLarge
        clip: true
        color: Theme.rgba(Theme.highlightDimmerColor, Theme.opacityLow)
        border {
            width: Math.max(1, Math.round(Theme.paddingSmall / 4))
            color: Theme.rgba(Theme.primaryColor, Theme.opacityLow)
        }

        Column {
            id: folder

            width: preview.screenWidth
            scale: preview.miniature
            transformOrigin: Item.TopLeft
            enabled: false

            PageHeader {
                //% "Documents"
                title: qsTrId("lautta-view-preview-title")
            }
            Column {
                visible: !preview.gridMode

                Repeater {
                    id: listRows

                    model: preview.gridMode ? [] : preview.rows
                    delegate: listRow
                }
            }
            Column {
                visible: preview.gridMode

                Repeater {
                    id: gridRows

                    model: preview.gridMode ? GridRows.rowCount(preview.count, preview.columns) : 0
                    delegate: gridRow
                }
            }
        }
    }
}
