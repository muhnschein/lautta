// SPDX-License-Identifier: LGPL-2.1-or-later
import QtQuick 2.6
import Sailfish.Silica 1.0
import Lautta 1.0
import "../components"
import "CoverRows.js" as CoverRows

Item {
    id: lines

    property alias model: view.model
    readonly property real iconSize: Theme.iconSizeSmall
    readonly property real edge: 2 * Theme.paddingLarge
    readonly property alias count: view.count

    clip: true

    Item {
        id: faded

        width: parent.width
        height: parent.height

        ListView {
            id: view

            width: parent.width
            height: parent.height
            interactive: false

            delegate: Item {
                id: line

                readonly property var row: CoverRows.told(typeof modelData !== "undefined" ? modelData : model, App)
                readonly property bool isDir: CoverRows.field(row, "isDir", false)
                readonly property alias name: label.text

                objectName: "coverLine"
                width: view.width
                height: lines.iconSize + Theme.paddingMedium

                FileIcon {
                    id: icon

                    objectName: "coverLineIcon"
                    x: Theme.paddingLarge
                    anchors.verticalCenter: parent.verticalCenter
                    size: lines.iconSize
                    glyphSize: lines.iconSize
                    isDir: line.isDir
                    category: CoverRows.field(line.row, "category", line.isDir ? "folder" : App.categoryOf(label.text))
                    thumbnailSource: CoverRows.field(line.row, "thumbnailSource", "")
                    mimeType: CoverRows.field(line.row, "mimeType", "")
                }

                Label {
                    id: label

                    anchors {
                        left: icon.right
                        leftMargin: Theme.paddingMedium
                        right: parent.right
                        verticalCenter: parent.verticalCenter
                    }
                    text: CoverRows.field(line.row, "name", "")
                    textFormat: Text.PlainText
                    font.pixelSize: Theme.fontSizeExtraSmall
                }
            }
        }

        OpacityRampEffect {
            objectName: "coverEdgeFade"
            sourceItem: view
            direction: OpacityRamp.LeftToRight
            offset: 1 - lines.edge / faded.width
            slope: faded.width / lines.edge
        }
    }

    OpacityRampEffect {
        sourceItem: faded
        direction: OpacityRamp.TopToBottom
        offset: 0.75
        slope: 4
    }
}
