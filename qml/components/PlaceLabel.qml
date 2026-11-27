// SPDX-License-Identifier: LGPL-2.1-or-later
import QtQuick 2.6
import Sailfish.Silica 1.0
import "FileIcons.js" as FileIcons

Item {
    id: line

    property string before
    property string text
    property bool remote
    property color color: Theme.secondaryColor
    property real fontSize: Theme.fontSizeExtraSmall
    property int horizontalAlignment: Text.AlignLeft
    property int maximumLineCount: 1
    readonly property bool iconShown: remote && text.length > 0
    readonly property bool split: maximumLineCount > 1 && before.length > 0
                                  && lead.implicitWidth + markWidth + nameMetrics.advanceWidth > width
    readonly property bool wraps: maximumLineCount > 1 && before.length === 0 && nameMetrics.advanceWidth > width

    readonly property real markWidth: iconShown ? icon.width + Theme.paddingSmall : 0
    readonly property real leadWidth: before.length > 0
                                      ? Math.min(lead.implicitWidth,
                                                 Math.max(width / 2, width - markWidth - name.implicitWidth))
                                      : 0
    readonly property real nameWidth: Math.min(name.implicitWidth, Math.max(0, width - leadWidth - markWidth))
    readonly property real used: leadWidth + markWidth + nameWidth
    readonly property real start: horizontalAlignment === Text.AlignRight ? width - used
                                  : horizontalAlignment === Text.AlignHCenter ? (width - used) / 2 : 0

    implicitWidth: lead.implicitWidth + markWidth + name.implicitWidth
    implicitHeight: split ? lead.implicitHeight + name.implicitHeight : name.implicitHeight
    height: implicitHeight

    TextMetrics {
        id: nameMetrics

        font: name.font
        text: line.text
    }

    Label {
        id: lead

        visible: line.before.length > 0
        x: line.split ? 0 : line.start
        y: line.split ? 0 : (line.height - height) / 2
        width: line.split ? line.width : line.leadWidth
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
        x: line.split ? 0 : line.start + line.leadWidth
        y: name.y + (name.height - height) / 2
        source: FileIcons.source("server", Theme.pixelRatio)
        width: Math.round(line.fontSize * 1.25)
        height: width
        sourceSize.width: width
        sourceSize.height: height
        color: line.color
    }

    Label {
        id: name

        x: line.split ? line.markWidth : line.start + line.leadWidth + line.markWidth
        y: line.split ? lead.height : (line.height - height) / 2
        width: line.split ? line.width - line.markWidth : line.wraps ? line.width : line.nameWidth
        text: line.text
        textFormat: Text.PlainText
        color: line.color
        font.pixelSize: line.fontSize
        wrapMode: line.wraps ? Text.Wrap : Text.NoWrap
        maximumLineCount: line.wraps ? line.maximumLineCount : 1
        truncationMode: line.wraps ? TruncationMode.Elide : TruncationMode.Fade
    }
}
