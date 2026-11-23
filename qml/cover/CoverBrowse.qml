// SPDX-License-Identifier: LGPL-2.1-or-later
// SCR-58 on Browse: free space of phone and first volume, below them newest Recents as tiles;
// none: Browse's places.
import QtQuick 2.6
import QtQml 2.2
import Sailfish.Silica 1.0
import "../components/browse/BrowseText.js" as BrowseText

Item {
    id: root

    // SCR-01 Browse rows.
    property var rows
    property real deviceFree: -1
    property real deviceTotal: -1
    readonly property var seen: {
        var out = []
        for (var i = 0; i < watch.count; ++i) {
            if (watch.objectAt(i))
                out.push(watch.objectAt(i))
        }
        return out
    }
    readonly property var recents: {
        var strip = seen.filter(function (r) { return r.kind === "recent-strip" && r.tiles.length > 0 })
        return strip.length > 0 ? JSON.parse(strip[0].tiles) : []
    }
    readonly property var places: seen.filter(function (r) { return r.icon.length > 0 }).map(function (r) {
        return { "placeIcon": r.icon }
    })
    // Only volumes tell a size; unmounted ones no free space.
    readonly property var storage: {
        var lines = []
        if (deviceTotal > 0) {
            //% "Phone"
            lines.push({ "name": qsTrId("lautta-cover-phone"), "free": deviceFree, "total": deviceTotal })
        }
        var volumes = seen.filter(function (r) { return r.total > 0 && r.free >= 0 })
        if (volumes.length > 0)
            lines.push({ "name": volumes[0].name, "free": volumes[0].free, "total": volumes[0].total })
        return lines
    }

    Instantiator {
        id: watch

        model: root.rows
        delegate: QtObject {
            readonly property string section: model.section
            readonly property string kind: model.kind
            readonly property string name: model.name
            readonly property string icon: model.icon
            readonly property string tiles: model.tiles === undefined ? "" : model.tiles
            readonly property real free: model.free
            readonly property real total: model.total
        }
    }

    CoverHeader {
        id: header

        //% "Lautta"
        text: qsTrId("lautta-app-name")
    }

    Column {
        id: lines

        anchors {
            top: header.bottom
            topMargin: Theme.paddingSmall
            left: parent.left
            leftMargin: Theme.paddingMedium
            right: parent.right
            rightMargin: Theme.paddingMedium
        }
        spacing: Theme.paddingSmall

        Repeater {
            model: root.storage

            Item {
                objectName: "storageLine"
                width: lines.width
                height: name.height + Theme.paddingSmall / 2 + bar.height

                Label {
                    id: free

                    objectName: "storageFree"
                    anchors.right: parent.right
                    readonly property string amount: BrowseText.space(modelData.free)

                    //% "%1 free"
                    text: qsTrId("lautta-cover-free").arg(amount)
                    font.pixelSize: Theme.fontSizeTiny
                    color: Theme.secondaryColor
                }
                Label {
                    id: name

                    objectName: "storageName"
                    anchors {
                        left: parent.left
                        right: free.left
                        rightMargin: Theme.paddingSmall
                    }
                    truncationMode: TruncationMode.Fade
                    textFormat: Text.PlainText
                    text: modelData.name
                    font.pixelSize: Theme.fontSizeTiny
                }
                Rectangle {
                    id: bar

                    anchors.bottom: parent.bottom
                    width: parent.width
                    height: Math.max(2, Math.round(Theme.paddingSmall / 3))
                    color: Theme.rgba(Theme.primaryColor, Theme.opacityFaint)

                    Rectangle {
                        objectName: "storageUsed"
                        width: Math.round(parent.width * (modelData.total - modelData.free) / modelData.total)
                        height: parent.height
                        color: Theme.highlightColor
                    }
                }
            }
        }
    }

    CoverGrid {
        objectName: "coverGrid"
        anchors {
            top: lines.bottom
            topMargin: Theme.paddingMedium
            left: parent.left
            right: parent.right
            bottom: parent.bottom
        }
        model: root.recents.length > 0 ? root.recents : root.places
    }
}
