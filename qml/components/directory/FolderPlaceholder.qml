// SPDX-License-Identifier: LGPL-2.1-or-later
// Big light text with a hint and optional buttons, for the empty, not
// accessible and error states of a folder (boards StateEmpty, StateNoAccess,
// StateError). Buttons are { text, action } objects; `triggered(action)`.
import QtQuick 2.6
import Sailfish.Silica 1.0

Column {
    id: placeholder

    property string title
    property string hint
    property var buttons: []

    signal triggered(string action)

    spacing: Theme.paddingLarge
    width: parent ? parent.width - 2 * Theme.horizontalPageMargin : 0

    Label {
        width: parent.width
        horizontalAlignment: Text.AlignHCenter
        wrapMode: Text.Wrap
        text: placeholder.title
        font {
            pixelSize: Theme.fontSizeExtraLarge
            family: Theme.fontFamilyHeading
        }
        color: Theme.highlightColor
        opacity: Theme.opacityHigh
    }

    Label {
        visible: text.length > 0
        width: parent.width
        horizontalAlignment: Text.AlignHCenter
        wrapMode: Text.Wrap
        text: placeholder.hint
        color: Theme.secondaryHighlightColor
    }

    Row {
        anchors.horizontalCenter: parent.horizontalCenter
        spacing: Theme.paddingLarge
        visible: placeholder.buttons.length > 0

        Repeater {
            model: placeholder.buttons

            Button {
                text: modelData.text
                onClicked: placeholder.triggered(modelData.action)
            }
        }
    }
}
