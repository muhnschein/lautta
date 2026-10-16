// SPDX-License-Identifier: LGPL-2.1-or-later
// The password of an encrypted PDF (PRV-12, board Password dialog): swipe
// forward or Unlock opens it, back or Cancel leaves it locked. The password
// goes to PdfDocument.unlock and is not kept anywhere.
import QtQuick 2.6
import Sailfish.Silica 1.0

Dialog {
    id: dialog

    property string fileName
    // The password given before did not open the PDF.
    property bool wrong
    readonly property string password: field.text

    allowedOrientations: Orientation.All
    canAccept: field.text.length > 0

    // Called by the opener after reading `password`: nothing of it stays.
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
                acceptText: qsTrId("lautta-viewers-pdf-unlock")
            }

            Column {
                x: Theme.horizontalPageMargin
                width: parent.width - 2 * x
                topPadding: Theme.paddingLarge * 2
                bottomPadding: Theme.paddingLarge * 2
                spacing: Theme.paddingMedium

                HighlightImage {
                    anchors.horizontalCenter: parent.horizontalCenter
                    source: Qt.resolvedUrl("../icons/lock.svg")
                    sourceSize.width: Theme.iconSizeLarge
                    sourceSize.height: Theme.iconSizeLarge
                    color: Theme.highlightColor
                }

                Label {
                    width: parent.width
                    horizontalAlignment: Text.AlignHCenter
                    font.pixelSize: Theme.fontSizeExtraLarge
                    font.family: Theme.fontFamilyHeading
                    color: Theme.highlightColor
                    //% "Locked PDF"
                    text: qsTrId("lautta-viewers-pdf-locked")
                }

                Label {
                    width: parent.width
                    horizontalAlignment: Text.AlignHCenter
                    wrapMode: Text.Wrap
                    font.pixelSize: Theme.fontSizeLarge
                    text: dialog.fileName
                }
            }

            PasswordField {
                id: field
                errorHighlight: dialog.wrong && text.length === 0
                //% "Wrong password, try again"
                label: dialog.wrong ? qsTrId("lautta-viewers-pdf-wrong-password")
                                    //% "Password"
                                    : qsTrId("lautta-viewers-pdf-password")
                EnterKey.enabled: text.length > 0
                EnterKey.iconSource: "image://theme/icon-m-enter-accept"
                EnterKey.onClicked: dialog.accept()
            }
        }
    }
}
