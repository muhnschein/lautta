// SPDX-License-Identifier: LGPL-2.1-or-later
import QtQuick 2.6
import Sailfish.Silica 1.0
import Lautta 1.0
import "../components"
import "../components/operations"
import "../components/operations/Json.js" as Json

Dialog {
    id: dialog

    property int planId: -1
    property var summary

    readonly property var s: Json.value(summary, {})
    readonly property var origin: s.origin || null
    property int conflictCount: s.conflicts || 0
    property bool resolving

    function originName() {
        if (!origin)
            return ""
        if (origin.more > 0)
            //% "%1 and %n more"
            return qsTrId("lautta-plan-origin-more", origin.more).arg(origin.name)
        return origin.name
    }

    function filesText() {
        //% "%n files"
        return qsTrId("lautta-plan-files", s.files || 0)
    }

    function foldersText() {
        //% "in %n folders"
        return qsTrId("lautta-plan-in-folders", s.dirs || 0)
    }

    function optionsJson() {
        return JSON.stringify({ "suggestedNames": suggestedSwitch.checked })
    }

    function resolveNext() {
        var open = JSON.parse(Operations.conflictsJson(planId))
        conflictCount = open.length
        if (open.length === 0) {
            resolving = false
            return
        }
        resolving = true
        var entry = open[0]
        var page = pageStack.push(Qt.resolvedUrl("ConflictDialog.qml"), {
            "planId": planId,
            "item": entry.index,
            "name": entry.name,
            "kind": s.kind,
            "conflict": entry.conflict,
            "sourceUri": entry.source,
            "destinationUri": entry.destination,
            "sourceLook": entry.sourceLook,
            "destinationLook": entry.destinationLook,
            "defaultChoice": entry.defaultChoice,
            "position": (s.conflicts || open.length) - open.length + 1,
            "total": s.conflicts || open.length
        })
        page.rejected.connect(function() { dialog.resolving = false })
    }

    canAccept: planId >= 0

    onAccepted: Operations.startPlanWith(planId, optionsJson())
    onDone: {
        if (result !== DialogResult.Accepted)
            Operations.discardPlan(planId)
    }

    Connections {
        target: Operations
        onConflictResolved: {
            if (planId !== dialog.planId)
                return
            dialog.conflictCount = remaining
            if (dialog.resolving && remaining > 0)
                dialog.resolveNext()
            else
                dialog.resolving = false
        }
    }

    SilicaFlickable {
        anchors.fill: parent
        contentHeight: column.height + Theme.paddingLarge

        Column {
            id: column

            width: parent.width

            DialogHeader {
                //% "Start"
                acceptText: qsTrId("lautta-plan-start")
            }

            Item {
                id: ends

                readonly property real arrowWidth: Theme.iconSizeMedium
                readonly property real columnWidth: (width - arrowWidth) / 2

                x: Theme.horizontalPageMargin
                width: parent.width - 2 * Theme.horizontalPageMargin
                height: Math.max(originTile.height, destinationTile.height)

                ItemTile {
                    id: originTile

                    objectName: "originTile"
                    visible: dialog.origin !== null
                    width: ends.columnWidth
                    tileSize: Math.min(width, Screen.width * 0.375)
                    //% "Origin"
                    label: qsTrId("lautta-plan-origin")
                    name: dialog.originName()
                    captions: dialog.origin ? [ dialog.origin.folder ] : []
                    isDir: dialog.origin ? dialog.origin.isDir : false
                    uri: dialog.origin ? dialog.origin.uri : ""
                    look: dialog.origin ? dialog.origin.look : ({})
                    stacked: dialog.origin ? dialog.origin.more > 0 : false
                }
                AppIcon {
                    objectName: "originArrow"
                    x: ends.columnWidth + (ends.arrowWidth - width) / 2
                    y: destinationTile.tileTop + (destinationTile.tileSize - height) / 2
                    icon: "arrow-right"
                    small: true
                    color: Theme.highlightColor
                }
                ItemTile {
                    id: destinationTile

                    objectName: "destinationTile"
                    x: ends.columnWidth + ends.arrowWidth
                    width: ends.columnWidth
                    tileSize: originTile.tileSize
                    //% "Destination"
                    label: qsTrId("lautta-plan-destination")
                    name: dialog.s.destinationName || ""
                    captions: dialog.s.destinationFolder ? [ dialog.s.destinationFolder ] : []
                    isDir: true
                }
            }

            Item {
                width: parent.width
                height: Theme.paddingLarge
            }
            Row {
                id: figures

                readonly property real cellWidth: width / 3

                x: Theme.horizontalPageMargin
                width: parent.width - 2 * Theme.horizontalPageMargin

                PlanFigure {
                    objectName: "filesFigure"
                    width: figures.cellWidth
                    value: dialog.filesText()
                    label: dialog.foldersText()
                }
                PlanFigure {
                    objectName: "sizeFigure"
                    width: figures.cellWidth
                    value: Format.formatFileSize(dialog.s.bytes || 0)
                    //% "in total"
                    label: qsTrId("lautta-plan-in-total")
                }
                PlanFigure {
                    objectName: "freeFigure"
                    width: figures.cellWidth
                    visible: dialog.s.freeBytes !== undefined && dialog.s.freeBytes !== null
                    value: Format.formatFileSize(dialog.s.freeBytes || 0)
                    //% "free there"
                    label: qsTrId("lautta-plan-free-there")
                }
            }

            Item {
                width: parent.width
                height: Theme.paddingLarge
            }
            BackgroundItem {
                id: conflictsRow

                readonly property bool open: dialog.conflictCount > 0
                readonly property color warning: Theme.presenceColor(Theme.PresenceAway)

                objectName: "conflictsRow"
                visible: (dialog.s.conflicts || 0) > 0
                width: parent.width
                height: Math.max(Theme.itemSizeMedium, conflictText.height + 2 * Theme.paddingMedium)
                enabled: open
                onClicked: dialog.resolveNext()

                Rectangle {
                    anchors.fill: parent
                    visible: conflictsRow.open
                    color: conflictsRow.warning
                    opacity: 0.12
                }
                AppIcon {
                    id: conflictIcon

                    x: Theme.horizontalPageMargin
                    anchors.verticalCenter: parent.verticalCenter
                    icon: conflictsRow.open ? "image://theme/icon-m-warning" : "image://theme/icon-m-acknowledge"
                    color: conflictsRow.open ? conflictsRow.warning : Theme.secondaryHighlightColor
                }
                Column {
                    id: conflictText

                    anchors {
                        left: conflictIcon.right
                        leftMargin: Theme.paddingLarge
                        right: parent.right
                        rightMargin: Theme.horizontalPageMargin
                        verticalCenter: parent.verticalCenter
                    }

                    Label {
                        objectName: "conflictsTitle"
                        width: parent.width
                        wrapMode: Text.Wrap
                        color: conflictsRow.open ? conflictsRow.warning : Theme.primaryColor
                        text: conflictsRow.open
                              //% "%n conflicts"
                              ? qsTrId("lautta-plan-conflict-count", dialog.conflictCount)
                              //% "Conflicts resolved"
                              : qsTrId("lautta-plan-conflicts-resolved", dialog.s.conflicts || 0)
                    }
                    Label {
                        width: parent.width
                        wrapMode: Text.Wrap
                        color: Theme.secondaryColor
                        font.pixelSize: Theme.fontSizeSmall
                        text: conflictsRow.open
                              //% "Tap to choose what happens"
                              ? qsTrId("lautta-plan-conflicts-tap")
                              //% "You chose for all %n"
                              : qsTrId("lautta-plan-conflicts-chosen", dialog.s.conflicts || 0)
                    }
                }
            }

            SectionHeader {
                visible: (dialog.s.renamed || 0) > 0
                //% "Names that need changing"
                text: qsTrId("lautta-plan-names")
            }
            Repeater {
                model: dialog.s.renames || []

                Item {
                    width: column.width
                    height: Theme.itemSizeExtraSmall

                    Label {
                        id: fromLabel

                        anchors {
                            left: parent.left
                            leftMargin: Theme.horizontalPageMargin
                            right: renameArrow.left
                            rightMargin: Theme.paddingMedium
                            verticalCenter: parent.verticalCenter
                        }
                        text: modelData.from
                        color: Theme.secondaryColor
                        truncationMode: TruncationMode.Fade
                    }
                    AppIcon {
                        id: renameArrow

                        anchors.centerIn: parent
                        icon: "arrow-right"
                        small: true
                        color: Theme.secondaryColor
                    }
                    Label {
                        anchors {
                            left: renameArrow.right
                            leftMargin: Theme.paddingMedium
                            right: parent.right
                            rightMargin: Theme.horizontalPageMargin
                            verticalCenter: parent.verticalCenter
                        }
                        text: modelData.to
                        truncationMode: TruncationMode.Fade
                    }
                }
            }
            TextSwitch {
                id: suggestedSwitch

                visible: (dialog.s.renamed || 0) > 0
                checked: true
                //% "Use suggested names for all"
                text: qsTrId("lautta-plan-use-suggested")
            }
        }
        VerticalScrollDecorator { }
    }
}
