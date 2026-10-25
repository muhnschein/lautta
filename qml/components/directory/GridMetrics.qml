// SPDX-License-Identifier: LGPL-2.1-or-later
// The sizes of the cells of the grid (SPEC §15.3), the one place for
// DirectoryView and the View options preview so both lay a cell out alike:
// at 3 per row a tile is followed by the name on two lines and two caption
// lines, and every cell has the height for all of them, as the view needs
// one cell height. At 4 per row a cell is the tile with the name inside.
import QtQuick 2.6
import Sailfish.Silica 1.0
import "GridName.js" as GridName

Item {
    id: metrics

    readonly property real nameHeight: 2 * nameFont.lineSpacing
    readonly property real captionHeight: captionFont.lineSpacing
    // Room taken from the name's width: its cut is measured, not laid out
    // by Qt, so a line must not end at the very edge.
    readonly property real nameMargin: Theme.paddingSmall / 2

    // The space around a tile.
    function gap(captioned) {
        return captioned ? Theme.paddingSmall : Math.round(Theme.paddingSmall / 4)
    }

    function cellHeight(cellWidth, captioned) {
        return captioned ? cellWidth + nameHeight + 2 * captionHeight + Theme.paddingSmall : cellWidth
    }

    // The theme glyph in a tile: large at 3 per row, about half the tile.
    function glyphSize(tileWidth, captioned) {
        return captioned ? Math.round(tileWidth / 2) : Theme.iconSizeMedium
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
