// SPDX-License-Identifier: LGPL-2.1-or-later
import QtQuick 2.6
import Sailfish.Silica 1.0

Item {
    id: status

    property string indicator
    property bool highlighted

    readonly property real dotSize: Theme.paddingMedium
    // Silica's orange (Away presence): reads on dark and light ambience.
    readonly property color warningColor: Theme.presenceColor(Theme.PresenceAway)
    property bool appActive: Qt.application.state === Qt.ApplicationActive
    // Pulse only while row busy, page on screen, app in front. Same binding as Silica's
    // BusyIndicator.
    readonly property bool pulsing: indicator === "busy" && visible && appActive

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
