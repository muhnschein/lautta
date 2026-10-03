// SPDX-License-Identifier: LGPL-2.1-or-later
// Compare results and sync (SYN-1..3; design: CompareResults): what a sync
// would copy, replace and delete, grouped by action; tap an item to leave it
// out; Sync queues ordinary transfers. Also the page a saved sync pair opens.
import QtQuick 2.6
import Sailfish.Silica 1.0
import Lautta 1.0
import "../components"
import "../components/search"
import "../components/ErrorText.js" as ErrorText
import "../components/search/SearchText.js" as SearchText

Page {
    id: page

    property string leftUri
    property string rightUri
    property bool dstTolerance
    property bool checksums
    property string excludesText
    // Keep the pair after the first comparison (SYN-3).
    property bool savePair
    property string pairLabel
    // A saved pair to run (its folders and options replace the ones above).
    property int pairId
    // `mirror_lr`, `mirror_rl` or `update_both` (SYN-2).
    property string mode: "update_both"
    property bool pairSaved

    readonly property string heading: cmp.pairLabel.length > 0
                                    ? cmp.pairLabel
                                    : (pairLabel.length > 0
                                       ? pairLabel
                                       : (cmp.leftUri.length > 0 && cmp.rightUri.length > 0
                                          ? App.nameOf(cmp.leftUri) + " → " + App.nameOf(cmp.rightUri)
                                          //% "Compare"
                                          : qsTrId("lautta-results-title")))

    function groupTitle(group) {
        switch (group) {
        case "copy_right":
            //% "Copy to right"
            return qsTrId("lautta-results-copy-right")
        case "replace_right":
            //% "Replace on right"
            return qsTrId("lautta-results-replace-right")
        case "delete_right":
            //% "Delete on right"
            return qsTrId("lautta-results-delete-right")
        case "copy_left":
            //% "Copy to left"
            return qsTrId("lautta-results-copy-left")
        case "replace_left":
            //% "Replace on left"
            return qsTrId("lautta-results-replace-left")
        case "delete_left":
            //% "Delete on left"
            return qsTrId("lautta-results-delete-left")
        default:
            //% "Differ, not synced"
            return qsTrId("lautta-results-skipped")
        }
    }

    function detail(status, group, size, modified, isDir) {
        var lead = ""
        if (group === "delete_right")
            //% "Not on left"
            lead = qsTrId("lautta-results-not-on-left")
        else if (group === "delete_left")
            //% "Not on right"
            lead = qsTrId("lautta-results-not-on-right")
        else if (status === "left_newer")
            //% "Left newer"
            lead = qsTrId("lautta-results-left-newer")
        else if (status === "right_newer")
            //% "Right newer"
            lead = qsTrId("lautta-results-right-newer")
        else if (status === "different")
            //% "Differs"
            lead = qsTrId("lautta-results-differs")
        var rest = SearchText.sizeAndDate(size, modified, isDir)
        return lead.length > 0 && rest.length > 0 ? lead + " · " + rest : lead + rest
    }

    function sideWord() {
        return cmp.mode === "mirror_rl" ? "left" : (cmp.mode === "mirror_lr" ? "right" : "")
    }

    function startSync() {
        var run = function () { cmp.sync(cmp.mode) }
        if (cmp.deleteCount > 0)
            Remorse.popupAction(page,
                                //% "Syncing, deleting %n items"
                                qsTrId("lautta-results-syncing", cmp.deleteCount),
                                run, Number(App.setting("remorse_seconds")) * 1000)
        else
            run()
    }

    allowedOrientations: Orientation.All
    Component.onCompleted: {
        if (pairId > 0)
            cmp.loadPair(pairId)
        else
            cmp.compare()
    }

    CompareModel {
        id: cmp

        leftUri: page.leftUri
        rightUri: page.rightUri
        mode: page.mode
        dstTolerance: page.dstTolerance
        checksums: page.checksums
        excludesText: page.excludesText
        onPairLoaded: {
            page.mode = cmp.mode
            cmp.compare()
        }
        onRunningChanged: {
            if (!running && ready && page.savePair && cmp.pairId === 0 && !page.pairSaved) {
                page.pairSaved = true
                cmp.saveAsPair(page.heading)
            }
        }
        onSyncStarted: pageStack.replace(Qt.resolvedUrl("TransfersPage.qml"))
        onSyncFailed: {
            notice.text = ErrorText.message(kind, {})
            notice.show()
        }
        onPairFailed: {
            notice.text = ErrorText.message(kind, {})
            notice.show()
        }
    }

    Notice {
        id: notice
    }

    SilicaListView {
        id: list

        anchors {
            fill: parent
            bottomMargin: dock.visibleSize
        }
        clip: true
        model: cmp

        PullDownMenu {
            MenuItem {
                //% "Compare again"
                text: qsTrId("lautta-results-again")
                enabled: !cmp.running
                onClicked: cmp.compare()
            }
        }

        header: Column {
            width: list.width

            PageHeader {
                title: page.heading
                description: cmp.ready
                             //% "%n items compared"
                             ? qsTrId("lautta-results-compared", cmp.totalCount)
                             : ""
            }

            ComboBox {
                visible: cmp.ready
                //% "Direction"
                label: qsTrId("lautta-results-direction")
                currentIndex: cmp.mode === "mirror_lr" ? 0 : (cmp.mode === "mirror_rl" ? 1 : 2)
                menu: ContextMenu {
                    MenuItem {
                        //% "Mirror left to right"
                        text: qsTrId("lautta-results-mirror-lr")
                        onClicked: page.chooseMode("mirror_lr")
                    }
                    MenuItem {
                        //% "Mirror right to left"
                        text: qsTrId("lautta-results-mirror-rl")
                        onClicked: page.chooseMode("mirror_rl")
                    }
                    MenuItem {
                        //% "Update both, newer wins, no deletes"
                        text: qsTrId("lautta-results-update-both")
                        onClicked: page.chooseMode("update_both")
                    }
                }
            }

            DetailItem {
                visible: cmp.ready && cmp.copyCount - cmp.replaceCount > 0
                label: page.sideWord() === "left"
                       //% "Copy to left"
                       ? qsTrId("lautta-results-copy-left")
                       : (page.sideWord() === "right"
                          //% "Copy to right"
                          ? qsTrId("lautta-results-copy-right")
                          //% "Copy"
                          : qsTrId("lautta-results-copy"))
                value: (cmp.copyCount - cmp.replaceCount) + " (" + SearchText.bytes(cmp.copyBytes) + ")"
            }
            DetailItem {
                visible: cmp.ready && cmp.replaceCount > 0
                label: page.sideWord() === "left"
                       ? qsTrId("lautta-results-replace-left")
                       : (page.sideWord() === "right"
                          ? qsTrId("lautta-results-replace-right")
                          //% "Replace"
                          : qsTrId("lautta-results-replace"))
                value: String(cmp.replaceCount)
            }
            DetailItem {
                visible: cmp.ready && cmp.deleteCount > 0
                label: page.sideWord() === "left"
                       ? qsTrId("lautta-results-delete-left")
                       : qsTrId("lautta-results-delete-right")
                //% "%1 · Mirror deletes"
                value: qsTrId("lautta-results-delete-value").arg(cmp.deleteCount)
            }
            DetailItem {
                visible: cmp.ready
                //% "Same"
                label: qsTrId("lautta-results-same")
                value: String(cmp.sameCount)
            }
        }

        section {
            property: "group"
            delegate: SectionHeader {
                text: page.groupTitle(section)
                horizontalAlignment: Text.AlignRight
            }
        }

        delegate: BackgroundItem {
            id: item

            height: Theme.itemSizeSmall
            onClicked: cmp.toggleExcluded(index)

            ResultIcon {
                id: icon

                x: Theme.horizontalPageMargin
                anchors.verticalCenter: parent.verticalCenter
                isDir: model.isDir
                category: App.categoryOf(model.name)
                opacity: model.excluded ? Theme.opacityLow : 1
                highlighted: item.highlighted
            }

            Column {
                anchors {
                    left: icon.right
                    leftMargin: Theme.paddingLarge
                    right: marker.left
                    rightMargin: Theme.paddingMedium
                    verticalCenter: parent.verticalCenter
                }

                Label {
                    width: parent.width
                    text: model.name
                    font.strikeout: model.excluded
                    truncationMode: TruncationMode.Fade
                    color: item.highlighted ? Theme.highlightColor : Theme.primaryColor
                }
                Label {
                    width: parent.width
                    text: model.excluded
                          //% "Left out of this sync"
                          ? qsTrId("lautta-results-left-out")
                          : page.detail(model.status, model.group, model.size, model.modified, model.isDir)
                    truncationMode: TruncationMode.Fade
                    color: Theme.secondaryColor
                    font.pixelSize: Theme.fontSizeExtraSmall
                }
            }

            Label {
                id: marker

                anchors {
                    right: parent.right
                    rightMargin: Theme.horizontalPageMargin
                    verticalCenter: parent.verticalCenter
                }
                text: model.excluded
                      //% "Excluded"
                      ? qsTrId("lautta-results-excluded")
                      : (model.group.indexOf("delete") === 0
                         //% "Delete"
                         ? qsTrId("lautta-results-delete")
                         : "")
                color: model.excluded ? Theme.secondaryColor : Theme.errorColor
                font.pixelSize: Theme.fontSizeSmall
            }
        }

        ViewPlaceholder {
            enabled: !cmp.running && (cmp.errorKind.length > 0 || (cmp.ready && list.count === 0))
            text: cmp.errorKind.length > 0
                  //% "Can't compare"
                  ? qsTrId("lautta-results-failed")
                  //% "Folders are in sync"
                  : qsTrId("lautta-results-in-sync")
            hintText: cmp.errorKind.length > 0 ? ErrorText.message(cmp.errorKind, {}) : ""
        }

        VerticalScrollDecorator { }
    }

    BusyIndicator {
        anchors.centerIn: parent
        size: BusyIndicatorSize.Large
        running: cmp.running
    }

    DockedPanel {
        id: dock

        width: parent.width
        height: Theme.itemSizeLarge
        dock: Dock.Bottom
        open: cmp.ready && !cmp.running
        modal: false

        Button {
            anchors.centerIn: parent
            preferredWidth: Theme.buttonWidthLarge
            enabled: cmp.syncCount > 0
            //% "Sync %n items"
            text: qsTrId("lautta-results-sync", cmp.syncCount)
            onClicked: page.startSync()
        }

        IconButton {
            anchors {
                right: parent.right
                rightMargin: Theme.horizontalPageMargin - Theme.paddingMedium
                verticalCenter: parent.verticalCenter
            }
            icon.source: "image://theme/icon-m-refresh"
            onClicked: cmp.compare()
        }
    }

    function chooseMode(newMode) {
        page.mode = newMode
        // A saved pair remembers its direction.
        if (cmp.pairId > 0)
            cmp.saveAsPair(cmp.pairLabel)
    }
}
