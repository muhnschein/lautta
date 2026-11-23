// SPDX-License-Identifier: LGPL-2.1-or-later
import QtQuick 2.6
import Sailfish.Silica 1.0

Item {
    id: band

    property string title
    property var pathModel
    property Item menuHost
    property int menuMost

    readonly property bool hasPath: pathModel ? pathModel.count > 0 : false
    readonly property real menuHeight: crumbs.menuHeight

    signal picked(string uri)
    signal home

    width: parent ? parent.width : 0
    height: Theme.itemSizeSmall

    Rectangle {
        anchors.fill: parent
        color: Theme.rgba(Theme.highlightBackgroundColor, Theme.highlightBackgroundOpacity / 2)
    }

    Label {
        id: current

        objectName: "bandTitle"
        anchors {
            right: parent.right
            rightMargin: Theme.horizontalPageMargin
            verticalCenter: parent.verticalCenter
        }
        width: Math.min(implicitWidth,
                        band.width - 2 * Theme.horizontalPageMargin - (band.hasPath ? crumbs.reserve : 0))
        text: band.title
        color: Theme.highlightColor
        truncationMode: TruncationMode.Fade
    }

    PathCrumbs {
        id: crumbs

        objectName: "bandCrumbs"
        visible: band.hasPath
        anchors {
            left: parent.left
            leftMargin: Theme.horizontalPageMargin
            right: current.left
        }
        height: parent.height
        pathModel: band.pathModel
        titleTop: current.y
        titlePixelSize: current.font.pixelSize
        titleFamily: current.font.family
        menuHost: band.menuHost
        menuMost: band.menuMost
        onPicked: band.picked(uri)
        onHome: band.home()
    }
}
