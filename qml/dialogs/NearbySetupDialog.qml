// SPDX-License-Identifier: LGPL-2.1-or-later
import QtQuick 2.6
import Sailfish.Silica 1.0
import Lautta 1.0

Dialog {
    id: dialog

    // Account provider of the server found nearby: `sftp`, `smb`, …
    property string provider

    allowedOrientations: Orientation.All

    onAccepted: Bridge.addServer(provider)

    SilicaFlickable {
        anchors.fill: parent
        contentHeight: column.height + Theme.paddingLarge

        Column {
            id: column
            width: parent.width

            DialogHeader {
                //% "Set up"
                acceptText: qsTrId("lautta-nearby-set-up")
            }

            Label {
                x: Theme.horizontalPageMargin
                width: parent.width - 2 * x
                font.pixelSize: Theme.fontSizeLarge
                font.family: Theme.fontFamilyHeading
                color: Theme.highlightColor
                wrapMode: Text.Wrap
                //% "This server isn't set up yet. Proceed to add it as an account in Settings › Accounts. After that, you can access it in Lautta."
                text: qsTrId("lautta-nearby-set-up-hint")
            }
        }
    }
}
