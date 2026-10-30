// SPDX-License-Identifier: LGPL-2.1-or-later Rename goes back to viewer (renameChosen):
// RenameDialog only asks, viewer does the rename.
import QtQuick 2.6
import Sailfish.Silica 1.0
import Lautta 1.0
import "../directory/ContextActions.js" as Actions
import "../operations/Compress.js" as Compress

Page {
    id: page

    property string uri

    signal renameChosen()

    allowedOrientations: Orientation.All

    function choose(action) {
        switch (action) {
        case "rename":
            page.renameChosen()
            break
        case "favourite":
            pageStack.replace(Qt.resolvedUrl("../../pages/FavouritePage.qml"), { "uri": page.uri, "isDir": false })
            break
        case "compress":
            Compress.start(App, Operations, [page.uri], false)
            pageStack.pop()
            break
        }
    }

    SilicaListView {
        anchors.fill: parent
        model: [
            //% "Rename"
            { "id": "rename", "text": qsTrId("lautta-viewers-more-rename") }
        ].concat(Actions.more({ "isDir": false }))

        header: PageHeader {
            title: App.nameOf(page.uri)
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
