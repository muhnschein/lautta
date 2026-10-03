// SPDX-License-Identifier: LGPL-2.1-or-later
// The remote file changed while the user edited a working copy (SPEC EDT-2,
// board EditConflict): upload mine and replace, save mine as a copy, or
// discard mine. Nothing is overwritten unless the user chooses it.
import QtQuick 2.6
import Sailfish.Silica 1.0
import Lautta 1.0
import "../components/transfers/TransferText.js" as TransferText

Dialog {
    id: dialog

    property int copyId
    // The conflict as loaded from the core; null while loading or when it
    // has been resolved elsewhere.
    property var conflict: null
    property bool loaded
    // Never replace by default (OPS-2 spirit): saving a copy loses nothing.
    property string choice: "save_copy"

    allowedOrientations: Orientation.All
    canAccept: conflict !== null && conflict.choices.indexOf(choice) >= 0
    onAccepted: Transfers.resolveEditConflict(copyId, choice)

    WorkingCopiesModel {
        id: copies

        onConflictLoaded: {
            dialog.conflict = conflictJson.length > 0 ? JSON.parse(conflictJson) : null
            dialog.loaded = true
        }
    }

    Component.onCompleted: copies.loadConflict(copyId)

    SilicaFlickable {
        anchors.fill: parent
        contentHeight: column.height

        Column {
            id: column

            width: parent.width

            DialogHeader {
                //% "Apply"
                acceptText: qsTrId("lautta-xfr-edit-apply")
            }
            Label {
                x: Theme.horizontalPageMargin
                width: parent.width - 2 * Theme.horizontalPageMargin
                visible: dialog.conflict !== null
                horizontalAlignment: Text.AlignRight
                color: Theme.secondaryHighlightColor
                font.pixelSize: Theme.fontSizeSmall
                text: dialog.conflict
                    //% "%1 changed on %2"
                    ? qsTrId("lautta-xfr-edit-changed-on").arg(dialog.conflict.name).arg(dialog.conflict.address)
                    : ""
                wrapMode: Text.WordWrap
            }
            Label {
                x: Theme.horizontalPageMargin
                width: parent.width - 2 * Theme.horizontalPageMargin
                visible: dialog.conflict !== null
                height: visible ? implicitHeight + Theme.paddingLarge : 0
                wrapMode: Text.WordWrap
                color: Theme.primaryColor
                text: dialog.conflict && dialog.conflict.remoteGone
                    //% "%1 was removed from the server after you started editing."
                    ? qsTrId("lautta-xfr-edit-gone").arg(dialog.conflict.name)
                    //% "Someone changed %1 on the server after you started editing."
                    : (dialog.conflict ? qsTrId("lautta-xfr-edit-explain").arg(dialog.conflict.name) : "")
            }
            DetailItem {
                visible: dialog.conflict !== null
                //% "Yours"
                label: qsTrId("lautta-xfr-edit-yours")
                value: dialog.conflict ? TransferText.size(dialog.conflict.localSize) : ""
            }
            DetailItem {
                visible: dialog.conflict !== null && !dialog.conflict.remoteGone
                //% "On server"
                label: qsTrId("lautta-xfr-edit-server")
                value: {
                    if (!dialog.conflict || dialog.conflict.remoteGone)
                        return ""
                    var c = dialog.conflict
                    var size = c.remoteSize === null ? "" : TransferText.size(c.remoteSize)
                    if (c.remoteMtimeMs === null)
                        return size
                    return size.length > 0 ? TransferText.when(c.remoteMtimeMs) + " · " + size : TransferText.when(c.remoteMtimeMs)
                }
            }
            SectionHeader {
                visible: dialog.conflict !== null
                //% "What to do"
                text: qsTrId("lautta-xfr-edit-what")
            }
            TextSwitch {
                visible: dialog.conflict !== null
                automaticCheck: false
                checked: dialog.choice === "upload_replace"
                //% "Upload mine and replace"
                text: qsTrId("lautta-xfr-edit-upload-replace")
                description: dialog.conflict && dialog.conflict.remoteGone
                    //% "The file is created again on the server."
                    ? qsTrId("lautta-xfr-edit-upload-recreate")
                    //% "The version on the server is overwritten."
                    : qsTrId("lautta-xfr-edit-upload-overwrite")
                onClicked: dialog.choice = "upload_replace"
            }
            TextSwitch {
                visible: dialog.conflict !== null
                automaticCheck: false
                checked: dialog.choice === "save_copy"
                //% "Save mine as a copy"
                text: qsTrId("lautta-xfr-edit-save-copy")
                //% "Saved next to the original, under a free name."
                description: qsTrId("lautta-xfr-edit-save-copy-hint")
                onClicked: dialog.choice = "save_copy"
            }
            TextSwitch {
                visible: dialog.conflict !== null
                automaticCheck: false
                checked: dialog.choice === "discard"
                //% "Discard mine"
                text: qsTrId("lautta-xfr-edit-discard")
                description: dialog.conflict && dialog.conflict.remoteGone
                    //% "Forget the changes."
                    ? qsTrId("lautta-xfr-edit-discard-gone")
                    //% "Keep the version on the server."
                    : qsTrId("lautta-xfr-edit-discard-hint")
                onClicked: dialog.choice = "discard"
            }
        }

        ViewPlaceholder {
            enabled: dialog.loaded && dialog.conflict === null
            //% "Nothing to resolve"
            text: qsTrId("lautta-xfr-edit-none")
            //% "The file is up to date."
            hintText: qsTrId("lautta-xfr-edit-none-hint")
        }
    }
}
