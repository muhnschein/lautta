// SPDX-License-Identifier: LGPL-2.1-or-later
// The sizes of the cells of the grid (SPEC §15.3), the one place for
// DirectoryView and the View options preview so both lay a cell out alike: a
// tile, then the lines of the name and the caption lines it has, no empty
// line kept. A row is as tall as its tallest cell (GridRows.js).
import QtQuick 2.6
import Sailfish.Silica 1.0
import "GridName.js" as GridName

Item {
    id: metrics

    // Tiles per row in portrait; landscape keeps the tile size and fits more.
    readonly property int portraitColumns: 3
    // The space around a tile.
    readonly property real gap: Theme.paddingSmall
    // One line of the name, one caption line.
    readonly property real nameLineHeight: nameFont.lineSpacing
    readonly property real captionHeight: captionFont.lineSpacing
    // Room taken from the name's width: its cut is measured, not laid out
    // by Qt, so a line must not end at the very edge.
    readonly property real nameMargin: Theme.paddingSmall / 2

    // A cell `cellWidth` wide whose text under the tile (GridCaption) is
    // `textHeight` high: the tile with the gap around it, the text half a
    // padding under the tile and half a padding and a gap above the cell's end.
    function cellHeight(cellWidth, textHeight) {
        return cellWidth + textHeight + Theme.paddingSmall
    }

    // The theme glyph in a tile: about half the tile.
    function glyphSize(tileWidth) {
        return Math.round(tileWidth / 2)
    }

    // The lines of a name under a tile `tileWidth` wide.
    function nameLines(name, tileWidth) {
        return GridName.lines(name, tileWidth - nameMargin, function (text) { return nameFont.advanceWidth(text) })
    }

    FontMetrics {
        id: nameFont

        font {
            family: Theme.fontFamily
            pixelSize: Theme.fontSizeExtraSmall
        }
    }

    FontMetrics {
        id: captionFont

        font {
            family: Theme.fontFamily
            pixelSize: Theme.fontSizeTiny
        }
    }
}
