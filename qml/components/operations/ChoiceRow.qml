// SPDX-License-Identifier: LGPL-2.1-or-later
// One resolution of a conflict: highlighted when selected, dimmed when it
// does not apply to this conflict.
import QtQuick 2.6
import Sailfish.Silica 1.0
import "Json.js" as Json

BackgroundItem {
    id: row

    property string choiceName
    property string title
    property string hint
    property string selected
    property var offeredChoices: []
    readonly property bool available: Json.contains(offeredChoices, choiceName)

    signal chosen(string name)

    height: hintLabel.visible ? Theme.itemSizeMedium : Theme.itemSizeSmall
    enabled: available
    opacity: available ? 1.0 : Theme.opacityLow
    highlighted: down || selected === choiceName
    onClicked: chosen(choiceName)

    Column {
        anchors {
            left: parent.left
            leftMargin: Theme.horizontalPageMargin
            right: parent.right
            rightMargin: Theme.horizontalPageMargin
            verticalCenter: parent.verticalCenter
        }
        Label {
            width: parent.width
            text: row.title
            color: row.highlighted ? Theme.highlightColor : Theme.primaryColor
            truncationMode: TruncationMode.Fade
        }
        Label {
            id: hintLabel
            width: parent.width
            visible: row.hint.length > 0
            text: row.hint
            color: row.highlighted ? Theme.secondaryHighlightColor : Theme.secondaryColor
            font.pixelSize: Theme.fontSizeExtraSmall
            truncationMode: TruncationMode.Fade
        }
    }
}
