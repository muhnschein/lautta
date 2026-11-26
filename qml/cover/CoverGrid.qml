// SPDX-License-Identifier: LGPL-2.1-or-later
import QtQuick 2.6
import Sailfish.Silica 1.0
import Lautta 1.0
import "CoverRows.js" as CoverRows

Item {
    id: grid

    property alias model: view.model
    readonly property int columns: 3
    readonly property real gap: Theme.paddingSmall
    readonly property real tile: Math.floor((width - (columns - 1) * gap) / columns)
    readonly property alias count: view.count

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
            readonly property var row: CoverRows.told(typeof modelData !== "undefined" ? modelData : model, App)

            size: grid.tile
            isDir: CoverRows.field(row, "isDir", false)
            category: CoverRows.field(row, "category", isDir ? "folder" : App.categoryOf(CoverRows.field(row, "name", "")))
            thumbnailSource: CoverRows.field(row, "thumbnailSource", "")
            mimeType: CoverRows.field(row, "mimeType", "")
            previewText: ownText ? row.textPreview : lines.text
            previewMono: CoverRows.field(row, "textPreviewMono", CoverRows.field(row, "textMono", false))
            typeIcon: CoverRows.field(row, "typeIcon", "")
            Component.onCompleted: if (CoverRows.field(row, "textThumb", false) && view.model.requestPreview)
                                       view.model.requestPreview(row.uri)

            TextThumb {
                id: lines

                uri: !cell.ownText && CoverRows.field(cell.row, "textThumb", false) ? cell.row.uri : ""
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
