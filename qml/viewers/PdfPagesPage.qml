// SPDX-License-Identifier: LGPL-2.1-or-later
import QtQuick 2.6
import Sailfish.Silica 1.0
import "../components"

Page {
    id: page

    property string uri
    property string fileName
    property int pageCount
    property int currentPage

    signal chosen(int index)

    allowedOrientations: Orientation.All

    FontMetrics {
        id: numberMetrics

        font.pixelSize: Theme.fontSizeSmall
    }

    SilicaGridView {
        id: grid

        readonly property int columns: page.isLandscape ? 5 : 3
        readonly property real sheetWidth: Math.floor(cellWidth - 2 * Theme.paddingLarge)

        objectName: "pdfPagesGrid"
        anchors.fill: parent
        cellWidth: Math.floor(width / columns)
        cellHeight: Math.ceil(sheetWidth * 1.5) + numberMetrics.height + 2 * Theme.paddingMedium
        model: page.pageCount
        cacheBuffer: cellHeight

        header: HeaderBand {
            width: grid.width

            PageHeader {
                //% "Pages"
                title: qsTrId("lautta-viewers-pdf-pages-title")
                description: page.fileName
            }
        }

        delegate: BackgroundItem {
            id: tile

            readonly property bool current: index === page.currentPage

            objectName: "pdfPageTile"
            width: grid.cellWidth
            height: grid.cellHeight

            Rectangle {
                anchors {
                    fill: thumb
                    margins: -Theme.paddingSmall
                }
                visible: tile.current
                color: "transparent"
                border.color: Theme.highlightColor
                border.width: Math.max(2, Math.round(Theme.paddingSmall / 2))
            }

            Image {
                id: thumb

                objectName: "pdfPageThumb"
                anchors {
                    horizontalCenter: parent.horizontalCenter
                    top: parent.top
                    topMargin: Theme.paddingMedium
                }
                width: status === Image.Ready ? implicitWidth : grid.sheetWidth
                height: status === Image.Ready ? implicitHeight : Math.ceil(grid.sheetWidth * 1.5)
                asynchronous: true
                cache: false
                sourceSize.width: grid.sheetWidth
                source: "image://lautta-pdf-thumb/" + page.uri + "?page=" + index

                Rectangle {
                    anchors.fill: parent
                    z: -1
                    color: "white"
                    opacity: thumb.status === Image.Ready ? 1 : 0.1
                }

                BusyIndicator {
                    anchors.centerIn: parent
                    size: BusyIndicatorSize.Small
                    running: thumb.status === Image.Loading
                }

                HighlightImage {
                    anchors.centerIn: parent
                    visible: thumb.status === Image.Error
                    source: "image://theme/icon-m-document"
                }
            }

            Label {
                objectName: "pdfPageNumber"
                anchors {
                    horizontalCenter: parent.horizontalCenter
                    bottom: parent.bottom
                    bottomMargin: Theme.paddingMedium
                }
                font.pixelSize: Theme.fontSizeSmall
                color: tile.current || tile.highlighted ? Theme.highlightColor : Theme.secondaryColor
                text: index + 1
            }

            onClicked: page.chosen(index)
        }

        Component.onCompleted: positionViewAtIndex(page.currentPage, GridView.Center)

        VerticalScrollDecorator { }
    }
}
