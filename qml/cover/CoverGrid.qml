// SPDX-License-Identifier: LGPL-2.1-or-later
import QtQuick 2.6
import Sailfish.Silica 1.0
import Lautta 1.0

Item {
    id: grid

    property alias model: view.model
    readonly property int columns: 3
    readonly property real gap: Theme.paddingSmall
    readonly property real tile: Math.floor((width - (columns - 1) * gap) / columns)
    readonly property alias count: view.count

    function field(row, name, fallback) {
        return row[name] === undefined || row[name] === null ? fallback : row[name]
    }

    function told(row) {
        if (row.thumbnailSource !== undefined || !row.uri)
            return row
        var json = App.tileOf(row.uri)
        return json.length > 0 ? JSON.parse(json) : row
    }

    clip: true

    GridView {
        id: view

        width: grid.columns * cellWidth
        height: parent.height
        cellWidth: grid.tile + grid.gap
        cellHeight: cellWidth
        interactive: false

        delegate: CoverTile {
            id: cell

            readonly property bool ownText: row.textPreview !== undefined
            readonly property alias textAsked: lines.uri
            readonly property var row: grid.told(typeof modelData !== "undefined" ? modelData : model)

            size: grid.tile
            isDir: grid.field(row, "isDir", false)
            category: grid.field(row, "category", isDir ? "folder" : App.categoryOf(grid.field(row, "name", "")))
            thumbnailSource: grid.field(row, "thumbnailSource", "")
            mimeType: grid.field(row, "mimeType", "")
            previewText: ownText ? row.textPreview : lines.text
            previewMono: grid.field(row, "textPreviewMono", grid.field(row, "textMono", false))
            typeIcon: grid.field(row, "typeIcon", "")
            Component.onCompleted: if (grid.field(row, "textThumb", false) && view.model.requestPreview)
                                       view.model.requestPreview(row.uri)

            TextThumb {
                id: lines

                uri: !cell.ownText && grid.field(cell.row, "textThumb", false) ? cell.row.uri : ""
            }
        }
    }

    OpacityRampEffect {
        sourceItem: view
        direction: OpacityRamp.TopToBottom
        offset: 0.75
        slope: 4
    }
}
