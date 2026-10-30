// SPDX-License-Identifier: LGPL-2.1-or-later
import QtQuick 2.6
import Sailfish.Silica 1.0
import "../components/viewers/PdfText.js" as PdfText

Page {
    id: page

    property string fileName
    // [{title, level, page}] (PdfDocument.outline, parsed).
    property var outline: []
    property int pageCount
    property int currentPage

    signal chosen(int index)

    readonly property int currentItem: PdfText.currentItem(outline, currentPage)

    allowedOrientations: Orientation.All

    SilicaListView {
        id: list

        anchors { top: parent.top; left: parent.left; right: parent.right; bottom: goTo.top }
        clip: true
        model: page.outline

        header: PageHeader {
            //% "Contents"
            title: qsTrId("lautta-viewers-pdf-contents")
            description: page.fileName
        }

        delegate: BackgroundItem {
            id: item

            readonly property bool lit: highlighted || index === page.currentItem

            enabled: modelData.page >= 0

            Label {
                id: title
                anchors {
                    left: parent.left
                    leftMargin: Theme.horizontalPageMargin + Math.min(modelData.level, 4) * Theme.paddingLarge * 2
                    right: number.left
                    rightMargin: Theme.paddingMedium
                    verticalCenter: parent.verticalCenter
                }
                truncationMode: TruncationMode.Fade
                font.pixelSize: modelData.level > 0 ? Theme.fontSizeSmall : Theme.fontSizeMedium
                color: item.lit ? Theme.highlightColor : Theme.primaryColor
                text: modelData.title
            }

            Label {
                id: number
                anchors {
                    right: parent.right
                    rightMargin: Theme.horizontalPageMargin
                    verticalCenter: parent.verticalCenter
                }
                font.pixelSize: Theme.fontSizeSmall
                color: item.lit ? Theme.secondaryHighlightColor : Theme.secondaryColor
                text: modelData.page >= 0 ? modelData.page + 1 : ""
            }

            onClicked: page.chosen(modelData.page)
        }

        ViewPlaceholder {
            enabled: page.outline.length === 0
            //% "No table of contents"
            text: qsTrId("lautta-viewers-pdf-no-contents")
        }

        Component.onCompleted: {
            if (page.currentItem >= 0)
                positionViewAtIndex(page.currentItem, ListView.Center)
        }

        VerticalScrollDecorator { }
    }

    TextField {
        id: goTo

        readonly property int target: PdfText.typedPage(text, page.pageCount)

        anchors.bottom: parent.bottom
        width: parent.width
        inputMethodHints: Qt.ImhDigitsOnly
        //% "Go to page"
        placeholderText: qsTrId("lautta-viewers-pdf-go-to-page")
        //% "Go to page, 1 to %1"
        label: qsTrId("lautta-viewers-pdf-go-to-page-range").arg(page.pageCount)
        errorHighlight: text !== "" && target < 0

        EnterKey.enabled: target >= 0
        EnterKey.iconSource: "image://theme/icon-m-enter-accept"
        EnterKey.onClicked: page.chosen(target)
    }
}
