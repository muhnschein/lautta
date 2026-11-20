// SPDX-License-Identifier: LGPL-2.1-or-later
import QtQuick 2.6
import Sailfish.Silica 1.0

BackgroundItem {
    id: entry

    property string iconSource
    property string text

    width: parent ? parent.width : 0
    height: Theme.itemSizeMedium

    HighlightImage {
        id: icon

        anchors {
            left: parent.left
            leftMargin: Theme.horizontalPageMargin
            verticalCenter: parent.verticalCenter
        }
        width: Theme.iconSizeMedium
        height: width
        sourceSize.width: width
        sourceSize.height: height
        source: entry.iconSource
        color: Theme.primaryColor
        highlighted: entry.highlighted
        highlightColor: Theme.highlightColor
    }

    Label {
        anchors {
            left: icon.right
            leftMargin: Theme.paddingMedium
            right: parent.right
            rightMargin: Theme.horizontalPageMargin
            verticalCenter: parent.verticalCenter
        }
        text: entry.text
        truncationMode: TruncationMode.Fade
        color: entry.highlighted ? Theme.highlightColor : Theme.primaryColor
    }
}
