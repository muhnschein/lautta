// SPDX-License-Identifier: LGPL-2.1-or-later
// Change the colour of a tag (ORG-3).
import QtQuick 2.6
import Sailfish.Silica 1.0

Dialog {
    id: dialog

    property string colour

    allowedOrientations: Orientation.All
    canAccept: colour.length > 0

    Column {
        width: parent.width

        DialogHeader {
            //% "Change"
            acceptText: qsTrId("lautta-tag-colour-accept")
            //% "Change colour"
            title: qsTrId("lautta-tag-colour-title")
        }
        ColourPicker {
            allowNone: false
            colour: dialog.colour
            onPicked: dialog.colour = colour
        }
    }
}
