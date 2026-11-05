// SPDX-License-Identifier: LGPL-2.1-or-later
import QtQuick 2.6
import Sailfish.Silica 1.0
import "../../qml/components"

ApplicationWindow {
    id: test
    property bool reachedEnd

    initialPage: Component { Page { } }

    TransfersHandlers {
        Component.onCompleted: test.reachedEnd = true
    }
}
