// SPDX-License-Identifier: LGPL-2.1-or-later
// Rename a tag (ORG-3).
import QtQuick 2.6
import Sailfish.Silica 1.0

Dialog {
    id: dialog

    property string tagName

    allowedOrientations: Orientation.All
    canAccept: nameField.text.trim().length > 0

    Column {
        width: parent.width

        DialogHeader {
            //% "Rename"
            acceptText: qsTrId("lautta-tag-rename-accept")
            //% "Rename tag"
            title: qsTrId("lautta-tag-rename-title")
        }
        TextField {
            id: nameField
            width: parent.width
            text: dialog.tagName
            //% "Name"
            label: qsTrId("lautta-tags-name")
            placeholderText: label
            focus: true
            EnterKey.iconSource: "image://theme/icon-m-enter-accept"
            EnterKey.onClicked: dialog.accept()
        }
    }

    // The new name, for the caller to read after `accepted`.
    readonly property string newName: nameField.text.trim()
}
