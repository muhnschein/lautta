// SPDX-License-Identifier: LGPL-2.1-or-later
import QtQuick 2.6
import Sailfish.Silica 1.0

Rectangle {
    id: root

    property bool shown
    property string current
    property var choices: []

    signal chosen(string value)

    function labelOf(value) {
        for (var i = 0; i < choices.length; ++i) {
            if (choices[i].value === value)
                return choices[i].label
        }
        return ""
    }

    width: parent ? parent.width : 0
    height: shown ? column.height + 2 * Theme.paddingSmall : 0
    visible: shown
    color: Theme.rgba(Theme.highlightBackgroundColor, Theme.highlightBackgroundOpacity / 2)

    Column {
        id: column

        y: Theme.paddingSmall
        width: parent.width

        Repeater {
            model: root.choices

            BackgroundItem {
                objectName: "pillChoice"
                readonly property string value: modelData.value
                width: column.width
                height: Theme.itemSizeExtraSmall
                onClicked: root.chosen(modelData.value)

                Label {
                    anchors.centerIn: parent
                    width: Math.min(implicitWidth, parent.width - 2 * Theme.horizontalPageMargin)
                    text: modelData.label
                    textFormat: Text.PlainText
                    truncationMode: TruncationMode.Fade
                    color: parent.highlighted || modelData.value === root.current
                           ? Theme.highlightColor : Theme.primaryColor
                }
            }
        }
    }
}
