// SPDX-License-Identifier: LGPL-2.1-or-later
import QtQuick 2.6
import Sailfish.Silica 1.0
import Lautta 1.0
import "../components/operations"
import "../components/operations/Json.js" as Json

Dialog {
    id: dialog

    property int planId: -1
    property int transferId: -1
    property int item: 0
    property var conflict
    readonly property var c: Json.value(conflict, {})
    property string name: c.name || ""
    property string kind: c.kind || "copy"
    property var sourceLook: c.source_look || {}
    property var destinationLook: c.destination_look || {}
    property string sourceUri: c.source || ""
    property string destinationUri: c.destination || ""
    property string defaultChoice
    property int position: 1
    property int total: c.open || 1

    readonly property var choices: c.choices || []
    readonly property bool isDir: c.src_is_dir === true || c.dst_is_dir === true
    readonly property int remaining: total - position
    property string choice: initialChoice()

    function initialChoice() {
        if (defaultChoice.length > 0 && defaultChoice !== "Replace" && Json.contains(choices, defaultChoice))
            return defaultChoice
        for (var i = 0; i < choices.length; ++i)
            if (choices[i] !== "Replace")
                return choices[i]
        return ""
    }

    function captions(size, mtime, dir) {
        var lines = []
        if (size !== undefined && size !== null && !dir)
            lines.push(Format.formatFileSize(size))
        if (mtime !== undefined && mtime !== null && mtime >= 0)
            lines.push(Format.formatDate(new Date(mtime), Formatter.TimepointRelative))
        return lines
    }

    function newer(mine, theirs) {
        var known = typeof mine === "number" && typeof theirs === "number"
        //% "Newer"
        return known && mine > theirs ? qsTrId("lautta-conflict-newer") : ""
    }

    function rows() {
        var list = []
        if (offered("Merge"))
            //% "Merge"
            list.push({ "id": "Merge", "title": qsTrId("lautta-conflict-merge"),
                        //% "Combine the contents of both folders"
                        "hint": qsTrId("lautta-conflict-merge-hint") })
        //% "Keep both"
        list.push({ "id": "KeepBoth", "title": qsTrId("lautta-conflict-keep-both"),
                    //% "Saves as %1"
                    "hint": qsTrId("lautta-conflict-keep-both-hint").arg(Json.keepBothName(name)) })
        //% "Skip"
        list.push({ "id": "Skip", "title": qsTrId("lautta-conflict-skip"), "hint": "" })
        //% "Replace"
        list.push({ "id": "Replace", "title": qsTrId("lautta-conflict-replace"), "hint": "" })
        var incomingNewer = c.src_mtime_ms > c.dst_mtime_ms
                            //% "The incoming item is newer"
                            ? qsTrId("lautta-conflict-src-newer")
                            //% "The existing item is newer"
                            : qsTrId("lautta-conflict-dst-newer")
        //% "Replace if newer"
        list.push({ "id": "ReplaceIfNewer", "title": qsTrId("lautta-conflict-replace-newer"), "hint": incomingNewer })
        //% "Resume"
        list.push({ "id": "Resume", "title": qsTrId("lautta-conflict-resume"),
                    //% "Only for unfinished copies"
                    "hint": qsTrId("lautta-conflict-resume-hint") })
        return list
    }

    function offered(choiceName) {
        return Json.contains(choices, choiceName)
    }

    canAccept: choice.length > 0 && offered(choice)

    onAccepted: {
        if (transferId >= 0)
            Transfers.answer(transferId, item, choice, applyAll.checked)
        else
            Operations.resolveConflict(planId, item, choice, applyAll.checked)
    }

    SilicaFlickable {
        anchors.fill: parent
        contentHeight: column.height + Theme.paddingLarge

        Column {
            id: column

            width: parent.width

            DialogHeader {
                //% "Apply"
                acceptText: qsTrId("lautta-conflict-apply")
            }

            SectionHeader {
                objectName: "conflictPosition"
                text: dialog.total > 1
                      //% "Conflict %1 of %2"
                      ? qsTrId("lautta-conflict-position").arg(dialog.position).arg(dialog.total)
                      //% "Conflict"
                      : qsTrId("lautta-conflict-one")
            }

            Row {
                id: sides

                readonly property real columnWidth: (width - spacing) / 2

                x: Theme.horizontalPageMargin
                width: parent.width - 2 * Theme.horizontalPageMargin
                spacing: Theme.paddingLarge

                ItemTile {
                    objectName: "incomingTile"
                    width: sides.columnWidth
                    tileSize: Math.min(width, Screen.width * 0.4)
                    label: dialog.kind === "move"
                           //% "Moving"
                           ? qsTrId("lautta-conflict-moving")
                           //% "Copying"
                           : qsTrId("lautta-conflict-copying")
                    name: dialog.name
                    isDir: dialog.c.src_is_dir === true
                    uri: dialog.sourceUri
                    look: dialog.sourceLook
                    captions: dialog.captions(dialog.c.src_size, dialog.c.src_mtime_ms, isDir)
                    note: dialog.newer(dialog.c.src_mtime_ms, dialog.c.dst_mtime_ms)
                }
                ItemTile {
                    objectName: "existingTile"
                    width: sides.columnWidth
                    tileSize: Math.min(width, Screen.width * 0.4)
                    //% "Already there"
                    label: qsTrId("lautta-conflict-already-there")
                    name: dialog.name
                    isDir: dialog.c.dst_is_dir === true
                    uri: dialog.destinationUri
                    look: dialog.destinationLook
                    captions: dialog.captions(dialog.c.dst_size, dialog.c.dst_mtime_ms, isDir)
                    note: dialog.newer(dialog.c.dst_mtime_ms, dialog.c.src_mtime_ms)
                }
            }

            SectionHeader {
                //% "Resolution"
                text: qsTrId("lautta-conflict-resolution")
            }
            Repeater {
                model: dialog.rows()

                TextSwitch {
                    objectName: "choice" + modelData.id
                    automaticCheck: false
                    checked: dialog.choice === modelData.id
                    enabled: dialog.offered(modelData.id)
                    opacity: enabled ? 1.0 : Theme.opacityLow
                    text: modelData.title
                    description: modelData.hint
                    onClicked: dialog.choice = modelData.id
                }
            }
            Label {
                visible: !dialog.isDir || dialog.c.add_only === true
                x: Theme.horizontalPageMargin
                width: parent.width - 2 * Theme.horizontalPageMargin
                horizontalAlignment: Text.AlignRight
                wrapMode: Text.Wrap
                color: Theme.secondaryColor
                font.pixelSize: Theme.fontSizeExtraSmall
                text: dialog.c.add_only === true
                      //% "Lautta may only add files here."
                      ? qsTrId("lautta-conflict-add-only")
                      //% "Merge is only offered for folders."
                      : qsTrId("lautta-conflict-merge-folders")
            }

            SectionHeader {
                visible: applyAll.visible
                //% "Further conflicts"
                text: qsTrId("lautta-conflict-further")
            }
            TextSwitch {
                id: applyAll

                objectName: "applyAll"
                visible: dialog.remaining > 0
                //% "Do this for the remaining %n conflicts"
                text: qsTrId("lautta-conflict-apply-all", dialog.remaining)
            }
        }
        VerticalScrollDecorator { }
    }
}
