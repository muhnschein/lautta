// SPDX-License-Identifier: LGPL-2.1-or-later
// How a Browse row's connection or mount stands, as one icon (SPEC §15.2):
// the glowing dot of a TextSwitch when connected or mounted, a busy indicator
// while connecting, an empty ring when not connected or not mounted, a ring
// in the error colour when connecting failed, a warning icon when sign-in or
// identity needs the user. The icons sit at the right edge of this item, so
// that the row puts it at the page margin. No text (RowStatus.js has the
// states).
import QtQuick 2.6
import Sailfish.Silica 1.0

Item {
    id: status

    // "on", "busy", "off", "failed" or "warning"; "" shows nothing.
    property string indicator
    // The row is pressed: the ring takes the pressed secondary colour.
    property bool highlighted

    // Across of the dot's core and of the ring.
    readonly property real dotSize: Theme.paddingMedium

    width: Theme.iconSizeExtraSmall
    height: Theme.iconSizeExtraSmall
    visible: indicator !== ""

    // Silica's own glass item, as TextSwitch uses it. Its radius is the
    // core's share of its width, so the item is as wide as the dot needs;
    // the glow reaches past the core by the same share.
    GlassItem {
        objectName: "statusOn"
        visible: status.indicator === "on"
        width: status.dotSize / (2 * radius)
        height: width
        x: parent.width - (status.dotSize + width) / 2
        anchors.verticalCenter: parent.verticalCenter
        dimmed: false
        color: Theme.highlightColor
        backgroundColor: Theme.backgroundGlowColor
    }

    Rectangle {
        objectName: "statusRing"
        visible: status.indicator === "off" || status.indicator === "failed"
        anchors {
            right: parent.right
            verticalCenter: parent.verticalCenter
        }
        width: status.dotSize
        height: width
        radius: width / 2
        color: "transparent"
        border {
            width: Math.max(1, Math.round(Theme.paddingSmall / 4))
            color: status.indicator === "failed" ? Theme.errorColor
                   : (status.highlighted ? Theme.secondaryHighlightColor : Theme.secondaryColor)
        }
    }

    BusyIndicator {
        objectName: "statusBusy"
        visible: status.indicator === "busy"
        running: visible
        anchors {
            right: parent.right
            verticalCenter: parent.verticalCenter
        }
        size: BusyIndicatorSize.ExtraSmall
    }

    HighlightImage {
        objectName: "statusWarning"
        visible: status.indicator === "warning"
        anchors {
            right: parent.right
            verticalCenter: parent.verticalCenter
        }
        source: "image://theme/icon-s-warning"
        width: Theme.iconSizeExtraSmall
        height: width
        sourceSize.width: width
        sourceSize.height: height
        color: Theme.errorColor
    }
}
