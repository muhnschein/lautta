// SPDX-License-Identifier: LGPL-2.1-or-later
import QtQuick 2.6
import Sailfish.Silica 1.0
import Lautta 1.0
import "../components"
import "../components/transfers/TransferText.js" as TransferText

Item {
    id: root

    // Tests set them.
    property int activeCount: Transfers.activeCount
    property int pausedCount: Transfers.pausedCount
    property int pendingCount: Transfers.pendingCount
    property bool closing: Transfers.closing
    property real bytesDone: Transfers.bytesDone
    property real bytesTotal: Transfers.bytesTotal
    property real rate: Transfers.rate
    // "transferring", "paused", "pending" or "idle".
    readonly property string mode: closing && pendingCount > 0
        ? "pending"
        : (activeCount > 0 ? "transferring" : (pausedCount > 0 ? "paused" : "idle"))
    readonly property bool active: mode !== "idle"
    readonly property int percent: TransferText.percent(bytesDone, bytesTotal)
    readonly property real fraction: bytesTotal > 0 ? Math.min(1, bytesDone / bytesTotal) : 0

    // Whole cover is the progress bar: see-through fill from the left.
    Rectangle {
        objectName: "coverFill"
        visible: root.mode !== "pending"
        width: Math.round(parent.width * root.fraction)
        height: parent.height
        color: root.mode === "paused" ? Theme.rgba(Theme.primaryColor, Theme.opacityFaint)
                                      : Theme.rgba(Theme.highlightBackgroundColor, Theme.opacityLow)
    }

    CoverHeader {
        id: header

        text: root.mode === "pending"
            //% "Lautta"
            ? qsTrId("lautta-app-name")
            : (root.mode === "paused"
               //% "%n paused"
               ? qsTrId("lautta-xfr-header-paused", root.pausedCount)
               //% "%n tasks"
               : qsTrId("lautta-xfr-cover-count", root.activeCount))
    }

    Column {
        objectName: "coverProgress"
        anchors {
            left: parent.left
            right: parent.right
            leftMargin: Theme.paddingMedium
            rightMargin: Theme.paddingMedium
            verticalCenter: parent.verticalCenter
        }
        spacing: Theme.paddingSmall
        visible: root.mode !== "pending"

        Label {
            width: parent.width
            horizontalAlignment: Text.AlignHCenter
            color: Theme.highlightColor
            font.pixelSize: Theme.fontSizeHuge
            font.weight: Font.Light
            text: root.percent + "%"
        }
        Label {
            objectName: "coverRate"
            width: parent.width
            horizontalAlignment: Text.AlignHCenter
            color: Theme.secondaryColor
            font.pixelSize: Theme.fontSizeSmall
            visible: root.rate > 0
            //% "%1/s"
            text: qsTrId("lautta-xfr-rate").arg(TransferText.size(root.rate))
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
            text: TransferText.pendingCover(root.pendingCount, App.setting("auto_resume") !== false)
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
