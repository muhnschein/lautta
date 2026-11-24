// SPDX-License-Identifier: LGPL-2.1-or-later
import QtQuick 2.6
import Sailfish.Silica 1.0
import "../"
import "../FileIcons.js" as FileIcons

BackgroundItem {
    id: row

    property string text
    property string icon

    objectName: "moreRow"
    height: Theme.itemSizeSmall

    AppIcon {
        id: glyph

        anchors {
            left: parent.left
            leftMargin: Theme.horizontalPageMargin
            verticalCenter: parent.verticalCenter
        }
        icon: FileIcons.sized(row.icon, iconSize)
        highlighted: row.highlighted
    }

    Label {
        anchors {
            left: glyph.right
            leftMargin: Theme.paddingLarge
            right: parent.right
            rightMargin: Theme.horizontalPageMargin
            verticalCenter: parent.verticalCenter
        }
        text: row.text
        truncationMode: TruncationMode.Fade
        color: row.highlighted ? Theme.highlightColor : Theme.primaryColor
    }
}
