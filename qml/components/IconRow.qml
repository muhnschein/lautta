// SPDX-License-Identifier: LGPL-2.1-or-later
import QtQuick 2.6
import Sailfish.Silica 1.0

Row {
    id: row

    property var actions: []
    property Item menu

    signal triggered(string actionId)

    width: parent ? parent.width : 0
    height: Theme.itemSizeLarge

    Repeater {
        model: row.actions

        BackgroundItem {
            id: button

            width: row.width / Math.max(1, row.actions.length)
            height: row.height
            onClicked: {
                row.triggered(modelData.id)
                if (row.menu)
                    row.menu.close()
            }

            Column {
                anchors.centerIn: parent
                spacing: Theme.paddingSmall / 2

                AppIcon {
                    anchors.horizontalCenter: parent.horizontalCenter
                    icon: modelData.icon
                    highlighted: button.highlighted
                }
                Label {
                    anchors.horizontalCenter: parent.horizontalCenter
                    width: button.width - Theme.paddingSmall
                    horizontalAlignment: Text.AlignHCenter
                    text: modelData.text
                    font.pixelSize: Theme.fontSizeTiny
                    truncationMode: TruncationMode.Fade
                    color: button.highlighted ? Theme.highlightColor : Theme.secondaryColor
                }
            }
        }
    }
}
