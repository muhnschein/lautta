// SPDX-License-Identifier: LGPL-2.1-or-later
// New folder or empty file in `parentUri` (board NewItem). The dialog only
// asks: after it is accepted the caller creates the item with
// `DirectoryModel.createFolder/createFile(itemName)` (`isFolder` says which).
// A name that already exists is shown as an error instead of a hint, and the
// button stays off.
import QtQuick 2.6
import Sailfish.Silica 1.0
import Lautta 1.0
import "../components/ErrorText.js" as ErrorText

Dialog {
    id: dialog

    property string parentUri
    // Folders only (the folder picker's "New folder"): hides the type.
    property bool foldersOnly
    readonly property string itemName: nameField.text
    readonly property bool isFolder: foldersOnly || typeBox.currentIndex === 0
    // "folder", "file" or "" for what already has this name here.
    readonly property string clash: parentModel.count >= 0 ? parentModel.nameKind(nameField.text) : ""
    readonly property bool badName: nameField.text.indexOf("/") >= 0 || nameField.text === "." || nameField.text === ".."

    function errorText() {
        if (clash === "folder")
            //% "A folder named “%1” already exists"
            return qsTrId("lautta-dir-new-exists-folder").arg(nameField.text)
        if (clash === "file")
            //% "A file named “%1” already exists"
            return qsTrId("lautta-dir-new-exists-file").arg(nameField.text)
        if (badName)
            return ErrorText.message("InvalidName", {})
        return ""
    }

    allowedOrientations: Orientation.All
    canAccept: nameField.text.length > 0 && clash === "" && !badName

    DirectoryModel {
        id: parentModel

        uri: dialog.parentUri
    }

    SilicaFlickable {
        anchors.fill: parent
        contentHeight: column.height

        Column {
            id: column

            width: parent.width

            DialogHeader {
                //% "Create"
                acceptText: qsTrId("lautta-dir-new-create")
                //% "New in %1"
                title: qsTrId("lautta-dir-new-title").arg(App.nameOf(dialog.parentUri))
            }

            ComboBox {
                id: typeBox

                visible: !dialog.foldersOnly
                //% "Type"
                label: qsTrId("lautta-dir-new-type")
                menu: ContextMenu {
                    MenuItem {
                        //% "Folder"
                        text: qsTrId("lautta-dir-new-type-folder")
                    }
                    MenuItem {
                        //% "File"
                        text: qsTrId("lautta-dir-new-type-file")
                    }
                }
            }

            TextField {
                id: nameField

                width: parent.width
                focus: true
                errorHighlight: dialog.errorText().length > 0
                //% "Name"
                placeholderText: qsTrId("lautta-dir-new-name")
                label: errorHighlight ? dialog.errorText() : placeholderText
                inputMethodHints: Qt.ImhNoPredictiveText
                EnterKey.enabled: dialog.canAccept
                EnterKey.iconSource: "image://theme/icon-m-enter-accept"
                EnterKey.onClicked: dialog.accept()
            }
        }
    }

    Component.onCompleted: nameField.forceActiveFocus()
}
