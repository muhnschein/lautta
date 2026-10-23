// SPDX-License-Identifier: LGPL-2.1-or-later
// A row of the Settings page that opens a page of settings (SPEC §18, laid
// out as Salama's settings): an icon, the page's name and, under it, what
// is chosen there now.
import QtQuick 2.6
import Sailfish.Silica 1.0

BackgroundItem {
    id: entry

    property string iconSource
    property string text
    property string value

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
        // Tinted like ItemIcon: an uncoloured theme glyph vanishes on a light ambience.
        color: Theme.primaryColor
        highlighted: entry.highlighted
        highlightColor: Theme.highlightColor
    }

    Column {
        anchors {
            left: icon.right
            leftMargin: Theme.paddingMedium
            right: parent.right
            rightMargin: Theme.horizontalPageMargin
            verticalCenter: parent.verticalCenter
        }

        Label {
            width: parent.width
            text: entry.text
            truncationMode: TruncationMode.Fade
            color: entry.highlighted ? Theme.highlightColor : Theme.primaryColor
        }
        Label {
            width: parent.width
            visible: text.length > 0
            text: entry.value
            textFormat: Text.PlainText
            truncationMode: TruncationMode.Fade
            font.pixelSize: Theme.fontSizeExtraSmall
            color: entry.highlighted ? Theme.secondaryHighlightColor : Theme.secondaryColor
        }
    }
}
