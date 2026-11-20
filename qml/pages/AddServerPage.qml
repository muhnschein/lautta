// SPDX-License-Identifier: LGPL-2.1-or-later
import QtQuick 2.6
import Sailfish.Silica 1.0
import Lautta 1.0

Page {
    id: page

    property alias listView: list

    objectName: "addServerPage"
    allowedOrientations: Orientation.All

    SilicaListView {
        id: list

        anchors.fill: parent
        // Providers netvfs creates accounts for; names are protocol names, not translated.
        model: ListModel {
            ListElement { provider: "sftp"; name: "SFTP" }
            ListElement { provider: "smb"; name: "SMB" }
            ListElement { provider: "webdav"; name: "WebDAV" }
            ListElement { provider: "ftps"; name: "FTPS" }
        }
        header: PageHeader {
            //% "Add server"
            title: qsTrId("lautta-browse-add-server")
        }
        delegate: ListItem {
            readonly property string provider: model.provider

            contentHeight: Theme.itemSizeSmall
            onClicked: {
                Bridge.addServer(provider)
                pageStack.pop()
            }

            Label {
                x: Theme.horizontalPageMargin
                width: parent.width - 2 * x
                anchors.verticalCenter: parent.verticalCenter
                text: model.name
                color: highlighted ? Theme.highlightColor : Theme.primaryColor
                truncationMode: TruncationMode.Fade
            }
        }

        VerticalScrollDecorator {}
    }
}
