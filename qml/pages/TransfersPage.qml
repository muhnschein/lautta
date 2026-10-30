// SPDX-License-Identifier: LGPL-2.1-or-later
import QtQuick 2.6
import Sailfish.Silica 1.0
import Lautta 1.0
import "../components/transfers"
import "../components"
import "../components/transfers/TransferText.js" as TransferText

Page {
    id: page

    property var listModel: transfers

    objectName: "transfersPage"
    allowedOrientations: Orientation.All

    onStatusChanged: if (status === PageStatus.Active) Transfers.barSeen()

    function headerDescription() {
        if (Transfers.pendingAtStart > 0)
            //% "%n stopped"
            return qsTrId("lautta-xfr-header-stopped", Transfers.pendingAtStart)
        if (Transfers.activeCount > 0) {
            //% "%n running"
            var running = qsTrId("lautta-xfr-header-running", Transfers.activeCount)
            return Transfers.rate > 0 ? running + " · " + qsTrId("lautta-xfr-rate").arg(TransferText.size(Transfers.rate)) : running
        }
        if (Transfers.pausedCount > 0)
            //% "%n paused"
            return qsTrId("lautta-xfr-header-paused", Transfers.pausedCount)
        return ""
    }

    function activate(row) {
        if (row.questions > 0) {
            var asked = JSON.parse(Transfers.questionsJson(row.transferId))
            if (asked.length > 0) {
                pageStack.push(Qt.resolvedUrl("../dialogs/ConflictDialog.qml"), {
                    "transferId": row.transferId, "item": asked[0].item, "conflict": asked[0]
                })
                return
            }
        }
        pageStack.push(Qt.resolvedUrl("TransferDetailsPage.qml"), { "transferId": row.transferId })
    }

    TransfersModel { id: transfers }

    SilicaListView {
        id: list

        anchors.fill: parent
        model: page.listModel

        header: Column {
            width: list.width

            PageHeader {
                //% "Activity"
                title: qsTrId("lautta-xfr-title")
                description: page.headerDescription()
            }
            BackgroundItem {
                id: banner

                width: parent.width
                visible: Transfers.pendingAtStart > 0
                height: visible ? Math.max(Theme.itemSizeSmall, bannerText.height + 2 * Theme.paddingMedium) : 0
                onClicked: Transfers.resumeAll()

                AppIcon {
                    id: bannerIcon

                    x: Theme.horizontalPageMargin
                    anchors.verticalCenter: parent.verticalCenter
                    icon: "image://theme/icon-m-warning"
                    small: true
                    highlighted: banner.highlighted
                }
                Label {
                    id: bannerText

                    anchors {
                        left: bannerIcon.right
                        leftMargin: Theme.paddingMedium
                        right: resumeLabel.left
                        rightMargin: Theme.paddingMedium
                        verticalCenter: parent.verticalCenter
                    }
                    //% "%n tasks stopped when Lautta closed."
                    text: qsTrId("lautta-xfr-restored", Transfers.pendingAtStart)
                    wrapMode: Text.WordWrap
                    font.pixelSize: Theme.fontSizeSmall
                    color: banner.highlighted ? Theme.highlightColor : Theme.primaryColor
                }
                Label {
                    id: resumeLabel

                    anchors {
                        right: parent.right
                        rightMargin: Theme.horizontalPageMargin
                        verticalCenter: parent.verticalCenter
                    }
                    text: qsTrId("lautta-xfr-menu-resume")
                    color: Theme.highlightColor
                }
            }
        }

        section {
            property: "dateGroup"
            delegate: DateSectionHeader {
                group: section
            }
        }

        delegate: TransferDelegate {
            row: model
            restored: Transfers.pendingAtStart > 0
            onClicked: page.activate({ "transferId": transferId, "questions": questions })
        }

        footer: Column {
            width: list.width

            Label {
                visible: Transfers.pendingAtStart > 0
                x: Theme.horizontalPageMargin
                width: parent.width - 2 * Theme.horizontalPageMargin
                height: visible ? implicitHeight + Theme.paddingLarge : 0
                //% "Nothing was lost. Finished files stay where they are."
                text: qsTrId("lautta-xfr-restored-hint")
                wrapMode: Text.WordWrap
                font.pixelSize: Theme.fontSizeSmall
                color: Theme.secondaryColor
            }
        }

        // Every entry needs a task in the list.
        PullDownMenu {
            visible: list.count > 0

            MenuItem {
                //% "Clear activity history"
                text: qsTrId("lautta-xfr-menu-clear-activity")
                visible: list.count > 0
                onClicked: Remorse.popupAction(
                    page,
                    //% "Clearing activity history"
                    qsTrId("lautta-xfr-remorse-clear-activity"),
                    function() { Transfers.clearHistory() },
                    App.setting("remorse_seconds") * 1000)
            }
            MenuItem {
                //% "Resume all"
                text: qsTrId("lautta-xfr-menu-resume-all")
                visible: Transfers.pausedCount > 0
                onClicked: Transfers.resumeAll()
            }
            MenuItem {
                //% "Pause all"
                text: qsTrId("lautta-xfr-menu-pause-all")
                visible: Transfers.activeCount > 0 || Transfers.waitingCount > 0
                onClicked: Transfers.pauseAll()
            }
        }

        ViewPlaceholder {
            enabled: list.count === 0
            //% "No activity"
            text: qsTrId("lautta-xfr-empty")
            //% "Copies, moves, deletes and archives you start show up here."
            hintText: qsTrId("lautta-xfr-empty-hint")
        }

        VerticalScrollDecorator { }
    }
}
