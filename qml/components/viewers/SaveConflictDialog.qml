// SPDX-License-Identifier: LGPL-2.1-or-later
// The file changed (or vanished) while it was being edited (EDT-2): upload
// mine and replace, save mine as a copy, or discard mine. `choice` is
// "replace", "copy" or "discard" once accepted.
import QtQuick 2.6
import Sailfish.Silica 1.0

Dialog {
    id: dialog

    property string fileName
    property bool deleted
    property string choice: "replace"

    allowedOrientations: Orientation.All
    onAccepted: choice = ["replace", "copy", "discard"][picker.currentIndex]

    Column {
        width: parent.width

        DialogHeader {
            //% "Continue"
            acceptText: qsTrId("lautta-viewers-conflict-continue")
        }

        Label {
            x: Theme.horizontalPageMargin
            width: parent.width - 2 * Theme.horizontalPageMargin
            wrapMode: Text.Wrap
            color: Theme.highlightColor
            font.pixelSize: Theme.fontSizeLarge
            text: dialog.deleted
                  //% "%1 was deleted while you were editing it."
                  ? qsTrId("lautta-viewers-conflict-deleted").arg(dialog.fileName)
                  //% "%1 changed while you were editing it."
                  : qsTrId("lautta-viewers-conflict-changed").arg(dialog.fileName)
        }

        Item { width: 1; height: Theme.paddingLarge }

        ComboBox {
            id: picker
            //% "What to do"
            label: qsTrId("lautta-viewers-conflict-what")
            menu: ContextMenu {
                MenuItem {
                    //% "Upload mine and replace"
                    text: qsTrId("lautta-viewers-conflict-replace")
                }
                MenuItem {
                    //% "Save mine as copy"
                    text: qsTrId("lautta-viewers-conflict-copy")
                }
                MenuItem {
                    //% "Discard mine"
                    text: qsTrId("lautta-viewers-conflict-discard")
                }
            }
        }
    }
}
