// SPDX-License-Identifier: LGPL-2.1-or-later
// The sizes of the cells of the grid (SPEC §15.3), the one place for
// DirectoryView and the View options preview so both lay a cell out alike: a
// tile is followed by the name on two lines and two caption lines, and every
// cell has the height for all of them, as the view needs one cell height.
import QtQuick 2.6
import Sailfish.Silica 1.0
import "GridName.js" as GridName
import "../FileIcons.js" as FileIcons

Item {
    id: metrics

    // Tiles per row in portrait; landscape keeps the tile size and fits more.
    readonly property int portraitColumns: 3
    // The space around a tile.
    readonly property real gap: Theme.paddingSmall
    readonly property real nameHeight: 2 * nameFont.lineSpacing
    readonly property real captionHeight: captionFont.lineSpacing
    // Room taken from the name's width: its cut is measured, not laid out
    // by Qt, so a line must not end at the very edge.
    readonly property real nameMargin: Theme.paddingSmall / 2

    function cellHeight(cellWidth) {
        return cellWidth + nameHeight + 2 * captionHeight + Theme.paddingSmall
    }

    // The glyph in a tile: about half the tile, at a multiple of 28 px, where
    // the glyphs' 28-unit set is sharp (FileIcons.js, UI-2).
    function glyphSize(tileWidth) {
        return FileIcons.snap(tileWidth / 2)
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
