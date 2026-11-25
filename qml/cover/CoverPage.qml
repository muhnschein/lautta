// SPDX-License-Identifier: LGPL-2.1-or-later
import QtQuick 2.6
import Sailfish.Silica 1.0
import Lautta 1.0

CoverBackground {
    id: cover

    property bool live: status === Cover.Active
    readonly property var source: {
        if (typeof pageStack === "undefined" || !pageStack || !pageStack.currentPage)
            return null
        return pageStack.find(function (page) { return page.coverKind !== undefined })
    }
    readonly property alias tasks: transferCover
    readonly property bool transferring: transferCover.active
    readonly property string kind: transferring ? "tasks" : (source ? source.coverKind : "")

    function told(name, fallback) {
        return source && source[name] !== undefined ? source[name] : fallback
    }

    TransferCover {
        id: transferCover

        anchors.fill: parent
        visible: active
    }

    CoverHeader {
        visible: cover.kind === ""
        //% "Lautta"
        text: qsTrId("lautta-app-name")
    }

    Loader {
        objectName: "coverContent"
        anchors.fill: parent
        active: cover.live && cover.source !== null
        sourceComponent: cover.kind === "home" ? home : (cover.kind === "grid" ? grid : (cover.kind === "file" ? file : null))
    }

    Component {
        id: home

        CoverHome {
            rows: cover.told("listModel", null)
            deviceFree: cover.told("deviceFree", -1)
            deviceTotal: cover.told("deviceTotal", -1)
        }
    }

    Component {
        id: file

        CoverFile {
            uri: cover.told("coverUri", "")
        }
    }

    Component {
        id: grid

        Item {
            CoverHeader {
                id: header

                text: cover.told("coverTitle", "")
            }
            CoverGrid {
                id: tiles

                objectName: "coverGrid"
                anchors {
                    top: header.bottom
                    topMargin: Theme.paddingLarge
                    left: parent.left
                    right: parent.right
                    bottom: parent.bottom
                }
                model: cover.told("coverModel", null)
            }
            Label {
                objectName: "coverEmpty"
                visible: tiles.count === 0 && tiles.model !== null && tiles.model.loading !== true
                anchors.centerIn: parent
                width: parent.width - 2 * Theme.paddingMedium
                horizontalAlignment: Text.AlignHCenter
                wrapMode: Text.Wrap
                //% "No files"
                text: qsTrId("lautta-dir-empty")
                font.pixelSize: Theme.fontSizeLarge
                color: Theme.highlightColor
                opacity: Theme.opacityHigh
            }
        }
    }
}
