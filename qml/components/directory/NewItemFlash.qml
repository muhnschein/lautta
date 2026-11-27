// SPDX-License-Identifier: LGPL-2.1-or-later
import QtQuick 2.6
import Sailfish.Silica 1.0

Rectangle {
    id: flash

    property Item owner
    property string uri
    readonly property bool running: fade.running

    color: Theme.rgba(Theme.highlightBackgroundColor, Theme.highlightBackgroundOpacity)
    opacity: 0
    visible: opacity > 0

    Connections {
        target: flash.owner
        ignoreUnknownSignals: true
        onFlashed: if (itemUri === flash.uri) fade.restart()
    }

    SequentialAnimation {
        id: fade

        FadeAnimation { target: flash; to: 1; duration: 200 }
        PauseAnimation { duration: 600 }
        FadeAnimation { target: flash; to: 0; duration: 800 }
    }
}
