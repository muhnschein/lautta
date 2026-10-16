// SPDX-License-Identifier: LGPL-2.1-or-later
// "More" of the image viewer (board ImageViewer): the file actions of the
// folder view for the image being shown, handled by the other areas'
// dialogs and pages.
import QtQuick 2.6
import Sailfish.Silica 1.0
import Lautta 1.0

Page {
    id: page

    property string uri

    allowedOrientations: Orientation.All

    function choose(action) {
        if (action === "rename") {
            pageStack.replace(Qt.resolvedUrl("../../dialogs/RenameDialog.qml"), { "uri": page.uri })
        } else {
            pageStack.replace(Qt.resolvedUrl("../../pages/InfoPage.qml"), { "uri": page.uri })
        }
    }

    SilicaListView {
        anchors.fill: parent
        model: [
            //% "Rename"
            { "id": "rename", "text": qsTrId("lautta-viewers-more-rename") },
            //% "Info"
            { "id": "info", "text": qsTrId("lautta-viewers-more-info") }
        ]

        header: PageHeader {
            title: App.nameOf(page.uri)
            //% "More actions"
            description: qsTrId("lautta-viewers-more")
        }

        delegate: BackgroundItem {
            onClicked: page.choose(modelData.id)

            Label {
                x: Theme.horizontalPageMargin
                anchors.verticalCenter: parent.verticalCenter
                width: parent.width - 2 * Theme.horizontalPageMargin
                text: modelData.text
                color: parent.highlighted ? Theme.highlightColor : Theme.primaryColor
            }
        }

        VerticalScrollDecorator { }
    }
}
