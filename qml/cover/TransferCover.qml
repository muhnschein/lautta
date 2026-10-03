// SPDX-License-Identifier: LGPL-2.1-or-later
// The cover while transfers run (SPEC INT-3, board Covers): count, aggregate
// progress and rate, with the actions Pause all / Resume all and Transfers.
// The browse area's CoverPage shows this item instead of its own content
// while `active`, and turns its own CoverActionList off then:
//
//     TransferCover { anchors.fill: parent; visible: active }
//     CoverActionList { enabled: !transferCover.active; ... }
//
// Three states: transferring, paused (Resume all) and, when the app is
// closing with unfinished work, "will resume when you open Lautta again"
// (XFR-6).
import QtQuick 2.6
import Sailfish.Silica 1.0
import Lautta 1.0
import "../components"
import "../components/transfers/TransferText.js" as TransferText

Item {
    id: root

    // "transferring", "paused", "pending" or "idle".
    readonly property string mode: Transfers.closing && Transfers.pendingCount > 0
        ? "pending"
        : (Transfers.activeCount > 0 ? "transferring" : (Transfers.pausedCount > 0 ? "paused" : "idle"))
    readonly property bool active: mode !== "idle"
    readonly property int percent: TransferText.percent(Transfers.bytesDone, Transfers.bytesTotal)

    Column {
        anchors {
            left: parent.left
            right: parent.right
            leftMargin: Theme.paddingLarge
            rightMargin: Theme.paddingLarge
            verticalCenter: parent.verticalCenter
            verticalCenterOffset: -Theme.paddingLarge
        }
        spacing: Theme.paddingSmall
        visible: root.mode !== "pending"

        Label {
            width: parent.width
            horizontalAlignment: Text.AlignHCenter
            color: Theme.secondaryColor
            font.pixelSize: Theme.fontSizeSmall
            text: root.mode === "paused"
                //% "%n paused"
                ? qsTrId("lautta-xfr-header-paused", Transfers.pausedCount)
                //% "%n transfer(s)"
                : qsTrId("lautta-xfr-cover-count", Transfers.activeCount)
        }
        Label {
            width: parent.width
            horizontalAlignment: Text.AlignHCenter
            color: Theme.highlightColor
            font.pixelSize: Theme.fontSizeHuge
            font.weight: Font.Light
            text: root.percent + "%"
        }
        ProgressBar {
            width: parent.width + 2 * Theme.paddingLarge
            x: -Theme.paddingLarge
            minimumValue: 0
            maximumValue: 1
            value: Transfers.bytesTotal > 0 ? Transfers.bytesDone / Transfers.bytesTotal : 0
            leftMargin: Theme.paddingLarge
            rightMargin: Theme.paddingLarge
        }
        Label {
            width: parent.width
            horizontalAlignment: Text.AlignHCenter
            color: Theme.secondaryColor
            font.pixelSize: Theme.fontSizeSmall
            visible: Transfers.rate > 0
            //% "%1/s"
            text: qsTrId("lautta-xfr-rate").arg(TransferText.size(Transfers.rate))
        }
    }

    Column {
        anchors {
            left: parent.left
            right: parent.right
            leftMargin: Theme.paddingLarge
            rightMargin: Theme.paddingLarge
            verticalCenter: parent.verticalCenter
        }
        spacing: Theme.paddingLarge
        visible: root.mode === "pending"

        AppIcon {
            anchors.horizontalCenter: parent.horizontalCenter
            icon: "image://theme/icon-l-play"
        }
        Label {
            width: parent.width
            horizontalAlignment: Text.AlignHCenter
            wrapMode: Text.WordWrap
            //% "%n transfer(s) will resume when you open Lautta again"
            text: qsTrId("lautta-xfr-cover-pending", Transfers.pendingCount)
        }
    }

    CoverActionList {
        enabled: root.mode === "transferring" || root.mode === "paused"

        CoverAction {
            iconSource: root.mode === "paused" ? "image://theme/icon-cover-play" : "image://theme/icon-cover-pause"
            onTriggered: root.mode === "paused" ? Transfers.resumeAll() : Transfers.pauseAll()
        }
        CoverAction {
            iconSource: "image://theme/icon-cover-transfers"
            onTriggered: {
                Transfers.requestShow()
                __silica_applicationwindow_instance.activate()
            }
        }
    }
}
