// SPDX-License-Identifier: LGPL-2.1-or-later
// What stands under a tile of the grid (SPEC §15.3): the name on
// up to two lines, the middle of a long one cut so that its end stays
// (GridName.js), then the caption lines, small and in the secondary colour
// (DirFormat.gridCaptions). It always has the height of GridMetrics, so
// that the cells of a row are as tall as the tallest.
import QtQuick 2.6
import Sailfish.Silica 1.0

Column {
    id: caption

    property Item metrics
    property string name
    // At most two lines of text.
    property var captions: []
    // The name in the highlight colour, for a pressed or selected cell.
    property bool emphasised
    property bool pressed
    readonly property var nameLines: visible && metrics ? metrics.nameLines(name, width) : []

    Label {
        objectName: "gridName"

        width: caption.width
        height: caption.metrics ? caption.metrics.nameHeight : 0
        horizontalAlignment: Text.AlignHCenter
        verticalAlignment: Text.AlignTop
        wrapMode: Text.NoWrap
        // A name is text, never markup.
        textFormat: Text.PlainText
        text: caption.nameLines.join("\n")
        font.pixelSize: Theme.fontSizeExtraSmall
        color: caption.emphasised ? Theme.highlightColor : Theme.primaryColor
    }
    Repeater {
        model: 2

        Label {
            objectName: "gridCaption" + index

            width: caption.width
            height: caption.metrics ? caption.metrics.captionHeight : 0
            horizontalAlignment: Text.AlignHCenter
            text: index < caption.captions.length ? caption.captions[index] : ""
            truncationMode: TruncationMode.Elide
            font.pixelSize: Theme.fontSizeTiny
            color: caption.pressed ? Theme.secondaryHighlightColor : Theme.secondaryColor
        }
    }
}
