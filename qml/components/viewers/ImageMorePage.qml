// SPDX-License-Identifier: LGPL-2.1-or-later
import QtQuick 2.6
import Sailfish.Silica 1.0
import Lautta 1.0
import ".."
import "../directory"
import "../directory/ContextActions.js" as Actions

Page {
    id: page

    property string uri
    property bool changeable: true
    property bool writable: true

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
            pageStack.replace(Qt.resolvedUrl("../../dialogs/CompressDialog.qml"), { "uris": [page.uri] })
            break
        case "checksums":
            pageStack.replace(Qt.resolvedUrl("../../pages/ChecksumsPage.qml"), { "uri": page.uri })
            break
        }
    }

    SilicaListView {
        anchors.fill: parent
        model: (page.changeable ? [
            //% "Rename"
            { "id": "rename", "text": qsTrId("lautta-viewers-more-rename"), "icon": Actions.icon("rename") }
        ] : []).concat(Actions.more({ "isDir": false, "writable": page.writable, "favourite": App.isFavourite(page.uri) }))

        header: HeaderBand {
            PageHeader {
                title: App.nameOf(page.uri)
            }
        }

        delegate: MoreActionRow {
            text: modelData.text
            icon: modelData.icon
            onClicked: page.choose(modelData.id)
        }

        VerticalScrollDecorator { }
    }
}
