// SPDX-License-Identifier: LGPL-2.1-or-later
import QtQuick 2.6
import Sailfish.Silica 1.0

Column {
    id: header

    property alias acceptText: dialogHeader.acceptText
    property string title
    property string subtitle
    property string note
    property bool noteWarns
    property bool tappable

    signal tapped

    width: parent ? parent.width : 0

    DialogHeader {
        id: dialogHeader
    }

    BackgroundItem {
        objectName: "pickTitle"
        width: parent.width
        height: titleColumn.height + Theme.paddingSmall
        enabled: header.tappable
        onClicked: header.tapped()

        Column {
            id: titleColumn

            anchors {
                left: parent.left
                right: parent.right
                margins: Theme.horizontalPageMargin
            }

            Label {
                width: parent.width
                horizontalAlignment: Text.AlignRight
                text: header.title
                color: Theme.highlightColor
                font.pixelSize: Theme.fontSizeLarge
                truncationMode: TruncationMode.Fade
            }
            Label {
                visible: text.length > 0
                width: parent.width
                horizontalAlignment: Text.AlignRight
                text: header.subtitle
                color: Theme.secondaryHighlightColor
                font.pixelSize: Theme.fontSizeExtraSmall
                truncationMode: TruncationMode.Fade
            }
        }
    }

    Label {
        objectName: "pickNote"
        visible: text.length > 0
        x: Theme.horizontalPageMargin
        width: parent.width - 2 * Theme.horizontalPageMargin
        horizontalAlignment: Text.AlignRight
        wrapMode: Text.Wrap
        text: header.note
        color: header.noteWarns ? Theme.errorColor : Theme.secondaryHighlightColor
        font.pixelSize: Theme.fontSizeExtraSmall
    }

    Item {
        width: 1
        height: Theme.paddingSmall
    }
}
