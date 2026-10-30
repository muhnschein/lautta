// SPDX-License-Identifier: LGPL-2.1-or-later
import QtQuick 2.6
import Sailfish.Silica 1.0
import Lautta 1.0

Page {
    allowedOrientations: Orientation.All

    ViewPlaceholder {
        enabled: true
        //% "Lautta can't open its data"
        text: qsTrId("lautta-start-error")
        hintText: App.startError
    }
}
