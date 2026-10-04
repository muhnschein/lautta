// SPDX-License-Identifier: LGPL-2.1-or-later
// Rename one item (board Rename). The dialog only asks: after it is
// accepted the caller calls `DirectoryModel.rename(uri, newName)`. A name
// that is taken is an error right away; there is no case hint.
import QtQuick 2.6
import Sailfish.Silica 1.0
import Lautta 1.0
import "../components/ErrorText.js" as ErrorText
import "../components/directory/DirFormat.js" as DirFormat

Dialog {
    id: dialog

    property string uri
    // Shown under the field when known (-1: unknown).
    property real size: -1
    property real modified: -1
    property bool isDir
    readonly property string oldName: App.nameOf(uri)
    readonly property string newName: nameField.text
    readonly property bool taken: nameField.text !== oldName && parentModel.count >= 0
                                  && parentModel.nameKind(nameField.text) !== ""
    readonly property bool badName: nameField.text.indexOf("/") >= 0 || nameField.text === "." || nameField.text === ".."

    function errorText() {
        if (taken)
            return ErrorText.message("AlreadyExists", { "item": nameField.text })
        if (badName)
            return ErrorText.message("InvalidName", {})
        return ""
    }

    allowedOrientations: Orientation.All
    canAccept: nameField.text.length > 0 && nameField.text !== oldName && !taken && !badName

    DirectoryModel {
        id: parentModel

        uri: App.parentUri(dialog.uri)
    }

    PathModel {
        id: where

        uri: App.parentUri(dialog.uri)
    }

    SilicaFlickable {
        anchors.fill: parent
        contentHeight: column.height

        Column {
            id: column

            width: parent.width

            DialogHeader {
                //% "Rename"
                acceptText: qsTrId("lautta-dir-rename-accept")
                //% "Rename · %1"
                title: qsTrId("lautta-dir-rename-title").arg(dialog.oldName)
            }

            TextField {
                id: nameField

                width: parent.width
                text: dialog.oldName
                errorHighlight: dialog.errorText().length > 0
                //% "Name"
                placeholderText: qsTrId("lautta-dir-rename-name")
                label: errorHighlight ? dialog.errorText() : placeholderText
                inputMethodHints: Qt.ImhNoPredictiveText
                EnterKey.enabled: dialog.canAccept
                EnterKey.iconSource: "image://theme/icon-m-enter-accept"
                EnterKey.onClicked: dialog.accept()
            }

            Item {
                width: 1
                height: Theme.paddingLarge
            }

            DetailItem {
                //% "Location"
                label: qsTrId("lautta-dir-rename-location")
                value: where.fullPath
            }
            DetailItem {
                visible: !dialog.isDir && dialog.size >= 0
                //% "Size"
                label: qsTrId("lautta-dir-rename-size")
                value: Format.formatFileSize(dialog.size)
            }
            DetailItem {
                visible: dialog.modified >= 0
                //% "Modified"
                label: qsTrId("lautta-dir-rename-modified")
                value: DirFormat.modified(dialog.modified)
            }
        }
    }

    Component.onCompleted: {
        nameField.forceActiveFocus()
        // Select the name without its extension, like the system does.
        var dot = dialog.oldName.lastIndexOf(".")
        nameField.select(0, dot > 0 && !dialog.isDir ? dot : dialog.oldName.length)
    }
}
