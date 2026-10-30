// SPDX-License-Identifier: LGPL-2.1-or-later
import QtQuick 2.6
import Sailfish.Silica 1.0

Item {
    id: line

    property string before
    property string text
    property bool remote
    property color color: Theme.secondaryColor
    property real fontSize: Theme.fontSizeExtraSmall
    property int horizontalAlignment: Text.AlignLeft
    readonly property bool iconShown: remote && text.length > 0

    readonly property real markWidth: iconShown ? icon.width + Theme.paddingSmall : 0
    readonly property real leadWidth: before.length > 0
                                      ? Math.min(lead.implicitWidth,
                                                 Math.max(width / 2, width - markWidth - name.implicitWidth))
                                      : 0
    readonly property real nameWidth: Math.min(name.implicitWidth, Math.max(0, width - leadWidth - markWidth))
    readonly property real start: horizontalAlignment === Text.AlignRight
                                  ? width - leadWidth - markWidth - nameWidth : 0

    implicitWidth: lead.implicitWidth + markWidth + name.implicitWidth
    implicitHeight: name.implicitHeight
    height: implicitHeight

    Label {
        id: lead

        visible: line.before.length > 0
        x: line.start
        width: line.leadWidth
        anchors.verticalCenter: parent.verticalCenter
        text: line.before
        textFormat: Text.PlainText
        color: line.color
        font.pixelSize: line.fontSize
        truncationMode: TruncationMode.Fade
    }

    HighlightImage {
        id: icon

        objectName: "remoteIcon"
        visible: line.iconShown
        x: line.start + line.leadWidth
        anchors.verticalCenter: parent.verticalCenter
        source: "image://theme/icon-m-computer"
        width: Math.round(line.fontSize * 1.25)
        height: width
        sourceSize.width: width
        sourceSize.height: height
        color: line.color
    }

    Label {
        id: name

        x: line.start + line.leadWidth + line.markWidth
        width: line.nameWidth
        anchors.verticalCenter: parent.verticalCenter
        text: line.text
        textFormat: Text.PlainText
        color: line.color
        font.pixelSize: line.fontSize
        truncationMode: TruncationMode.Fade
    }
}
