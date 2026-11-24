// SPDX-License-Identifier: LGPL-2.1-or-later
// SCR-58 on Home: free space of phone and mounted volumes, below them newest Recents as tiles;
// none: Home's places.
import QtQuick 2.6
import QtQml 2.2
import Sailfish.Silica 1.0
import "../components"
import "../components/home/HomeText.js" as HomeText

Item {
    id: root

    // SCR-01 Home rows.
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
        var cells = []
        if (deviceTotal > 0)
            cells.push({ "icon": "image://theme/icon-m-device", "free": deviceFree })
        seen.forEach(function (r) {
            if (r.total > 0 && r.free >= 0)
                cells.push({ "icon": r.icon, "free": r.free })
        })
        return cells.slice(0, 4)
    }
    readonly property bool alone: storage.length === 1

    Instantiator {
        id: watch

        model: root.rows
        delegate: QtObject {
            readonly property string kind: model.kind
            readonly property string icon: model.icon
            readonly property string tiles: model.tiles === undefined ? "" : model.tiles
            readonly property real free: model.free
            readonly property real total: model.total
        }
    }

    // Phone alone: icon and free space on one line, centred, same gap below as above. More: two to a
    // row, free space under icon.
    Item {
        id: cells

        readonly property real gap: Theme.paddingMedium
        readonly property real cellWidth: root.alone ? width : (width - gap) / 2
        readonly property real cellHeight: root.alone ? Theme.iconSizeSmall : Theme.iconSizeSmallPlus + line.height
        readonly property int rows: Math.ceil(root.storage.length / 2)

        objectName: "storage"
        x: Theme.paddingLarge
        y: Theme.paddingLarge
        width: parent.width - 2 * x
        height: rows * cellHeight + Math.max(0, rows - 1) * Theme.paddingSmall

        FontMetrics {
            id: line

            font.pixelSize: Theme.fontSizeTiny
        }

        Repeater {
            model: root.storage

            Item {
                objectName: "storageCell"
                x: (index % 2) * (cells.cellWidth + cells.gap)
                y: Math.floor(index / 2) * (cells.cellHeight + Theme.paddingSmall)
                width: cells.cellWidth
                height: cells.cellHeight

                AppIcon {
                    id: glyph

                    objectName: "storageIcon"
                    x: root.alone ? (parent.width - width - Theme.paddingSmall - free.width) / 2 : (parent.width - width) / 2
                    iconSize: root.alone ? Theme.iconSizeSmall : Theme.iconSizeSmallPlus
                    icon: modelData.icon
                }
                Label {
                    id: free

                    readonly property string amount: HomeText.space(modelData.free)

                    objectName: "storageFree"
                    x: root.alone ? glyph.x + glyph.width + Theme.paddingSmall : (parent.width - width) / 2
                    y: root.alone ? (parent.height - height) / 2 : glyph.height
                    width: Math.min(implicitWidth, root.alone ? parent.width - glyph.width - Theme.paddingSmall : parent.width)
                    truncationMode: TruncationMode.Fade
                    //% "%1 free"
                    text: qsTrId("lautta-cover-free").arg(amount)
                    font.pixelSize: Theme.fontSizeTiny
                    color: Theme.secondaryColor
                }
            }
        }
    }

    CoverGrid {
        objectName: "coverGrid"
        anchors {
            top: parent.top
            topMargin: root.storage.length > 0 ? cells.y + cells.height + Theme.paddingLarge : 0
            left: parent.left
            right: parent.right
            bottom: parent.bottom
        }
        model: root.recents.length > 0 ? root.recents : root.places
    }
}
