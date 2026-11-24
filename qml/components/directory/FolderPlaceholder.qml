// SPDX-License-Identifier: LGPL-2.1-or-later
import QtQuick 2.6
import Sailfish.Silica 1.0

Column {
    id: placeholder

    property string title
    property string hint
    property var buttons: []
    property string level

    readonly property bool warning: level.length > 0
    readonly property bool security: level === "security"

    signal triggered(string action)

    spacing: Theme.paddingLarge
    width: parent ? parent.width - 2 * Theme.horizontalPageMargin : 0

    HighlightImage {
        objectName: "placeholderIcon"
        visible: placeholder.warning
        anchors.horizontalCenter: parent.horizontalCenter
        source: visible ? "image://theme/icon-l-attention" : ""
        color: placeholder.security ? Theme.errorColor : Theme.highlightColor
    }

    Column {
        width: parent.width
        spacing: Theme.paddingSmall

        Label {
            objectName: "placeholderKicker"
            visible: placeholder.security
            width: parent.width
            horizontalAlignment: Text.AlignHCenter
            //% "Security warning"
            text: qsTrId("lautta-attention-security-warning")
            font {
                pixelSize: Theme.fontSizeSmall
                capitalization: Font.AllUppercase
            }
            color: Theme.errorColor
        }

        Label {
            objectName: "placeholderTitle"
            width: parent.width
            horizontalAlignment: Text.AlignHCenter
            wrapMode: Text.Wrap
            text: placeholder.title
            font {
                pixelSize: Theme.fontSizeExtraLarge
                family: Theme.fontFamilyHeading
            }
            color: placeholder.security ? Theme.errorColor : Theme.highlightColor
            opacity: placeholder.warning ? 1.0 : Theme.opacityHigh
        }
    }

    Label {
        visible: text.length > 0
        width: parent.width
        horizontalAlignment: Text.AlignHCenter
        wrapMode: Text.Wrap
        text: placeholder.hint
        color: Theme.secondaryHighlightColor
    }

    Grid {
        anchors.horizontalCenter: parent.horizontalCenter
        columns: placeholder.warning ? 1 : Math.max(1, placeholder.buttons.length)
        spacing: Theme.paddingLarge
        visible: placeholder.buttons.length > 0

        Repeater {
            model: placeholder.buttons

            Button {
                objectName: "placeholderButton"
                text: modelData.text
                preferredWidth: placeholder.warning ? Math.min(Theme.buttonWidthLarge, placeholder.width)
                                                    : Theme.buttonWidthSmall
                backgroundColor: modelData.primary
                                 ? Theme.rgba(Theme.highlightBackgroundColor, Theme.highlightBackgroundOpacity)
                                 : Theme.rgba(color, Theme.opacityFaint)
                onClicked: placeholder.triggered(modelData.action)
            }
        }
    }
}
