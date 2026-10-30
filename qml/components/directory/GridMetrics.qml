// SPDX-License-Identifier: LGPL-2.1-or-later
import QtQuick 2.6
import Sailfish.Silica 1.0
import "GridName.js" as GridName
import "../FileIcons.js" as FileIcons

Item {
    id: metrics

    readonly property int portraitColumns: 3
    readonly property real gap: Theme.paddingSmall
    readonly property real nameLineHeight: nameFont.lineSpacing
    readonly property real captionHeight: captionFont.lineSpacing
    // Cut is measured, not laid out by Qt: a line must not end at the very edge.
    readonly property real nameMargin: Theme.paddingSmall / 2

    function cellHeight(cellWidth, textHeight) {
        return cellWidth + textHeight + Theme.paddingSmall
    }

    function glyphSize(tileWidth) {
        return FileIcons.snap(tileWidth / 2)
    }

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
