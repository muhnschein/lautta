// SPDX-License-Identifier: LGPL-2.1-or-later
import QtQuick 2.6
import Sailfish.Silica 1.0

Label {
    objectName: "coverHeader"
    anchors {
        top: parent.top
        topMargin: Theme.paddingMedium
        left: parent.left
        leftMargin: Theme.paddingMedium
        right: parent.right
        rightMargin: Theme.paddingMedium
    }
    horizontalAlignment: implicitWidth > width ? Text.AlignLeft : Text.AlignRight
    truncationMode: TruncationMode.Fade
    textFormat: Text.PlainText
    font.pixelSize: Theme.fontSizeSmall
    color: Theme.highlightColor
}
