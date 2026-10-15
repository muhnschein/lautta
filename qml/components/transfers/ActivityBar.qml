// SPDX-License-Identifier: LGPL-2.1-or-later
// The activity bar (XFR-13, boards ActivityBar*): at the bottom of Browse and
// every folder, what runs, what waits for an answer and what just ended. Tap
// opens Activity (or the conflict waiting for an answer); a sideways swipe
// hides it until something changes. The pages leave `space` free at the
// end of their lists.
import QtQuick 2.6
import Sailfish.Silica 1.0
import Lautta 1.0
import "../"
import "TransferText.js" as TransferText

Item {
    id: bar

    // False while the page selects or is not on top.
    property bool allowed: true
    readonly property string mode: Transfers.barMode
    readonly property bool shown: allowed && mode !== "hidden" && mode.length > 0
    // What the page keeps free under its list.
    readonly property real space: shown ? height : 0
    // The lead transfer's summary (Transfers.summaryJson), re-read as it moves.
    property var summary: null
    readonly property bool problem: mode === "failed"
    readonly property bool asking: mode === "question"

    // Hidden keeps the last summary so the bar fades out with its text.
    function reload() {
        if (Transfers.barMode === "hidden")
            return
        var text = Transfers.summaryJson(Transfers.barTransferId)
        summary = text.length > 0 ? JSON.parse(text) : null
    }

    function activate() {
        var id = Transfers.barTransferId
        if (asking) {
            var asked = JSON.parse(Transfers.questionsJson(id))
            if (asked.length > 0) {
                pageStack.push(Qt.resolvedUrl("../../dialogs/ConflictDialog.qml"), {
                    "transferId": id, "item": asked[0].item, "conflict": asked[0]
                })
                return
            }
        }
        pageStack.push(Qt.resolvedUrl("../../pages/TransfersPage.qml"))
    }

    objectName: "activityBar"
    width: parent ? parent.width : 0
    height: Theme.itemSizeLarge
    visible: opacity > 0 && summary !== null
    opacity: shown ? 1 : 0
    anchors.bottom: parent ? parent.bottom : undefined

    Behavior on opacity { FadeAnimator { } }

    Component.onCompleted: reload()

    Connections {
        target: Transfers
        onChanged: bar.reload()
        onBarChanged: bar.reload()
    }

    Item {
        id: handle
    }

    BackgroundItem {
        id: body

        width: parent.width
        height: parent.height
        opacity: 1 - Math.min(1, Math.abs(handle.x) / width)
        highlightedColor: Theme.rgba(Theme.highlightBackgroundColor, Theme.highlightBackgroundOpacity)
        drag {
            target: handle
            axis: Drag.XAxis
            minimumX: -width
            maximumX: width
        }
        onClicked: bar.activate()
        onReleased: {
            if (Math.abs(handle.x) > width / 3)
                Transfers.dismissBar()
            handle.x = 0
        }
        Accessible.role: Accessible.Button
        Accessible.name: title.text

        transform: Translate { x: handle.x }

        Rectangle {
            anchors.fill: parent
            color: Theme.rgba(Theme.overlayBackgroundColor, 0.9)
        }
        Rectangle {
            width: parent.width
            height: 1
            color: Theme.rgba(Theme.highlightColor, Theme.opacityFaint)
        }

        Item {
            id: badge

            x: Theme.horizontalPageMargin
            anchors.verticalCenter: parent.verticalCenter
            width: Theme.iconSizeMedium
            height: width

            ProgressCircle {
                anchors.fill: parent
                visible: bar.mode === "running" || bar.mode === "paused"
                value: bar.summary && bar.summary.bytesTotal > 0 ? bar.summary.bytesDone / bar.summary.bytesTotal : 0
                progressColor: Theme.highlightColor
                backgroundColor: Theme.rgba(Theme.primaryColor, Theme.opacityFaint)
                borderWidth: Theme.paddingSmall / 2
            }
            AppIcon {
                anchors.centerIn: parent
                icon: bar.summary ? TransferText.barIcon(bar.mode, bar.summary.direction) : ""
                small: true
                color: bar.problem ? Theme.errorColor : Theme.primaryColor
                highlighted: body.highlighted || bar.mode === "done"
            }
        }

        Column {
            anchors {
                left: badge.right
                leftMargin: Theme.paddingMedium
                right: show.left
                verticalCenter: parent.verticalCenter
            }

            Label {
                id: title

                width: parent.width
                text: bar.summary ? TransferText.barTitle(bar.mode, bar.summary) : ""
                textFormat: Text.PlainText
                truncationMode: TruncationMode.Fade
                font.pixelSize: Theme.fontSizeSmall
                color: body.highlighted ? Theme.highlightColor : Theme.primaryColor
            }
            Label {
                width: parent.width
                text: bar.summary ? TransferText.barDetail(bar.mode, bar.summary, Transfers.barUnfinished - 1,
                                                           Transfers.questionCount) : ""
                textFormat: Text.PlainText
                truncationMode: TruncationMode.Fade
                font.pixelSize: Theme.fontSizeExtraSmall
                color: bar.problem ? Theme.errorColor
                                   : (bar.asking ? Theme.highlightColor : Theme.secondaryColor)
            }
        }

        Label {
            id: show

            anchors {
                right: parent.right
                rightMargin: Theme.horizontalPageMargin
                verticalCenter: parent.verticalCenter
            }
            text: bar.asking
                  //% "Answer"
                  ? qsTrId("lautta-activity-bar-answer")
                  //% "Show"
                  : qsTrId("lautta-activity-bar-show")
            font.pixelSize: Theme.fontSizeSmall
            color: body.highlighted ? Theme.primaryColor : Theme.highlightColor
        }
    }
}
