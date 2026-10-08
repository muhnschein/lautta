// SPDX-License-Identifier: LGPL-2.1-or-later
// One row of the Transfers page: direction icon, title, progress and status
// (SPEC §15.4), with a context menu for the actions that fit its state.
// Used inside a ListView over TransfersModel (roles are in scope).
import QtQuick 2.6
import Sailfish.Silica 1.0
import Lautta 1.0
import ".."
import "TransferText.js" as TransferText

ListItem {
    id: item

    // Rows of the restored-at-start prompt say where they resume from.
    property bool restored
    // One TransfersModel row (the delegate's `model`).
    property var row: ({
        transferId: 0, group: "active", kind: "copy", title: "", state: "running", waitReason: "",
        bytesDone: 0, bytesTotal: 0, rate: 0, eta: -1, itemsDone: 0, itemsTotal: 0, itemsFailed: 0,
        direction: "local", destName: "", finishedMs: -1, questions: 0, progress: 0
    })
    readonly property bool hasProgress: (row.group === "active" && row.state !== "scanning") || row.group === "paused"
    readonly property var r: ({
        group: row.group, kind: row.kind, state: row.state, title: row.title, direction: row.direction,
        waitReason: row.waitReason, bytesDone: row.bytesDone, bytesTotal: row.bytesTotal, rate: row.rate,
        eta: row.eta, itemsDone: row.itemsDone, itemsTotal: row.itemsTotal, itemsFailed: row.itemsFailed,
        destName: row.destName, finishedMs: row.finishedMs
    })

    contentHeight: Math.max(Theme.itemSizeMedium, content.height + 2 * Theme.paddingSmall)
    menu: Component {
        ContextMenu {
            MenuItem {
                //% "Pause"
                text: qsTrId("lautta-xfr-menu-pause")
                visible: row.group === "active" || (row.group === "waiting" && row.waitReason !== "question")
                onClicked: Transfers.pause(row.transferId)
            }
            MenuItem {
                //% "Resume"
                text: qsTrId("lautta-xfr-menu-resume")
                visible: row.group === "paused"
                onClicked: Transfers.resume(row.transferId)
            }
            MenuItem {
                //% "Move to top of the queue"
                text: qsTrId("lautta-xfr-menu-top")
                visible: row.group !== "history"
                onClicked: Transfers.moveToTop(row.transferId)
            }
            MenuItem {
                //% "Retry failed"
                text: qsTrId("lautta-xfr-menu-retry")
                visible: row.group === "history" && row.state === "failed"
                onClicked: Transfers.retryFailed(row.transferId)
            }
            MenuItem {
                //% "Cancel transfer"
                text: qsTrId("lautta-xfr-menu-cancel")
                visible: row.group !== "history"
                onClicked: item.remorseAction(
                    //% "Canceling transfer"
                    qsTrId("lautta-xfr-remorse-cancel"),
                    function() { Transfers.cancel(row.transferId) },
                    App.setting("remorse_seconds") * 1000)
            }
        }
    }

    Row {
        id: content

        x: Theme.horizontalPageMargin
        width: parent.width - 2 * Theme.horizontalPageMargin
        anchors.verticalCenter: parent.verticalCenter
        spacing: Theme.paddingMedium

        AppIcon {
            id: directionIcon

            anchors.top: parent.top
            anchors.topMargin: Theme.paddingSmall
            icon: TransferText.directionIcon(row.direction)
            highlighted: item.highlighted
        }

        Column {
            width: parent.width - directionIcon.width - parent.spacing

            Label {
                width: parent.width
                text: TransferText.title(item.r)
                color: item.highlighted ? Theme.highlightColor : Theme.primaryColor
                truncationMode: TruncationMode.Fade
            }
            ProgressBar {
                width: parent.width + 2 * Theme.horizontalPageMargin
                x: -Theme.horizontalPageMargin
                visible: item.hasProgress
                height: visible ? implicitHeight : 0
                minimumValue: 0
                maximumValue: 1
                value: row.progress
            }
            Label {
                width: parent.width
                visible: row.questions === 0
                text: TransferText.status(item.r, item.restored)
                color: item.highlighted ? Theme.secondaryHighlightColor : Theme.secondaryColor
                font.pixelSize: Theme.fontSizeSmall
                truncationMode: TruncationMode.Fade
            }
            Row {
                visible: row.questions > 0
                spacing: Theme.paddingSmall

                AppIcon {
                    icon: "image://theme/icon-s-warning"
                    small: true
                    anchors.verticalCenter: parent.verticalCenter
                }
                Label {
                    text: TransferText.needsAnswer(row.questions)
                    color: Theme.highlightColor
                    font.pixelSize: Theme.fontSizeExtraSmall
                    anchors.verticalCenter: parent.verticalCenter
                }
            }
            Label {
                visible: row.group === "history" && row.state === "failed" && row.itemsFailed > 0
                text: qsTrId("lautta-xfr-menu-retry")
                color: Theme.highlightColor
                font.pixelSize: Theme.fontSizeSmall
                height: visible ? implicitHeight : 0

                MouseArea {
                    anchors.fill: parent
                    anchors.margins: -Theme.paddingMedium
                    onClicked: Transfers.retryFailed(row.transferId)
                }
            }
        }
    }
}
