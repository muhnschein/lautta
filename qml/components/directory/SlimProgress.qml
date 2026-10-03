// SPDX-License-Identifier: LGPL-2.1-or-later
// The slim progress line while a folder is listed (board Directory): a thin
// highlight bar sweeping across.
import QtQuick 2.6
import Sailfish.Silica 1.0

Item {
    id: line

    property bool running

    width: parent ? parent.width : 0
    height: Theme.paddingSmall / 3
    opacity: running ? 1 : 0
    clip: true

    Behavior on opacity { FadeAnimator { } }

    Rectangle {
        id: bar

        width: line.width / 3
        height: line.height
        color: Theme.highlightColor
        opacity: 0.85

        SequentialAnimation on x {
            running: line.running && line.visible
            loops: Animation.Infinite
            NumberAnimation { from: -bar.width; to: line.width; duration: 1400; easing.type: Easing.InOutQuad }
        }
    }
}
