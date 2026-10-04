// SPDX-License-Identifier: LGPL-2.1-or-later
// A server that needs the user: its sign-in failed, or it presents another
// identity than before (SPEC §20, NVB-4, NVB-5). The fix is made in netvfs'
// account settings; the button hands the user over.
import QtQuick 2.6
import Sailfish.Silica 1.0
import Lautta 1.0
import "../ErrorText.js" as ErrorText

Page {
    id: page

    property string locationId
    property string serverName
    // "auth-failed" or "server-identity-changed"
    property string attention

    readonly property bool identity: attention === "server-identity-changed"

    allowedOrientations: Orientation.All

    SilicaFlickable {
        anchors.fill: parent

        PageHeader {
            id: header
            title: page.serverName
            description: App.displayAddress("lautta://" + page.locationId + "/")
        }

        ViewPlaceholder {
            enabled: true
            text: page.identity
                  //% "Server identity changed"
                  ? qsTrId("lautta-attention-identity-title")
                  //% "Sign-in failed"
                  : qsTrId("lautta-attention-auth-title")
            hintText: ErrorText.message(page.identity ? "ServerIdentityChanged" : "AuthFailed",
                                        { "location": page.serverName })
        }

        Button {
            anchors {
                horizontalCenter: parent.horizontalCenter
                bottom: parent.bottom
                bottomMargin: parent.height / 4
            }
            text: page.identity
                  //% "Review in Settings"
                  ? qsTrId("lautta-attention-review")
                  //% "Update sign-in"
                  : qsTrId("lautta-attention-update")
            onClicked: Bridge.editAccount("lautta://" + page.locationId + "/")
        }
    }
}
