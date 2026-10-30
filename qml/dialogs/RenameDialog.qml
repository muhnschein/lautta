// SPDX-License-Identifier: LGPL-2.1-or-later Only asks; caller renames on accept.
import QtQuick 2.6
import Sailfish.Silica 1.0
import Lautta 1.0
import "../components/ErrorText.js" as ErrorText
import "../components/directory/Names.js" as Names

Dialog {
    id: dialog

    property string uri
    property bool isDir
    readonly property string oldName: App.nameOf(uri)
    readonly property string newName: nameField.text
    readonly property bool taken: nameField.text !== oldName && parentModel.count >= 0
                                  && parentModel.nameKind(nameField.text) !== ""
    readonly property string folderUri: App.parentUri(uri)
    readonly property bool badName: Names.invalid(nameField.text)

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

        uri: dialog.folderUri
    }

    SilicaFlickable {
        anchors.fill: parent
        contentHeight: column.height

        Column {
            id: column

            width: parent.width

            DialogHeader {
                //% "Rename"
                title: qsTrId("lautta-dir-rename-title")
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
        }
    }

    Component.onCompleted: {
        nameField.forceActiveFocus()
        var dot = dialog.oldName.lastIndexOf(".")
        nameField.select(0, dot > 0 && !dialog.isDir ? dot : dialog.oldName.length)
    }
}
