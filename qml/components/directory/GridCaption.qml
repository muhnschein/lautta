// SPDX-License-Identifier: LGPL-2.1-or-later
// What stands under a tile of the grid (SPEC §15.3): the name on
// up to two lines, the middle of a long one cut so that its end stays
// (GridName.js), then right under it the caption lines, small and in the
// secondary colour (DirFormat.gridCaptions). As high as the lines it has,
// no empty line kept: the row is as tall as its tallest cell (GridRows.js).
// Lines stacked by bindings, not a Column: a line that comes moves the next
// at once, not at the next polish.
import QtQuick 2.6
import Sailfish.Silica 1.0

Item {
    id: caption

    property Item metrics
    property string name
    // At most two lines of text.
    property var captions: []
    // Caption lines taken at the least: a folder's count still to come keeps
    // its line, so the row does not grow when the count comes.
    property int minLines
    // The name in the highlight colour, for a pressed or selected cell.
    property bool emphasised
    property bool pressed
    readonly property var nameLines: visible && metrics ? metrics.nameLines(name, width) : []
    readonly property int captionLines: Math.max(captions.length, minLines)

    implicitHeight: metrics ? nameLines.length * metrics.nameLineHeight + captionLines * metrics.captionHeight : 0

    Label {
        id: nameLabel

        objectName: "gridName"

        width: caption.width
        height: caption.metrics ? caption.nameLines.length * caption.metrics.nameLineHeight : 0
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
        model: caption.captionLines

        Label {
            objectName: "gridCaption" + index

            y: nameLabel.height + index * height
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
