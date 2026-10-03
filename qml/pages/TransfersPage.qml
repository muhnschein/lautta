// SPDX-License-Identifier: LGPL-2.1-or-later
// Transfers (SPEC §15.4): Active, Waiting for you, Paused, Edited files and
// History, with the prompt for transfers that stopped when the app closed
// (XFR-11, board TransfersRestored).
import QtQuick 2.6
import Sailfish.Silica 1.0
import Lautta 1.0
import "../components/transfers"
import "../components"
import "../components/transfers/TransferText.js" as TransferText

Page {
    id: page

    objectName: "transfersPage"
    allowedOrientations: Orientation.All

    function sectionTitle(group) {
        switch (group) {
        case "active":
            //% "Active"
            return qsTrId("lautta-xfr-group-active")
        case "waiting":
            //% "Waiting for you"
            return qsTrId("lautta-xfr-group-waiting")
        case "paused":
            //% "Paused"
            return qsTrId("lautta-xfr-group-paused")
        case "edited":
            //% "Edited files"
            return qsTrId("lautta-xfr-group-edited")
        default:
            //% "History"
            return qsTrId("lautta-xfr-group-history")
        }
    }

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

    // What a tap on a row does: details, the unanswered conflict, or the
    // upload of an edited file.
    function activate(row) {
        if (row.group === "edited") {
            if (row.dirty)
                workingCopies.uploadNow(row.copyId)
            return
        }
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

    WorkingCopiesModel {
        id: workingCopies

        onConflict: pageStack.push(Qt.resolvedUrl("../dialogs/EditConflictDialog.qml"), { "copyId": copyId })
    }

    SilicaListView {
        id: list

        anchors.fill: parent
        model: transfers

        header: Column {
            width: list.width

            PageHeader {
                //% "Transfers"
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
                    //% "%n transfer(s) stopped when Lautta closed."
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
            property: "group"
            delegate: SectionHeader {
                text: page.sectionTitle(section)
            }
        }

        delegate: TransferDelegate {
            row: model
            copies: workingCopies
            restored: Transfers.pendingAtStart > 0
            onClicked: page.activate({
                "group": group, "dirty": dirty, "copyId": copyId,
                "transferId": transferId, "questions": questions
            })
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

        PullDownMenu {
            MenuItem {
                //% "Settings"
                text: qsTrId("lautta-xfr-menu-settings")
                onClicked: pageStack.push(Qt.resolvedUrl("SettingsPage.qml"))
            }
            MenuItem {
                //% "Clear history"
                text: qsTrId("lautta-xfr-menu-clear")
                visible: list.count > 0
                onClicked: Remorse.popupAction(
                    page,
                    //% "Clearing history"
                    qsTrId("lautta-xfr-remorse-clear"),
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
            //% "No transfers"
            text: qsTrId("lautta-xfr-empty")
            //% "Copies, moves and uploads you start show up here."
            hintText: qsTrId("lautta-xfr-empty-hint")
        }

        VerticalScrollDecorator { }
    }
}
