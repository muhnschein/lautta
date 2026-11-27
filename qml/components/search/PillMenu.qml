// SPDX-License-Identifier: LGPL-2.1-or-later
import QtQuick 2.6
import Sailfish.Silica 1.0

ContextMenu {
    id: menu

    property string current
    property var choices: []

    signal chosen(string value)

    Repeater {
        model: menu.choices

        MenuItem {
            objectName: "pillChoice"
            readonly property string value: modelData.value
            text: modelData.label
            highlighted: modelData.value === menu.current
            onClicked: menu.chosen(modelData.value)
        }
    }
}
