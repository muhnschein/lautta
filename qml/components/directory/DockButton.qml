// SPDX-License-Identifier: LGPL-2.1-or-later
import QtQuick 2.6
import Sailfish.Silica 1.0
import "../"

BackgroundItem {
    id: button

    property string icon
    property string description
    property bool accent
    property bool key

    width: Theme.itemSizeMedium
    height: Theme.itemSizeLarge
    Accessible.role: Accessible.Button
    Accessible.name: description

    Rectangle {
        objectName: "dockKey"
        visible: button.key
        anchors.fill: parent
        color: Theme.rgba(Theme.highlightBackgroundColor, Theme.highlightBackgroundOpacity)
    }

    AppIcon {
        anchors.centerIn: parent
        icon: button.icon
        highlighted: button.highlighted || button.accent || button.key
    }
}
