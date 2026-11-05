// SPDX-License-Identifier: LGPL-2.1-or-later
import QtQuick 2.6
import Sailfish.Silica 1.0
import Lautta 1.0
import "../../qml/components/browse"

ApplicationWindow {
    id: test
    property bool reachedEnd

    initialPage: Component {
        ServerAttentionPage {
            Component.onCompleted: test.reachedEnd = true
            locationId: "nv-account:1"
            serverName: "NAS"
            attention: "server-identity-changed"
        }
    }
}
