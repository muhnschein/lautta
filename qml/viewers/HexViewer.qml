// SPDX-License-Identifier: LGPL-2.1-or-later
// Hex viewer (PRV-4, board HexViewer): 16 bytes per row, shown as two lines
// of 8 so it fits a phone; rows are read with ranged reads while scrolling,
// so a huge remote file is never downloaded.
import QtQuick 2.6
import Sailfish.Silica 1.0
import Lautta 1.0
import "../components/viewers"
import "../components/viewers/ViewerText.js" as ViewerText
import "../components/ErrorText.js" as ErrorText

Page {
    id: page

    property string uri

    readonly property string fileName: App.nameOf(uri)

    allowedOrientations: Orientation.All

    function gotoOffset(text) {
        var row = hexModel.rowOfOffset(text)
        if (row < 0)
            return false
        view.positionViewAtIndex(row, ListView.Beginning)
        return true
    }

    HexModel {
        id: hexModel
        uri: page.uri
    }

    ExternalActions {
        id: external
        uri: page.uri
    }

    SilicaListView {
        id: view

        anchors.fill: parent
        model: hexModel
        clip: true
        cacheBuffer: Theme.itemSizeSmall * 20

        PullDownMenu {
            MenuItem {
                //% "Go to offset"
                text: qsTrId("lautta-viewers-hex-goto")
                onClicked: {
                    var dialog = pageStack.push(Qt.resolvedUrl("../components/viewers/GotoOffsetDialog.qml"),
                                                { "size": hexModel.size })
                    dialog.accepted.connect(function () { page.gotoOffset(dialog.offsetText) })
                }
            }
            MenuItem {
                //% "Open with"
                text: qsTrId("lautta-viewers-open-with")
                onClicked: external.openWith()
            }
        }

        header: Column {
            width: view.width

            PageHeader {
                title: page.fileName
                //% "Hex · %1"
                description: qsTrId("lautta-viewers-hex-size").arg(Format.formatFileSize(hexModel.size))
            }
            Label {
                x: Theme.horizontalPageMargin
                width: parent.width - 2 * Theme.horizontalPageMargin
                horizontalAlignment: Text.AlignRight
                color: Theme.secondaryColor
                font.pixelSize: Theme.fontSizeExtraSmall
                //% "Loaded as you scroll"
                text: qsTrId("lautta-viewers-hex-lazy")
            }
        }

        delegate: Item {
            width: view.width
            height: lines.height + Theme.paddingSmall

            Column {
                id: lines
                x: Theme.horizontalPageMargin
                opacity: loaded ? 1 : Theme.opacityLow

                Label {
                    font.family: "monospace"
                    font.pixelSize: Theme.fontSizeTiny
                    color: Theme.highlightColor
                    text: loaded ? ViewerText.hexOffset(offset, 8) + "  " + hex.substring(0, 23) + "  " + ascii.substring(0, 8)
                                 : "…"
                }
                Label {
                    font.family: "monospace"
                    font.pixelSize: Theme.fontSizeTiny
                    color: Theme.highlightColor
                    visible: loaded && hex.length > 25
                    text: loaded ? ViewerText.hexOffset(offset + 8, 8) + "  " + hex.substring(25) + "  " + ascii.substring(8) : ""
                }
            }
        }

        ViewPlaceholder {
            enabled: hexModel.errorKind !== ""
            text: ErrorText.message(hexModel.errorKind, { "item": page.fileName, "location": App.locationName(page.uri) })
        }

        VerticalScrollDecorator { }
    }

    BusyIndicator {
        anchors.centerIn: parent
        size: BusyIndicatorSize.Large
        running: hexModel.loading
    }
}
