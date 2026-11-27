// SPDX-License-Identifier: LGPL-2.1-or-later
import QtQuick 2.6
import Sailfish.Silica 1.0
import "../"
import "../FileIcons.js" as FileIcons

Flow {
    id: picker

    property string icon
    property var icons: []
    property string defaultIcon
    property string colour
    readonly property int columns: Math.max(1, Math.floor(width / (Theme.iconSizeMedium + 2 * Theme.paddingLarge)))

    signal picked(string icon)

    function sourceOf(name) {
        if (name.length === 0)
            return defaultIcon
        return name.indexOf("file-") === 0 ? name : "image://theme/icon-m-" + name
    }

    x: Theme.horizontalPageMargin
    width: parent ? parent.width - 2 * Theme.horizontalPageMargin : implicitWidth

    Repeater {
        model: picker.icons

        MouseArea {
            id: cell

            readonly property string name: modelData
            readonly property bool chosen: picker.icon === modelData

            width: picker.width / picker.columns
            height: Theme.itemSizeMedium
            onClicked: picker.picked(modelData)

            Rectangle {
                anchors.centerIn: parent
                visible: cell.chosen
                width: Theme.iconSizeMedium + Theme.paddingLarge
                height: width
                radius: width / 2
                color: "transparent"
                border.width: Theme.paddingSmall / 2
                border.color: Theme.highlightColor
            }
            AppIcon {
                anchors.centerIn: parent
                icon: FileIcons.sized(picker.sourceOf(modelData), iconSize)
                color: picker.colour.length > 0 ? picker.colour : Theme.primaryColor
                highlighted: cell.pressed
            }
        }
    }
}
