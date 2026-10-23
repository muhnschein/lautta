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
    readonly property int gridColumns: setting("grid_columns") === 3 ? 3 : 4
    readonly property bool gridCaptions: gridColumns === 3
    readonly property bool thumbnails: setting("thumbnails_local") === true
    // As DirectoryView: landscape keeps the tile size of portrait.
    readonly property real tileWidth: Math.min(Screen.width, Screen.height) / gridColumns
    readonly property int columns: tileWidth > 0 ? Math.max(gridColumns, Math.round(screenWidth / tileWidth)) : gridColumns
    readonly property real cellWidth: screenWidth / columns
    readonly property real cellHeight: cellWidth + (gridCaptions ? nameMetrics.height + sizeMetrics.height + Theme.paddingSmall : 0)
    // Modified times count back from now, so the rows read like a real
    // folder's. File names keep their extensions out of the translations.
    readonly property var sampleEntries: {
        var now = Date.now()
        var minute = 60 * 1000
        var day = 24 * 60 * minute
        return [
            //% "Photos"
            { "name": qsTrId("lautta-view-preview-photos"), "dir": true, "modified": now - 2 * day },
            //% "Work"
            { "name": qsTrId("lautta-view-preview-work"), "dir": true, "modified": now - 190 * minute },
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

    // The rows as drawn, in order (tests): { name, isDir, size, photo }.
    function shown() {
        var drawn = gridMode ? tiles : listRows
        var out = []
        for (var i = 0; i < drawn.count; ++i) {
            var row = drawn.itemAt(i)
            out.push({ "name": row.name, "isDir": row.isDir, "size": row.size, "photo": row.photo })
        }
        return out
    }

    height: frame.height

    FontMetrics {
        id: nameMetrics
        font.pixelSize: Theme.fontSizeExtraSmall
    }

    FontMetrics {
        id: sizeMetrics
        font.pixelSize: Theme.fontSizeTiny
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

    // A tile of DirectoryView's grid: 4 per row with names on icons only,
    // 3 per row with name and size under every tile.
    Component {
        id: gridTile

        Item {
            id: cell

            readonly property string name: modelData.name
            readonly property bool isDir: modelData.isDir
            readonly property real size: modelData.size
            readonly property bool photo: preview.thumbnails && modelData.category === "image"
            readonly property real gap: preview.gridCaptions ? Theme.paddingSmall : Math.round(Theme.paddingSmall / 4)

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
                    category: modelData.category
                    isDir: modelData.isDir
                }
                Loader {
                    anchors.fill: parent
                    active: cell.photo
                    sourceComponent: photoComponent
                }
                Label {
                    visible: !preview.gridCaptions && !cell.photo
                    anchors {
                        left: parent.left
                        right: parent.right
                        bottom: parent.bottom
                        margins: Theme.paddingSmall
                    }
                    horizontalAlignment: Text.AlignHCenter
                    text: modelData.name
                    truncationMode: TruncationMode.Elide
                    font.pixelSize: Theme.fontSizeExtraSmall
                }
            }

            Column {
                visible: preview.gridCaptions
                anchors {
                    top: tile.bottom
                    topMargin: Theme.paddingSmall / 2
                    left: tile.left
                    right: tile.right
                }

                Label {
                    width: parent.width
                    horizontalAlignment: Text.AlignHCenter
                    text: modelData.name
                    truncationMode: TruncationMode.Elide
                    font.pixelSize: Theme.fontSizeExtraSmall
                }
                Label {
                    width: parent.width
                    horizontalAlignment: Text.AlignHCenter
                    text: !modelData.isDir && modelData.size >= 0 ? Format.formatFileSize(modelData.size) : ""
                    truncationMode: TruncationMode.Elide
                    font.pixelSize: Theme.fontSizeTiny
                    color: Theme.secondaryColor
                }
            }
        }
    }

    // A half-size screen, as the Browse page preview: its size follows the
    // page's alone, never the folder drawn in it, so that no view setting
    // (list or grid, items per row, names, the hidden file) makes the
    // preview grow or shrink.
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
