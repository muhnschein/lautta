// SPDX-License-Identifier: LGPL-2.1-or-later
// One conflict (OPS-2): what is copied, what is already there, and the
// resolution. The default is never Replace. A conflict before the run belongs
// to a plan (planId, answered through Operations); one during the run belongs
// to a transfer (transferId, answered through Transfers).
import QtQuick 2.6
import Sailfish.Silica 1.0
import Lautta 1.0
import "../components/operations"
import "../components/operations/Json.js" as Json

Dialog {
    id: dialog

    property int planId: -1
    property int transferId: -1
    // Index of the item in the plan or transfer.
    property int item: 0
    // The Conflict of the core, as object or JSON text.
    property var conflict
    property string name
    // Where it already exists (breadcrumb text).
    property string folder
    // "copy", "move", …: only changes the wording of the first section.
    property string kind: "copy"
    // Engineering name of the preselected choice; empty = first safe one.
    property string defaultChoice
    // "3 of 12": position among the conflicts shown now.
    property int position: 1
    property int total: 1

    readonly property var c: Json.value(conflict, {})
    readonly property var choices: c.choices || []
    readonly property bool isDir: c.src_is_dir === true || c.dst_is_dir === true
    property string choice: initialChoice()

    function initialChoice() {
        if (defaultChoice.length > 0 && defaultChoice !== "Replace" && Json.contains(choices, defaultChoice))
            return defaultChoice
        for (var i = 0; i < choices.length; ++i)
            if (choices[i] !== "Replace")
                return choices[i]
        return ""
    }

    function details(size, mtime) {
        var parts = []
        if (size !== undefined && size !== null && !isDir)
            parts.push(Format.formatFileSize(size))
        if (mtime !== undefined && mtime !== null && mtime >= 0)
            parts.push(Format.formatDate(new Date(mtime), Formatter.TimepointRelative))
        return parts.join(" · ")
    }

    // The resolution rows in display order; Merge only for folders (OPS-2).
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
        var newer = c.src_mtime_ms > c.dst_mtime_ms
                    //% "Copying is newer"
                    ? qsTrId("lautta-conflict-src-newer")
                    //% "The existing item is newer"
                    : qsTrId("lautta-conflict-dst-newer")
        //% "Replace if newer"
        list.push({ "id": "ReplaceIfNewer", "title": qsTrId("lautta-conflict-replace-newer"), "hint": newer })
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
        contentHeight: column.height

        Column {
            id: column
            width: parent.width

            DialogHeader {
                //% "Apply"
                acceptText: qsTrId("lautta-conflict-apply")
            }
            DialogSubtitle {
                visible: dialog.total > 1
                //% "%n conflicts · %1 of %2"
                text: qsTrId("lautta-conflict-progress", dialog.total).arg(dialog.position).arg(dialog.total)
            }

            SectionHeader {
                text: dialog.kind === "move"
                      //% "Moving"
                      ? qsTrId("lautta-conflict-moving")
                      //% "Copying"
                      : qsTrId("lautta-conflict-copying")
            }
            Row {
                x: Theme.horizontalPageMargin
                width: parent.width - 2 * Theme.horizontalPageMargin
                height: Theme.itemSizeMedium
                spacing: Theme.paddingMedium

                OpIcon {
                    anchors.verticalCenter: parent.verticalCenter
                    isDir: dialog.c.src_is_dir === true
                    category: App.categoryOf(dialog.name)
                }
                Column {
                    anchors.verticalCenter: parent.verticalCenter
                    width: parent.width - Theme.iconSizeMedium - Theme.paddingMedium
                    Label {
                        width: parent.width
                        text: dialog.name
                        truncationMode: TruncationMode.Fade
                    }
                    Label {
                        width: parent.width
                        text: dialog.details(dialog.c.src_size, dialog.c.src_mtime_ms)
                        color: Theme.secondaryColor
                        font.pixelSize: Theme.fontSizeSmall
                        truncationMode: TruncationMode.Fade
                    }
                }
            }

            SectionHeader {
                //% "Already in %1"
                text: qsTrId("lautta-conflict-already-in").arg(dialog.folder)
            }
            Row {
                x: Theme.horizontalPageMargin
                width: parent.width - 2 * Theme.horizontalPageMargin
                height: Theme.itemSizeMedium
                spacing: Theme.paddingMedium

                OpIcon {
                    anchors.verticalCenter: parent.verticalCenter
                    isDir: dialog.c.dst_is_dir === true
                    category: App.categoryOf(dialog.name)
                }
                Column {
                    anchors.verticalCenter: parent.verticalCenter
                    width: parent.width - Theme.iconSizeMedium - Theme.paddingMedium
                    Label {
                        width: parent.width
                        text: dialog.name
                        truncationMode: TruncationMode.Fade
                    }
                    Label {
                        width: parent.width
                        text: dialog.details(dialog.c.dst_size, dialog.c.dst_mtime_ms)
                        color: Theme.secondaryColor
                        font.pixelSize: Theme.fontSizeSmall
                        truncationMode: TruncationMode.Fade
                    }
                }
            }

            SectionHeader {
                //% "Resolution"
                text: qsTrId("lautta-conflict-resolution")
            }
            Repeater {
                model: dialog.rows()

                ChoiceRow {
                    width: column.width
                    choiceName: modelData.id
                    title: modelData.title
                    hint: modelData.hint
                    selected: dialog.choice
                    offeredChoices: dialog.choices
                    onChosen: dialog.choice = name
                }
            }
            Label {
                visible: !dialog.isDir
                x: Theme.horizontalPageMargin
                width: parent.width - 2 * Theme.horizontalPageMargin
                horizontalAlignment: Text.AlignRight
                wrapMode: Text.Wrap
                color: Theme.secondaryColor
                font.pixelSize: Theme.fontSizeExtraSmall
                //% "Merge is offered for folders."
                text: qsTrId("lautta-conflict-merge-folders")
            }
            TextSwitch {
                id: applyAll
                visible: dialog.total - dialog.position > 0
                //% "Do this for the remaining %n conflicts"
                text: qsTrId("lautta-conflict-apply-all", dialog.total - dialog.position)
            }
        }
        VerticalScrollDecorator { }
    }
}
