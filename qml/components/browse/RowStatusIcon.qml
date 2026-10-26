// SPDX-License-Identifier: LGPL-2.1-or-later
// How a Browse row's connection or mount stands (SPEC §15.2): one glowing
// dot, as TextSwitch draws. Highlight colour = connected, mounted. Primary
// colour (white on dark ambience, dark on light) = not connected, not
// mounted. Same dot pulsing = connecting, mounting, unmounting. Red =
// connecting failed. Orange = sign-in or identity needs user. Locked drive:
// Silica's padlock instead, as the WLAN list marks a secured network (LOC-3).
// Dot or padlock sits at right edge of this item; row puts item at page
// margin. No text (RowStatus.js has the states).
import QtQuick 2.6
import Sailfish.Silica 1.0

Item {
    id: status

    // "on", "busy", "off", "failed", "warning" or "locked"; "" shows nothing.
    property string indicator
    // Row pressed: dot of primary colour takes highlight colour, as row
    // labels do.
    property bool highlighted

    // Across of dot core.
    readonly property real dotSize: Theme.paddingMedium
    // Silica's orange: colour of Away presence. Fixed, reads on dark and
    // light ambience.
    readonly property color warningColor: Theme.presenceColor(Theme.PresenceAway)
    // App in front.
    property bool appActive: Qt.application.state === Qt.ApplicationActive
    // Pulse only while row busy, its page on screen and app in front.
    // Background: nothing animates (PRF-8). Silica's BusyIndicator binds the
    // same way.
    readonly property bool pulsing: indicator === "busy" && visible && appActive

    // Brightness while pulsing. Animation moves it; dot reads it only while
    // pulsing, so a stopped pulse leaves full brightness.
    property real glow: 1.0

    readonly property bool locked: indicator === "locked"

    width: locked ? lock.width : Theme.iconSizeExtraSmall
    height: locked ? lock.height : Theme.iconSizeExtraSmall
    visible: indicator !== ""

    HighlightImage {
        id: lock
        objectName: "statusLock"
        visible: status.locked
        anchors.verticalCenter: parent.verticalCenter
        source: status.locked ? "image://theme/icon-s-secure" : ""
        color: Theme.primaryColor
        highlighted: status.highlighted
    }

    // Silica's glass item, as TextSwitch uses it. Its radius is the core's
    // share of its width, so item is as wide as dot needs; glow reaches past
    // core by same share.
    GlassItem {
        objectName: "statusDot"
        visible: !status.locked
        width: status.dotSize / (2 * radius)
        height: width
        x: parent.width - (status.dotSize + width) / 2
        anchors.verticalCenter: parent.verticalCenter
        opacity: status.pulsing ? status.glow : 1.0
        dimmed: false
        color: {
            switch (status.indicator) {
            case "on":
                return Theme.highlightColor
            case "failed":
                return Theme.errorColor
            case "warning":
                return status.warningColor
            default:
                return status.highlighted ? Theme.highlightColor : Theme.primaryColor
            }
        }
        backgroundColor: Theme.backgroundGlowColor
    }

    SequentialAnimation {
        objectName: "statusPulse"
        running: status.pulsing
        loops: Animation.Infinite

        NumberAnimation {
            target: status
            property: "glow"
            to: Theme.opacityLow
            duration: 700
            easing.type: Easing.InOutQuad
        }
        NumberAnimation {
            target: status
            property: "glow"
            to: 1.0
            duration: 700
            easing.type: Easing.InOutQuad
        }
    }
}
