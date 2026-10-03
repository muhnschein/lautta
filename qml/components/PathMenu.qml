// SPDX-License-Identifier: LGPL-2.1-or-later
// The path menu below the folder header (SPEC BRW-9, board PathMenu): the
// ancestors from the location down, an icon row (Copy address, Edit path,
// Favourite, Info) and the path editor with completion from cached listings.
import QtQuick 2.6
import Sailfish.Silica 1.0
import Lautta 1.0

Item {
    id: menu

    // The PathModel of the folder.
    property var pathModel
    property string currentUri
    property bool open
    property bool editing

    signal navigate(string uri)
    signal infoRequested
    signal favouriteRequested

    function close() {
        open = false
        editing = false
    }

    function _resolveAndGo() {
        var target = pathModel ? pathModel.resolve(pathField.text) : ""
        if (target.length > 0) {
            close()
            navigate(target)
        } else {
            pathField.errorHighlight = true
        }
    }

    width: parent ? parent.width : 0
    height: open ? content.height : 0
    visible: height > 0
    clip: true

    Behavior on height { NumberAnimation { duration: 150; easing.type: Easing.InOutQuad } }

    Rectangle {
        anchors.fill: parent
        gradient: Gradient {
            GradientStop { position: 0; color: Theme.rgba(Theme.highlightBackgroundColor, Theme.highlightBackgroundOpacity / 2) }
            GradientStop { position: 1; color: Theme.rgba(Theme.highlightBackgroundColor, Theme.highlightBackgroundOpacity) }
        }
    }

    Column {
        id: content

        width: parent.width

        Repeater {
            model: menu.pathModel

            BackgroundItem {
                readonly property bool current: model.uri === menu.currentUri

                width: content.width
                height: Theme.itemSizeExtraSmall
                onClicked: {
                    menu.close()
                    if (!current)
                        menu.navigate(model.uri)
                }

                Label {
                    anchors {
                        left: parent.left
                        right: parent.right
                        margins: Theme.horizontalPageMargin
                        verticalCenter: parent.verticalCenter
                    }
                    horizontalAlignment: Text.AlignHCenter
                    text: model.name
                    truncationMode: TruncationMode.Fade
                    color: parent.current || parent.highlighted ? Theme.highlightColor : Theme.primaryColor
                }
            }
        }

        IconRow {
            menu: null
            actions: [
                //% "Copy address"
                { "id": "copy", "icon": "image://theme/icon-m-link", "text": qsTrId("lautta-dir-path-copy-address") },
                //% "Edit path"
                { "id": "edit", "icon": "image://theme/icon-m-edit", "text": qsTrId("lautta-dir-path-edit") },
                //% "Favourite"
                { "id": "favourite", "icon": "image://theme/icon-m-favorite", "text": qsTrId("lautta-dir-path-favourite") },
                //% "Info"
                { "id": "info", "icon": "image://theme/icon-m-about", "text": qsTrId("lautta-dir-path-info") }
            ]
            onTriggered: {
                switch (actionId) {
                case "copy":
                    Clipboard.text = App.displayAddress(menu.currentUri)
                    menu.close()
                    break
                case "edit":
                    pathField.text = menu.pathModel ? menu.pathModel.address : ""
                    menu.editing = true
                    pathField.forceActiveFocus()
                    break
                case "favourite":
                    menu.close()
                    menu.favouriteRequested()
                    break
                case "info":
                    menu.close()
                    menu.infoRequested()
                    break
                }
            }
        }

        TextField {
            id: pathField

            property var completions: []

            visible: menu.editing
            width: parent.width
            //% "Path"
            label: qsTrId("lautta-dir-path-label")
            placeholderText: label
            inputMethodHints: Qt.ImhNoAutoUppercase | Qt.ImhNoPredictiveText | Qt.ImhUrlCharactersOnly
            EnterKey.iconSource: "image://theme/icon-m-enter-accept"
            EnterKey.onClicked: menu._resolveAndGo()
            onTextChanged: {
                errorHighlight = false
                completions = menu.editing && menu.pathModel ? menu.pathModel.complete(text) : []
            }
        }

        Repeater {
            model: menu.editing ? pathField.completions : []

            BackgroundItem {
                width: content.width
                height: Theme.itemSizeSmall
                onClicked: {
                    menu.close()
                    menu.navigate(modelData)
                }

                Column {
                    anchors {
                        left: parent.left
                        right: parent.right
                        margins: Theme.horizontalPageMargin
                        verticalCenter: parent.verticalCenter
                    }

                    Label {
                        width: parent.width
                        text: App.nameOf(modelData)
                        truncationMode: TruncationMode.Fade
                        color: parent.parent.highlighted ? Theme.highlightColor : Theme.primaryColor
                    }
                    Label {
                        width: parent.width
                        //% "Folder · from cache"
                        text: qsTrId("lautta-dir-path-from-cache")
                        font.pixelSize: Theme.fontSizeExtraSmall
                        color: Theme.secondaryColor
                    }
                }
            }
        }
    }
}
