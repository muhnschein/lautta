// SPDX-License-Identifier: LGPL-2.1-or-later
import QtQuick 2.6
import Sailfish.Silica 1.0
import "../../qml/pages"

ApplicationWindow {
    id: test
    property bool reachedEnd

    initialPage: Component {
        TransferDetailsPage {
            transferId: 1
            Component.onCompleted: test.reachedEnd = true
        }
    }
}
