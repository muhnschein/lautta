// SPDX-License-Identifier: LGPL-2.1-or-later
// One transfer: progress, where it goes, and its items (SPEC §11, board
// TransferDetails). Item failures show the translated error (SPEC §20).
import QtQuick 2.6
import Sailfish.Silica 1.0
import Lautta 1.0
import "../components"
import "../components/ErrorText.js" as ErrorText
import "../components/transfers/TransferText.js" as TransferText

Page {
    id: page

    property int transferId
    // The header numbers; null when the transfer no longer exists.
    readonly property var s: items.summaryJson.length > 0 ? JSON.parse(items.summaryJson) : null
    readonly property bool running: s !== null && (s.state === "running" || s.state === "scanning" || s.state === "queued")
    readonly property bool finished: s !== null && (s.state === "completed" || s.state === "failed" || s.state === "canceled")

    allowedOrientations: Orientation.All

    TransferItemsModel {
        id: items

        transferId: page.transferId
    }

    SilicaListView {
        id: list

        anchors.fill: parent
        model: items

        header: Column {
            width: list.width

            PageHeader {
                title: page.s ? TransferText.title({
                    kind: page.s.kind, direction: page.s.direction, state: page.s.state,
                    title: page.s.title, itemsTotal: page.s.itemsTotal, destName: page.s.destName
                }) : ""
                description: page.s && page.s.sourceName
                    //% "From %1"
                    ? qsTrId("lautta-xfr-from").arg(page.s.sourceAddress) : ""
            }
            ProgressBar {
                width: parent.width
                visible: page.s !== null
                minimumValue: 0
                maximumValue: 1
                value: page.s && page.s.bytesTotal > 0 ? page.s.bytesDone / page.s.bytesTotal : 0
                indeterminate: page.s !== null && page.s.state === "scanning"
                label: page.s ? TransferText.etaLine(page.s.rate, page.s.eta) : ""
            }
            Label {
                x: Theme.horizontalPageMargin
                width: parent.width - 2 * Theme.horizontalPageMargin
                visible: page.s !== null
                text: page.s ? TransferText.progressLine(page.s.bytesDone, page.s.bytesTotal, 0, -1) : ""
                font.pixelSize: Theme.fontSizeLarge
                font.weight: Font.Light
                color: Theme.highlightColor
            }
            Label {
                x: Theme.horizontalPageMargin
                width: parent.width - 2 * Theme.horizontalPageMargin
                visible: page.s !== null && page.s.state !== "running" && page.s.state !== "scanning"
                height: visible ? implicitHeight : 0
                text: page.s ? TransferText.status({
                    group: page.finished ? "history" : (page.s.state === "paused" ? "paused" : (page.s.state === "waiting" ? "waiting" : "active")),
                    state: page.s.state, waitReason: page.s.waitReason, bytesDone: page.s.bytesDone,
                    bytesTotal: page.s.bytesTotal, rate: 0, eta: -1, itemsDone: page.s.itemsDone,
                    itemsTotal: page.s.itemsTotal, itemsFailed: page.s.itemsFailed,
                    finishedMs: page.s.finishedMs === null ? -1 : page.s.finishedMs
                }, false) : ""
                color: Theme.secondaryHighlightColor
                font.pixelSize: Theme.fontSizeSmall
            }
            Item { width: 1; height: Theme.paddingLarge }
            DetailItem {
                visible: page.s !== null
                //% "Destination"
                label: qsTrId("lautta-xfr-destination")
                value: page.s ? page.s.destination : ""
            }
            DetailItem {
                visible: page.s !== null
                //% "Started"
                label: qsTrId("lautta-xfr-started")
                value: page.s ? TransferText.when(page.s.createdMs) : ""
            }
            DetailItem {
                visible: page.s !== null
                //% "Verify"
                label: qsTrId("lautta-xfr-verify")
                value: page.s && page.s.verifyChecksums
                    //% "Checksum"
                    ? qsTrId("lautta-xfr-verify-checksum")
                    //% "Size"
                    : qsTrId("lautta-xfr-verify-size")
            }
            SectionHeader {
                //% "Items"
                text: qsTrId("lautta-xfr-items")
                visible: list.count > 0
            }
        }

        delegate: Item {
            width: list.width
            height: Math.max(Theme.itemSizeSmall, rowColumn.height + 2 * Theme.paddingSmall)

            Row {
                x: Theme.horizontalPageMargin
                width: parent.width - 2 * Theme.horizontalPageMargin
                anchors.verticalCenter: parent.verticalCenter
                spacing: Theme.paddingMedium

                AppIcon {
                    id: stateIcon

                    anchors.top: parent.top
                    anchors.topMargin: Theme.paddingSmall
                    icon: TransferText.itemIcon(model.state)
                }
                Column {
                    id: rowColumn

                    width: parent.width - stateIcon.width - parent.spacing

                    Label {
                        width: parent.width
                        text: name
                        truncationMode: TruncationMode.Fade
                    }
                    ProgressBar {
                        width: parent.width + 2 * Theme.horizontalPageMargin
                        x: -Theme.horizontalPageMargin
                        visible: model.state === "running"
                        height: visible ? implicitHeight : 0
                        minimumValue: 0
                        maximumValue: 1
                        value: progress
                    }
                    Label {
                        width: parent.width
                        visible: model.state !== "failed"
                        height: visible ? implicitHeight : 0
                        text: TransferText.itemStatus(model.state, kind, size, page.s ? page.s.direction : "")
                        color: Theme.secondaryColor
                        font.pixelSize: Theme.fontSizeSmall
                        truncationMode: TruncationMode.Fade
                    }
                    Label {
                        width: parent.width
                        visible: model.state === "failed"
                        height: visible ? implicitHeight : 0
                        text: ErrorText.message(errorKind, { "item": name, "location": page.s ? page.s.destName : "" })
                        color: Theme.highlightColor
                        font.pixelSize: Theme.fontSizeSmall
                        wrapMode: Text.WordWrap
                    }
                    Label {
                        visible: model.state === "failed" && page.finished
                        height: visible ? implicitHeight : 0
                        //% "Retry"
                        text: qsTrId("lautta-xfr-item-retry")
                        color: Theme.highlightColor
                        font.pixelSize: Theme.fontSizeSmall

                        MouseArea {
                            anchors.fill: parent
                            anchors.margins: -Theme.paddingMedium
                            onClicked: Transfers.retryFailed(page.transferId)
                        }
                    }
                }
            }
        }

        PullDownMenu {
            MenuItem {
                text: qsTrId("lautta-xfr-menu-cancel")
                visible: page.s !== null && !page.finished
                onClicked: Remorse.popupAction(
                    page,
                    qsTrId("lautta-xfr-remorse-cancel"),
                    function() { Transfers.cancel(page.transferId) },
                    App.setting("remorse_seconds") * 1000)
            }
            MenuItem {
                //% "Move up in queue"
                text: qsTrId("lautta-xfr-menu-move-up")
                visible: page.s !== null && !page.finished
                onClicked: Transfers.moveUp(page.transferId)
            }
            MenuItem {
                text: qsTrId("lautta-xfr-menu-retry")
                visible: page.s !== null && page.finished && page.s.itemsFailed > 0
                onClicked: Transfers.retryFailed(page.transferId)
            }
            MenuItem {
                text: page.s && page.s.state === "paused"
                    ? qsTrId("lautta-xfr-menu-resume")
                    : qsTrId("lautta-xfr-menu-pause")
                visible: page.s !== null && !page.finished
                onClicked: page.s.state === "paused" ? Transfers.resume(page.transferId) : Transfers.pause(page.transferId)
            }
        }

        ViewPlaceholder {
            enabled: page.s === null
            //% "This transfer is gone"
            text: qsTrId("lautta-xfr-gone")
        }

        VerticalScrollDecorator { }
    }
}
