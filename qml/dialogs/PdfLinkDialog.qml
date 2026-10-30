// SPDX-License-Identifier: LGPL-2.1-or-later
import QtQuick 2.6
import Sailfish.Silica 1.0
import "../components/viewers/PdfText.js" as PdfText

Dialog {
    id: dialog

    property string url

    allowedOrientations: Orientation.All
    canAccept: PdfText.isWebLink(url)

    onAccepted: Qt.openUrlExternally(url)

    SilicaFlickable {
        anchors.fill: parent
        contentHeight: column.height + Theme.paddingLarge

        Column {
            id: column
            width: parent.width

            DialogHeader {
                //% "Open"
                acceptText: qsTrId("lautta-viewers-pdf-open-link")
            }

            Column {
                x: Theme.horizontalPageMargin
                width: parent.width - 2 * x
                topPadding: Theme.paddingLarge * 2
                bottomPadding: Theme.paddingLarge * 2
                spacing: Theme.paddingMedium

                HighlightImage {
                    anchors.horizontalCenter: parent.horizontalCenter
                    source: Qt.resolvedUrl("../icons/link.svg")
                    sourceSize.width: Theme.iconSizeLarge
                    sourceSize.height: Theme.iconSizeLarge
                    color: Theme.highlightColor
                }

                Label {
                    width: parent.width
                    horizontalAlignment: Text.AlignHCenter
                    font.pixelSize: Theme.fontSizeExtraLarge
                    font.family: Theme.fontFamilyHeading
                    color: Theme.highlightColor
                    wrapMode: Text.Wrap
                    //% "Link to a website"
                    text: qsTrId("lautta-viewers-pdf-web-link")
                }

                Label {
                    width: parent.width
                    horizontalAlignment: Text.AlignHCenter
                    font.pixelSize: Theme.fontSizeLarge
                    wrapMode: Text.WrapAnywhere
                    text: dialog.url
                }
            }

            Button {
                anchors.horizontalCenter: parent.horizontalCenter
                //% "Copy link"
                text: qsTrId("lautta-viewers-pdf-copy-link")
                onClicked: {
                    Clipboard.text = dialog.url
                    dialog.reject()
                }
            }
        }
    }
}
