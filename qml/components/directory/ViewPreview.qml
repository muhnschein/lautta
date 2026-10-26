// SPDX-License-Identifier: LGPL-2.1-or-later
// A half-size picture of a folder for the view options page (BRW-4), like
// the Browse page preview (BRW-12): sample folders and files, a hidden file
// and a photo, drawn as DirectoryView draws a folder on a screen of the
// page's size. It shows the global view settings and follows every change
// at once, at a size that does not change with them; the order and the
// hidden file come from the core (App.previewRows). Takes no input.
import QtQuick 2.6
import Sailfish.Silica 1.0
import Lautta 1.0
import "../"
import "DirFormat.js" as DirFormat

Item {
    id: preview

    // The page's size, which is a folder page's size.
    property real screenWidth
    property real screenHeight
    readonly property real miniature: 0.5
    readonly property string sortKey: setting("sort_key")
    readonly property bool descending: setting("sort_descending") === true
    readonly property bool foldersFirst: setting("folders_first") === true
    readonly property bool showHidden: setting("show_hidden") === true
    readonly property bool gridMode: setting("view_mode") === "grid"
    readonly property bool thumbnails: setting("thumbnails_local") === true
    // As DirectoryView: landscape keeps the tile size of portrait.
    readonly property real tileWidth: Math.min(Screen.width, Screen.height) / gridMetrics.portraitColumns
    readonly property int columns: tileWidth > 0
                                   ? Math.max(gridMetrics.portraitColumns, Math.round(screenWidth / tileWidth))
                                   : gridMetrics.portraitColumns
    readonly property real cellWidth: screenWidth / columns
    readonly property real cellHeight: gridMetrics.cellHeight(cellWidth)
    // Modified times count back from now, so the rows read like a real
    // folder's; folders have a number of items, as on the device. File names
    // keep their extensions out of the translations.
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
    // The samples as a folder shows them: { name, isDir, size, modified,
    // hidden, category } each.
    readonly property var rows: App.previewRows(JSON.stringify(sampleEntries), sortKey, descending,
                                                foldersFirst, showHidden)
    readonly property int count: rows.length

    function setting(key) {
        var dependency = App.settingsJson
        return App.setting(key)
    }

    // The items of the sample folder called `name`, -1 for a file.
    function itemsOf(name) {
        for (var i = 0; i < sampleEntries.length; ++i) {
            if (sampleEntries[i].name === name && sampleEntries[i].dir)
                return sampleEntries[i].items
        }
        return -1
    }

    // The rows as drawn, in order (tests): { name, isDir, size, photo,
    // captions }, the captions as the lines under a tile.
    function shown() {
        var drawn = gridMode ? tiles : listRows
        var out = []
        for (var i = 0; i < drawn.count; ++i) {
            var row = drawn.itemAt(i)
            out.push({ "name": row.name, "isDir": row.isDir, "size": row.size, "photo": row.photo,
                       "captions": row.captions })
        }
        return out
    }

    height: frame.height

    GridMetrics {
        id: gridMetrics
    }

    // A stand-in for the photo's thumbnail: sky, sun and a hill.
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

    // A row of DirectoryView's list.
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

    // A tile of DirectoryView's grid: the name and the caption lines under it.
    Component {
        id: gridTile

        Item {
            id: cell

            readonly property string name: modelData.name
            readonly property bool isDir: modelData.isDir
            readonly property real size: modelData.size
            readonly property bool photo: preview.thumbnails && modelData.category === "image"
            readonly property real gap: gridMetrics.gap
            readonly property var captions: DirFormat.gridCaptions(modelData.isDir, modelData.size, modelData.modified,
                                                                   true, preview.itemsOf(modelData.name), false)

            width: preview.cellWidth
            height: preview.cellHeight

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
                }
                Loader {
                    anchors.fill: parent
                    active: cell.photo
                    sourceComponent: photoComponent
                }
            }

            GridCaption {
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

    // A half-size screen, as the Browse page preview: its size follows the
    // page's alone, never the folder drawn in it, so that no view setting
    // (list or grid, the hidden file) makes the preview grow or shrink.
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

        // Full size scaled down, so the layout matches a real folder.
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
            Grid {
                visible: preview.gridMode
                columns: preview.columns

                Repeater {
                    id: tiles

                    model: preview.gridMode ? preview.rows : []
                    delegate: gridTile
                }
            }
        }
    }
}
