// SPDX-License-Identifier: LGPL-2.1-or-later
import QtQuick 2.6
import Sailfish.Silica 1.0
import "../FileIcons.js" as FileIcons

Item {
    id: root

    property alias text: label.text
    property string icon: ""
    property bool checked: false
    property bool open: false
    readonly property bool down: mouse.pressed && mouse.containsMouse
    readonly property bool lit: checked || open || down
    readonly property color ink: lit ? Theme.highlightColor : Theme.primaryColor

    signal clicked()
    signal pressAndHold()

    width: content.width + (icon !== "" ? Theme.paddingMedium + Theme.paddingLarge : 2 * Theme.paddingLarge)
    height: label.height + 2 * Theme.paddingMedium

    Rectangle {
        objectName: "pillBacking"
        anchors.fill: parent
        radius: height / 2
        color: root.lit
               ? Theme.rgba(Theme.highlightBackgroundColor,
                            root.down ? Theme.highlightBackgroundOpacity * 1.5
                                      : Theme.highlightBackgroundOpacity)
               : Theme.rgba(Theme.primaryColor, 0.1)
    }

    Row {
        id: content

        x: root.icon !== "" ? Theme.paddingMedium : Theme.paddingLarge
        anchors.verticalCenter: parent.verticalCenter
        spacing: root.icon !== "" ? Theme.paddingSmall : 0

        HighlightImage {
            objectName: "pillIcon"
            visible: root.icon !== ""
            anchors.verticalCenter: parent.verticalCenter
            width: visible ? Theme.iconSizeSmallPlus : 0
            height: Theme.iconSizeSmallPlus
            sourceSize.width: Theme.iconSizeSmallPlus
            sourceSize.height: Theme.iconSizeSmallPlus
            source: root.icon !== "" ? FileIcons.source(root.icon, Theme.pixelRatio) : ""
            color: root.ink
            highlightColor: root.ink
        }

        Label {
            id: label

            objectName: "pillLabel"
            anchors.verticalCenter: parent.verticalCenter
            font.pixelSize: Theme.fontSizeMedium
            color: root.ink
        }
    }

    MouseArea {
        id: mouse

        anchors.fill: parent
        anchors.margins: -Theme.paddingSmall
        onClicked: root.clicked()
        onPressAndHold: root.pressAndHold()
    }
}
