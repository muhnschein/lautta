// SPDX-License-Identifier: LGPL-2.1-or-later
// Undo of the last rename, same-location move or delete, offered for 10 s
// (SPEC OPS-9) as a Silica notice at the bottom of the screen.
import QtQuick 2.6
import Sailfish.Silica 1.0
import Lautta 1.0

Item {
    id: banner

    anchors.fill: parent
    z: 1000

    function message() {
        switch (App.undoText) {
        case "rename":
            //% "Renamed"
            return qsTrId("lautta-undo-renamed")
        case "move":
            //% "Moved"
            return qsTrId("lautta-undo-moved")
        default:
            //% "Moved to Recently deleted"
            return qsTrId("lautta-undo-trashed")
        }
    }

    BackgroundItem {
        id: bar

        visible: opacity > 0
        opacity: App.canUndo ? 1 : 0
        anchors {
            left: parent.left
            right: parent.right
            bottom: parent.bottom
        }
        height: Theme.itemSizeSmall
        highlightedColor: Theme.rgba(Theme.highlightBackgroundColor, Theme.highlightBackgroundOpacity)
        onClicked: App.undo()

        Behavior on opacity { FadeAnimator { } }

        Rectangle {
            anchors.fill: parent
            color: Theme.rgba(Theme.overlayBackgroundColor, 0.9)
        }
        Label {
            anchors {
                left: parent.left
                leftMargin: Theme.horizontalPageMargin
                right: undoLabel.left
                verticalCenter: parent.verticalCenter
            }
            text: banner.message()
            truncationMode: TruncationMode.Fade
        }
        Label {
            id: undoLabel

            anchors {
                right: parent.right
                rightMargin: Theme.horizontalPageMargin
                verticalCenter: parent.verticalCenter
            }
            //% "Undo"
            text: qsTrId("lautta-undo")
            color: bar.highlighted ? Theme.highlightColor : Theme.secondaryHighlightColor
        }
    }
}
