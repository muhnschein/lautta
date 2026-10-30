// SPDX-License-Identifier: LGPL-2.1-or-later
// A line that names a location (§15.5): a remote one, on a server (LOC-7),
// has a small server icon right before its name, the icon of Browse's
// Servers rows in the colour of the line; a local one has none. The name
// may follow other text, as in "Copied 4 items to NAS": that text is
// `before`, the name and what follows it is `text`. Too long a line fades
// out at its end, as a Label does.
import QtQuick 2.6
import Sailfish.Silica 1.0

Item {
    id: line

    // Shown before the name, without the icon.
    property string before
    // The location's name and what follows it.
    property string text
    property bool remote
    property color color: Theme.secondaryColor
    property real fontSize: Theme.fontSizeExtraSmall
    // Text.AlignLeft or Text.AlignRight (a section header).
    property int horizontalAlignment: Text.AlignLeft
    readonly property bool iconShown: remote && text.length > 0

    // What each part takes, left to right. Too long a line keeps at least
    // half of it for the text before the name, so that both show.
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
        source: Qt.resolvedUrl("../icons/server.svg")
        // As tall as the text's capitals and descenders together.
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
