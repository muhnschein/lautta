// SPDX-License-Identifier: LGPL-2.1-or-later
import QtQuick 2.6
import Sailfish.Silica 1.0

Dialog {
    id: dialog

    property bool wrong
    readonly property string password: field.text

    allowedOrientations: Orientation.All
    canAccept: field.text.length > 0

    function forget() {
        field.text = ""
    }

    onOpened: field.forceActiveFocus()
    Component.onDestruction: forget()

    SilicaFlickable {
        anchors.fill: parent
        contentHeight: column.height + Theme.paddingLarge

        Column {
            id: column
            width: parent.width

            DialogHeader {
                //% "Unlock"
                acceptText: qsTrId("lautta-locked-unlock")
            }

            Column {
                x: Theme.horizontalPageMargin
                width: parent.width - 2 * x
                topPadding: Theme.paddingLarge * 4
                bottomPadding: Theme.paddingLarge * 4

                Label {
                    width: parent.width
                    horizontalAlignment: Text.AlignHCenter
                    wrapMode: Text.Wrap
                    font.pixelSize: Theme.fontSizeExtraLarge
                    font.family: Theme.fontFamilyHeading
                    color: Theme.highlightColor
                    //% "Locked file"
                    text: qsTrId("lautta-locked-title")
                }
            }

            PasswordField {
                id: field
                errorHighlight: dialog.wrong && text.length === 0
                //% "Wrong password. Try again."
                label: dialog.wrong ? qsTrId("lautta-locked-wrong")
                                    //% "Password"
                                    : qsTrId("lautta-locked-password")
                EnterKey.enabled: text.length > 0
                EnterKey.iconSource: "image://theme/icon-m-enter-accept"
                EnterKey.onClicked: dialog.accept()
            }
        }
    }
}
