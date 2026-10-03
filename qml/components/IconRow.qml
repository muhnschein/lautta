// SPDX-License-Identifier: LGPL-2.1-or-later
// The icon row at the top of busy context menus (design: DirectoryContext):
// the most used actions as icons with a short label, the rest as menu items
// below it. `actions` is a list of { id, icon, text } where icon is a theme
// icon id ("image://theme/icon-m-share") or an app icon file name.
import QtQuick 2.6
import Sailfish.Silica 1.0

Row {
    id: row

    property var actions: []
    // The ContextMenu to close after a tap.
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
