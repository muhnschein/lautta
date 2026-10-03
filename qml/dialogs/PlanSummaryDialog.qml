// SPDX-License-Identifier: LGPL-2.1-or-later
// The summary sheet of a large plan (OPS-1): totals, space, conflicts, names
// that change, and the options of this one run. "Start" queues the transfer.
import QtQuick 2.6
import Sailfish.Silica 1.0
import Lautta 1.0
import "../components/operations"
import "../components/operations/Json.js" as Json

Dialog {
    id: dialog

    // The id from Operations.needsSummary and its summary (object or JSON).
    property int planId: -1
    property var summary

    readonly property var s: Json.value(summary, {})
    property int conflictCount: s.conflicts || 0
    property bool resolving
    property bool started

    function destinationText() {
        var name = s.destinationName || ""
        switch (s.kind) {
        case "move":
            //% "Move to %1"
            return qsTrId("lautta-plan-move-to").arg(name)
        case "extract":
            //% "Extract to %1"
            return qsTrId("lautta-plan-extract-to").arg(name)
        default:
            //% "Copy to %1"
            return qsTrId("lautta-plan-copy-to").arg(name)
        }
    }

    function itemsText() {
        //% "%n files"
        var files = qsTrId("lautta-plan-files", s.files || 0)
        //% "%n folders"
        var dirs = qsTrId("lautta-plan-folders", s.dirs || 0)
        //% "%1 in %2"
        return qsTrId("lautta-plan-items").arg(files).arg(dirs)
    }

    function optionsJson() {
        return JSON.stringify({
            "suggestedNames": suggestedSwitch.checked,
            "preserveMtime": mtimeSwitch.checked,
            "preserveMode": modeSwitch.checked,
            "verifyChecksums": verifySwitch.checked
        })
    }

    // Walks through the open conflicts one by one (OPS-2); each answer comes
    // back as Operations.conflictResolved.
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
            "folder": entry.folder,
            "kind": s.kind,
            "conflict": entry.conflict,
            "defaultChoice": entry.defaultChoice,
            "position": (s.conflicts || open.length) - open.length + 1,
            "total": s.conflicts || open.length
        })
        page.rejected.connect(function() { dialog.resolving = false })
    }

    canAccept: planId >= 0

    onAccepted: {
        started = true
        Operations.startPlanWith(planId, optionsJson())
    }
    onDone: {
        if (!started)
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
        contentHeight: column.height

        Column {
            id: column
            width: parent.width

            DialogHeader {
                //% "Start"
                acceptText: qsTrId("lautta-plan-start")
            }
            DialogSubtitle {
                text: dialog.destinationText()
            }

            DetailItem {
                //% "Items"
                label: qsTrId("lautta-plan-items-label")
                value: dialog.itemsText()
            }
            DetailItem {
                //% "Size"
                label: qsTrId("lautta-plan-size")
                value: Format.formatFileSize(dialog.s.bytes || 0)
            }
            DetailItem {
                visible: dialog.s.freeBytes !== undefined && dialog.s.freeBytes !== null
                //% "Free on destination"
                label: qsTrId("lautta-plan-free")
                value: Format.formatFileSize(dialog.s.freeBytes || 0)
            }
            BackgroundItem {
                id: conflictsRow
                width: parent.width
                height: Theme.itemSizeSmall
                enabled: dialog.conflictCount > 0
                onClicked: dialog.resolveNext()

                Label {
                    anchors {
                        left: parent.left
                        right: parent.horizontalCenter
                        rightMargin: Theme.paddingSmall
                        verticalCenter: parent.verticalCenter
                    }
                    horizontalAlignment: Text.AlignRight
                    color: Theme.secondaryHighlightColor
                    font.pixelSize: Theme.fontSizeSmall
                    //% "Conflicts"
                    text: qsTrId("lautta-plan-conflicts")
                }
                Label {
                    anchors {
                        left: parent.horizontalCenter
                        leftMargin: Theme.paddingSmall
                        right: parent.right
                        rightMargin: Theme.horizontalPageMargin
                        verticalCenter: parent.verticalCenter
                    }
                    color: conflictsRow.enabled ? Theme.highlightColor : Theme.primaryColor
                    font.pixelSize: Theme.fontSizeSmall
                    text: dialog.conflictCount
                }
            }
            DetailItem {
                //% "Names changed"
                label: qsTrId("lautta-plan-renamed")
                value: dialog.s.renamed || 0
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
                    height: Theme.itemSizeSmall

                    Column {
                        anchors {
                            left: parent.left
                            leftMargin: Theme.horizontalPageMargin
                            right: parent.right
                            rightMargin: Theme.horizontalPageMargin
                            verticalCenter: parent.verticalCenter
                        }
                        Label {
                            width: parent.width
                            text: modelData.from
                            color: Theme.secondaryColor
                            font.pixelSize: Theme.fontSizeSmall
                            truncationMode: TruncationMode.Fade
                        }
                        Label {
                            width: parent.width
                            text: modelData.to
                            truncationMode: TruncationMode.Fade
                        }
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

            SectionHeader {
                //% "Options"
                text: qsTrId("lautta-plan-options")
            }
            TextSwitch {
                id: mtimeSwitch
                checked: dialog.s.options ? dialog.s.options.preserveMtime : true
                //% "Keep modification times"
                text: qsTrId("lautta-plan-keep-mtime")
            }
            TextSwitch {
                id: modeSwitch
                visible: dialog.s.permissionsSupported === true
                checked: dialog.s.options ? dialog.s.options.preserveMode : false
                //% "Keep permissions"
                text: qsTrId("lautta-plan-keep-mode")
                //% "Both sides support it"
                description: qsTrId("lautta-plan-keep-mode-hint")
            }
            TextSwitch {
                id: verifySwitch
                checked: dialog.s.options ? dialog.s.options.verifyChecksums : false
                //% "Verify with checksums"
                text: qsTrId("lautta-plan-verify")
            }
        }
        VerticalScrollDecorator { }
    }
}
