// SPDX-License-Identifier: LGPL-2.1-or-later
// The cover (INT-3): the folder's name and item count while browsing; while
// transfers run, the transfers area's cover (progress, rate, count and its
// actions) takes over.
import QtQuick 2.6
import Sailfish.Silica 1.0
import Lautta 1.0

CoverBackground {
    id: cover

    // The folder of the page on top, when it is a folder page.
    readonly property var topPage: typeof pageStack !== "undefined" && pageStack ? pageStack.currentPage : null
    readonly property string folderUri: topPage && typeof topPage.uri === "string" ? topPage.uri : ""
    // Transferring, paused or pending at close: the transfers cover shows.
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

        HighlightImage {
            visible: cover.folderUri.length > 0
            anchors.horizontalCenter: parent.horizontalCenter
            source: "image://theme/icon-m-file-folder"
            width: Theme.iconSizeLarge
            height: width
            sourceSize.width: width
            sourceSize.height: height
            color: Theme.highlightColor
            highlighted: false
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
