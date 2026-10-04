// SPDX-License-Identifier: LGPL-2.1-or-later
// An in-page notice with one action (boards StateOffline, StateLarge,
// DirectoryGrid): text on the left, the action as a highlighted word.
import QtQuick 2.6
import Sailfish.Silica 1.0

BackgroundItem {
    id: banner

    property alias text: message.text
    property string actionText

    signal action

    width: parent ? parent.width : 0
    height: Math.max(Theme.itemSizeSmall, row.height + 2 * Theme.paddingMedium)
    highlightedColor: Theme.rgba(Theme.highlightBackgroundColor, Theme.highlightBackgroundOpacity)
    enabled: actionText.length > 0
    onClicked: action()

    Rectangle {
        anchors.fill: parent
        color: Theme.rgba(Theme.highlightBackgroundColor, Theme.highlightBackgroundOpacity / 2)
    }

    Row {
        id: row

        x: Theme.horizontalPageMargin
        width: parent.width - 2 * Theme.horizontalPageMargin
        anchors.verticalCenter: parent.verticalCenter
        spacing: Theme.paddingLarge

        Label {
            id: message

            width: row.width - (actionLabel.visible ? actionLabel.width + row.spacing : 0)
            wrapMode: Text.Wrap
            font.pixelSize: Theme.fontSizeSmall
            color: Theme.primaryColor
        }
        Label {
            id: actionLabel

            visible: banner.actionText.length > 0
            anchors.verticalCenter: parent.verticalCenter
            text: banner.actionText
            font.pixelSize: Theme.fontSizeSmall
            color: banner.highlighted ? Theme.primaryColor : Theme.highlightColor
        }
    }
}
