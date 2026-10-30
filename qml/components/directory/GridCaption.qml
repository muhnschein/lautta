// SPDX-License-Identifier: LGPL-2.1-or-later Lines stacked by bindings, not Column: a line that
// comes moves next at once, not next polish.
import QtQuick 2.6
import Sailfish.Silica 1.0

Item {
    id: caption

    property Item metrics
    property string name
    property var captions: []
    property int minLines
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
