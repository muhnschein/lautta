// SPDX-License-Identifier: LGPL-2.1-or-later
// An icon-only button of the docked panels (board DirectorySelect). Plain
// IconButton cannot tint the app's own SVG icons, so this wraps AppIcon.
import QtQuick 2.6
import Sailfish.Silica 1.0
import "../"

BackgroundItem {
    id: button

    // Theme icon id or app icon name, see AppIcon.
    property string icon
    // The spoken name of the button.
    property string description
    property bool accent
    // The button that leaves the bar, set apart from the bar's actions on a
    // key of the ambience's highlight background (board DirectorySelect).
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
