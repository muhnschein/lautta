// SPDX-License-Identifier: LGPL-2.1-or-later
// The icons a favourite can have (ORG-1), laid out like ColourPicker. `icon`
// is the chosen icon name; "" stands for the default icon, which
// `defaultIcon` shows. Tapping an icon emits `picked`, the owner sets `icon`.
import QtQuick 2.6
import Sailfish.Silica 1.0
import "../"
import "../FileIcons.js" as FileIcons

Flow {
    id: picker

    property string icon
    // Icon names (FavouritesModel.iconChoices()).
    property var icons: []
    property string defaultIcon
    // The favourite's colour: the icons are tinted as Browse shows them.
    property string colour
    // Equal cells across the width, as many as fit.
    readonly property int columns: Math.max(1, Math.floor(width / (Theme.iconSizeMedium + 2 * Theme.paddingLarge)))

    signal picked(string icon)

    // As the core's Favourite::icon_source: a file-type name is the app's
    // own glyph (FileIcons.js), any other a theme icon.
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

            // The ring sits around the icon, as around ColourPicker's dot.
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
