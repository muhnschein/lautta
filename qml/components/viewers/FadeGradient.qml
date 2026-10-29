// SPDX-License-Identifier: LGPL-2.1-or-later
// The dark fade behind the image viewer's controls (PRV-4) as Jolla
// Gallery's overlay has it (Silica's own is private, not a Harbour import):
// dark over the fifth nearest the screen's edge, clear towards the image.
import QtQuick 2.6
import Sailfish.Silica 1.0

Rectangle {
    id: fade

    property real fadeOpacity: Theme.opacityHigh
    // The screen's edge is at the top (true) or at the bottom.
    property bool topDown
    readonly property color dark: Theme.rgba("black", fadeOpacity)
    readonly property color clear: Theme.rgba("black", 0.0)

    color: "transparent"
    gradient: Gradient {
        GradientStop { position: 0.0; color: fade.topDown ? fade.dark : fade.clear }
        GradientStop { position: fade.topDown ? 0.2 : 0.8; color: fade.dark }
        GradientStop { position: 1.0; color: fade.topDown ? fade.clear : fade.dark }
    }
}
