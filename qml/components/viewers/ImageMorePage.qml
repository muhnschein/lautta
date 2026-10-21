// SPDX-License-Identifier: LGPL-2.1-or-later
// "More" of the image viewer (board ImageViewer): the file actions of the
// folder view for the image being shown, Rename and the folder view's More
// actions (ContextActions.js), handled by the other areas' dialogs.
import QtQuick 2.6
import Sailfish.Silica 1.0
import Lautta 1.0
import "../directory/ContextActions.js" as Actions

Page {
    id: page

    property string uri

    allowedOrientations: Orientation.All

    function choose(action) {
        switch (action) {
        case "rename":
            pageStack.replace(Qt.resolvedUrl("../../dialogs/RenameDialog.qml"), { "uri": page.uri })
            break
        case "favourite":
            pageStack.replace(Qt.resolvedUrl("../../dialogs/FavouriteDialog.qml"), { "uri": page.uri, "isDir": false })
            break
        case "compress":
            pageStack.replace(Qt.resolvedUrl("../../dialogs/CompressDialog.qml"), { "uris": JSON.stringify([page.uri]) })
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
