// SPDX-License-Identifier: LGPL-2.1-or-later
import QtQuick 2.6
import Sailfish.Silica 1.0

// Row's or tile's brief highlight when owner says `flashed(uri)` for this item.
Rectangle {
    id: flash

    property var owner
    property string uri
    readonly property bool running: fade.running

    color: Theme.rgba(Theme.highlightBackgroundColor, Theme.highlightBackgroundOpacity)
    opacity: 0
    visible: opacity > 0

    Connections {
        target: flash.owner
        onFlashed: if (itemUri === flash.uri) fade.restart()
    }

    SequentialAnimation {
        id: fade

        FadeAnimation { target: flash; to: 1; duration: 200 }
        PauseAnimation { duration: 600 }
        FadeAnimation { target: flash; to: 0; duration: 800 }
    }
}
