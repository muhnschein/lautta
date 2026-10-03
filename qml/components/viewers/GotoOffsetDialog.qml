// SPDX-License-Identifier: LGPL-2.1-or-later
// "Go to offset" of the hex viewer: a hexadecimal offset inside the file.
import QtQuick 2.6
import Sailfish.Silica 1.0

Dialog {
    id: dialog

    // File size in bytes.
    property real size
    property string offsetText

    readonly property real parsed: parseInt(field.text.replace(/^\s*0x/i, "").replace(/\s+/g, ""), 16)

    allowedOrientations: Orientation.All
    canAccept: field.acceptableInput && parsed < size
    onAccepted: offsetText = field.text

    Column {
        width: parent.width

        DialogHeader {
            //% "Go"
            acceptText: qsTrId("lautta-viewers-goto-accept")
        }

        TextField {
            id: field
            width: parent.width
            //% "Offset (hexadecimal)"
            label: qsTrId("lautta-viewers-goto-label")
            placeholderText: label
            inputMethodHints: Qt.ImhNoPredictiveText | Qt.ImhNoAutoUppercase | Qt.ImhPreferUppercase
            validator: RegExpValidator { regExp: /^\s*(0x)?[0-9a-fA-F ]+$/ }
            EnterKey.iconSource: "image://theme/icon-m-enter-accept"
            EnterKey.onClicked: if (dialog.canAccept) dialog.accept()
            Component.onCompleted: forceActiveFocus()
        }
    }
}
