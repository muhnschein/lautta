// SPDX-License-Identifier: LGPL-2.1-or-later
import QtQuick 2.6
import Sailfish.Silica 1.0
import Lautta 1.0
import "../components"
import "../components/FileIcons.js" as FileIcons

CoverBackground {
    id: cover

    readonly property var topPage: typeof pageStack !== "undefined" && pageStack ? pageStack.currentPage : null
    readonly property string folderUri: topPage && typeof topPage.uri === "string" ? topPage.uri : ""
    readonly property bool transferring: transferCover.active

    onStatusChanged: if (status === Cover.Active) info.refresh()

    FolderInfo {
        id: info
        uri: cover.folderUri
    }

    TransferCover {
        id: transferCover
        anchors.fill: parent
        visible: active
    }

    Column {
        visible: !cover.transferring
        anchors {
            left: parent.left
            right: parent.right
            margins: Theme.paddingLarge
            verticalCenter: parent.verticalCenter
        }
        spacing: Theme.paddingSmall

        AppIcon {
            visible: cover.folderUri.length > 0
            anchors.horizontalCenter: parent.horizontalCenter
            // Multiple of 28 px: glyph's sharp set.
            iconSize: FileIcons.snap(Theme.iconSizeLarge)
            icon: FileIcons.glyph("folder", true, iconSize)
            color: Theme.highlightColor
        }
        Label {
            width: parent.width
            horizontalAlignment: Text.AlignHCenter
            wrapMode: Text.Wrap
            maximumLineCount: 3
            elide: Text.ElideRight
            text: cover.folderUri.length > 0
                  ? App.nameOf(cover.folderUri)
                  //% "Lautta"
                  : qsTrId("lautta-app-name")
            font.pixelSize: Theme.fontSizeLarge
            color: Theme.highlightColor
        }
        Label {
            visible: cover.folderUri.length > 0 && info.count >= 0
            width: parent.width
            horizontalAlignment: Text.AlignHCenter
            //% "%n items"
            text: qsTrId("lautta-cover-items", info.count)
            font.pixelSize: Theme.fontSizeSmall
            color: Theme.secondaryColor
        }
    }
}
