// SPDX-License-Identifier: LGPL-2.1-or-later
import QtQuick 2.6
import Sailfish.Silica 1.0

Rectangle {
    id: sheet

    property int pageIndex
    property string uri
    property real fitWidth
    property real zoom: 1
    property var hits: []

    property var links: []

    signal doubleTapped(real tapX, real tapY)
    signal linkClicked(var link)

    function source() {
        return "image://lautta-pdf/" + uri + "?page=" + pageIndex
    }

    color: "white"

    Image {
        id: base
        anchors.fill: parent
        asynchronous: true
        cache: false
        smooth: true
        sourceSize.width: Math.round(sheet.fitWidth)
        source: sheet.uri !== "" ? sheet.source() : ""
    }

    Image {
        anchors.fill: parent
        asynchronous: true
        cache: false
        smooth: true
        visible: status === Image.Ready
        // About 40 MB for an A4 page at most: phone keeps two or three.
        sourceSize.width: Math.min(2560, Math.round(sheet.fitWidth * sheet.zoom))
        source: sheet.zoom > 1 && sheet.uri !== "" ? sheet.source() : ""
    }

    BusyIndicator {
        anchors.centerIn: parent
        size: BusyIndicatorSize.Medium
        running: base.status === Image.Loading
    }

    Label {
        anchors.centerIn: parent
        width: parent.width - 2 * Theme.horizontalPageMargin
        visible: base.status === Image.Error
        horizontalAlignment: Text.AlignHCenter
        wrapMode: Text.Wrap
        color: "black"
        //% "Can't show page %1"
        text: qsTrId("lautta-viewers-pdf-page-error").arg(sheet.pageIndex + 1)
    }

    Repeater {
        model: sheet.hits

        Rectangle {
            x: modelData.x * sheet.width
            y: modelData.y * sheet.height
            width: modelData.width * sheet.width
            height: modelData.height * sheet.height
            color: Theme.rgba(Theme.highlightBackgroundColor, modelData.current ? 0.55 : 0.25)
            border.width: modelData.current ? Math.max(1, Math.round(Theme.paddingSmall / 3)) : 0
            border.color: Theme.highlightBackgroundColor
        }
    }

    MouseArea {
        anchors.fill: parent
        onDoubleClicked: sheet.doubleTapped(mouse.x, mouse.y)
    }

    Repeater {
        model: sheet.links

        MouseArea {
            x: modelData.x * sheet.width
            y: modelData.y * sheet.height
            width: modelData.width * sheet.width
            height: modelData.height * sheet.height
            onClicked: sheet.linkClicked(modelData)

            Rectangle {
                anchors.fill: parent
                visible: parent.pressed
                color: Theme.rgba(Theme.highlightBackgroundColor, Theme.highlightBackgroundOpacity)
            }
        }
    }
}
