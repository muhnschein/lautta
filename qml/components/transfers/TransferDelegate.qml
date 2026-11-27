// SPDX-License-Identifier: LGPL-2.1-or-later
import QtQuick 2.6
import Sailfish.Silica 1.0
import Lautta 1.0
import ".."
import "TransferText.js" as TransferText

ListItem {
    id: item

    property bool restored
    property var row: ({
        transferId: 0, group: "active", dateGroup: "inProgress", kind: "copy", title: "", state: "running",
        waitReason: "", bytesDone: 0, bytesTotal: 0, rate: 0, eta: -1, itemsDone: 0, itemsTotal: 0,
        itemsFailed: 0, direction: "local", destName: "", destUri: "", finishedMs: -1, questions: 0, progress: 0
    })
    readonly property bool finished: row.group === "history"
    readonly property bool hasProgress: (row.group === "active" && row.state !== "scanning") || row.group === "paused"
    readonly property var r: ({
        group: row.group, dateGroup: row.dateGroup, kind: row.kind, state: row.state, title: row.title,
        direction: row.direction, waitReason: row.waitReason, bytesDone: row.bytesDone, bytesTotal: row.bytesTotal,
        rate: row.rate, eta: row.eta, itemsDone: row.itemsDone, itemsTotal: row.itemsTotal,
        itemsFailed: row.itemsFailed, destName: row.destName, finishedMs: row.finishedMs, questions: row.questions
    })
    readonly property var titleParts: TransferText.titleParts(r)
    readonly property bool toServer: TransferText.toServer(row.direction)
    readonly property string line: TransferText.rowLine(r, restored)
    readonly property bool warns: TransferText.rowWarns(r)
    readonly property color warnColor: Theme.presenceColor(Theme.PresenceAway)

    signal showFolder(string uri)

    function cancelLater() {
        var id = row.transferId
        //% "Cancelling"
        remorseAction(qsTrId("lautta-xfr-remorse-cancel"), function() { Transfers.cancel(id) },
                      App.setting("remorse_seconds") * 1000)
    }

    function removeLater() {
        var id = row.transferId
        //% "Removing"
        remorseAction(qsTrId("lautta-xfr-remorse-remove"), function() { Transfers.removeFromHistory(id) },
                      App.setting("remorse_seconds") * 1000)
    }

    contentHeight: Math.max(finished ? Theme.itemSizeSmall : Theme.itemSizeMedium, content.height + 2 * Theme.paddingSmall)
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
                //% "Move to the top of the queue"
                text: qsTrId("lautta-xfr-menu-top")
                visible: !item.finished
                onClicked: Transfers.moveToTop(row.transferId)
            }
            MenuItem {
                //% "Retry failed"
                text: qsTrId("lautta-xfr-menu-retry")
                visible: item.finished && row.state === "failed"
                onClicked: Transfers.retryFailed(row.transferId)
            }
            MenuItem {
                //% "Show in folder"
                text: qsTrId("lautta-xfr-menu-show-folder")
                visible: item.finished && row.destUri.length > 0
                onClicked: item.showFolder(row.destUri)
            }
            MenuItem {
                //% "Remove from Activity"
                text: qsTrId("lautta-xfr-menu-remove")
                visible: item.finished
                onClicked: item.removeLater()
            }
            MenuItem {
                //% "Cancel"
                text: qsTrId("lautta-xfr-menu-cancel")
                visible: !item.finished
                onClicked: item.cancelLater()
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

            anchors.verticalCenter: item.finished ? parent.verticalCenter : undefined
            icon: TransferText.directionIcon(row.direction)
            color: item.finished ? Theme.secondaryColor : Theme.primaryColor
            highlighted: item.highlighted
        }

        Column {
            id: body

            width: parent.width - directionIcon.width - parent.spacing - (time.visible ? time.width + parent.spacing : 0)
            anchors.verticalCenter: parent.verticalCenter

            PlaceLabel {
                objectName: "transferTitle"
                width: parent.width
                before: item.titleParts.before
                text: item.titleParts.rest
                remote: item.titleParts.named && item.toServer
                color: item.highlighted ? Theme.highlightColor : Theme.primaryColor
                fontSize: Theme.fontSizeSmall
                maximumLineCount: 2
            }
            ProgressBar {
                width: parent.width + 2 * Theme.horizontalPageMargin
                x: -Theme.horizontalPageMargin
                visible: item.hasProgress
                highlighted: row.group !== "paused"
                opacity: row.group === "paused" ? Theme.opacityHigh : 1
                height: visible ? implicitHeight : 0
                minimumValue: 0
                maximumValue: 1
                value: row.progress
            }
            Row {
                objectName: "transferLine"
                visible: item.line.length > 0
                width: parent.width
                spacing: Theme.paddingSmall

                AppIcon {
                    id: glyph

                    objectName: "transferGlyph"
                    visible: icon.length > 0
                    icon: TransferText.stateGlyph(item.r)
                    iconSize: Theme.iconSizeExtraSmall
                    color: item.warns ? item.warnColor : Theme.secondaryColor
                    highlighted: item.highlighted && !item.warns
                    anchors.verticalCenter: parent.verticalCenter
                }
                Label {
                    width: parent.width - (glyph.visible ? glyph.width + parent.spacing : 0)
                    text: item.line
                    color: item.warns ? item.warnColor
                                      : item.highlighted ? Theme.secondaryHighlightColor : Theme.secondaryColor
                    font.pixelSize: Theme.fontSizeExtraSmall
                    truncationMode: TruncationMode.Fade
                    anchors.verticalCenter: parent.verticalCenter
                }
            }
        }

        Label {
            id: time

            objectName: "transferTime"
            visible: text.length > 0
            anchors.verticalCenter: parent.verticalCenter
            text: TransferText.rowTime(item.r)
            color: item.highlighted ? Theme.secondaryHighlightColor : Theme.secondaryColor
            font.pixelSize: Theme.fontSizeExtraSmall
        }
    }
}
