// SPDX-License-Identifier: LGPL-2.1-or-later
// A row of colour dots (favourites and tags, ORG-1, ORG-3). `colour` is the
// chosen one ("#rrggbb"); tapping a dot sets it and emits `picked`.
import QtQuick 2.6
import Sailfish.Silica 1.0
import "BrowseText.js" as BrowseText

Flow {
    id: picker

    property string colour
    property var colours: BrowseText.colours
    // Tapping the chosen colour again clears it.
    property bool allowNone: true
    readonly property real dot: Theme.iconSizeSmall + Theme.paddingSmall

    signal picked(string colour)

    x: Theme.horizontalPageMargin
    width: parent ? parent.width - 2 * Theme.horizontalPageMargin : implicitWidth
    spacing: Theme.paddingLarge

    Repeater {
        model: picker.colours

        MouseArea {
            width: picker.dot + Theme.paddingSmall
            height: Theme.itemSizeSmall
            onClicked: {
                picker.colour = (picker.allowNone && picker.colour === modelData) ? "" : modelData
                picker.picked(picker.colour)
            }

            Rectangle {
                anchors.centerIn: parent
                width: picker.dot
                height: width
                radius: width / 2
                color: modelData
                // The ring sits outside the dot so the colour stays readable.
                Rectangle {
                    anchors.centerIn: parent
                    visible: picker.colour === modelData
                    width: parent.width + Theme.paddingSmall * 2
                    height: width
                    radius: width / 2
                    color: "transparent"
                    border.width: Theme.paddingSmall / 2
                    border.color: Theme.highlightColor
                }
            }
        }
    }
}
