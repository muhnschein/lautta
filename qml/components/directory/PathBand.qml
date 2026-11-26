// SPDX-License-Identifier: LGPL-2.1-or-later
import QtQuick 2.6
import Sailfish.Silica 1.0

Item {
    id: band

    property alias title: crumbs.title
    property alias pathModel: crumbs.pathModel
    property alias menuHost: crumbs.menuHost
    property alias menuMost: crumbs.menuMost
    property alias homeText: crumbs.homeText

    readonly property real menuHeight: crumbs.menuHeight

    signal picked(string uri)
    signal home

    width: parent ? parent.width : 0
    height: Theme.itemSizeSmall

    Rectangle {
        objectName: "pathBandTint"
        anchors.fill: parent
        color: Theme.rgba(Theme.highlightBackgroundColor, Theme.highlightBackgroundOpacity / 2)
    }

    PathCrumbs {
        id: crumbs

        objectName: "bandCrumbs"
        anchors {
            fill: parent
            leftMargin: Theme.horizontalPageMargin
            rightMargin: Theme.horizontalPageMargin
        }
        pathShown: pathModel ? pathModel.count > 0 : false
        titleTop: band.height / 2 - capDrop
        onPicked: band.picked(uri)
        onHome: band.home()
    }
}
