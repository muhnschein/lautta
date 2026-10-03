// SPDX-License-Identifier: LGPL-2.1-or-later
// Add or edit a favourite (ORG-1): a label and a colour for a folder.
// `favouriteId` is -1 for a new favourite of `uri`.
import QtQuick 2.6
import Sailfish.Silica 1.0
import Lautta 1.0
import "../components/browse"

Dialog {
    id: dialog

    property string uri
    property int favouriteId: -1
    property string colour
    readonly property bool editing: favouriteId >= 0
    property bool filled

    allowedOrientations: Orientation.All
    canAccept: labelField.text.trim().length > 0

    function fill() {
        if (filled || !favourites.loaded)
            return
        filled = true
        if (!editing)
            return
        var f = JSON.parse(favourites.get(favouriteId))
        if (f.uri === undefined)
            return
        uri = f.uri
        labelField.text = f.label
        colour = f.colour
    }

    onAccepted: {
        if (editing)
            favourites.edit(favouriteId, labelField.text, colour)
        else
            favourites.add(uri, labelField.text, colour)
    }

    FavouritesModel {
        id: favourites
        onLoadedChanged: dialog.fill()
        Component.onCompleted: reload()
    }

    Component.onCompleted: {
        if (!editing)
            labelField.text = App.nameOf(uri)
    }

    SilicaFlickable {
        anchors.fill: parent
        contentHeight: column.height + Theme.paddingLarge

        Column {
            id: column
            width: parent.width

            DialogHeader {
                acceptText: dialog.editing
                            //% "Save"
                            ? qsTrId("lautta-favourite-save")
                            //% "Add"
                            : qsTrId("lautta-favourite-add")
                title: dialog.editing
                       //% "Edit favourite"
                       ? qsTrId("lautta-favourite-edit-title")
                       //% "Add to favourites"
                       : qsTrId("lautta-favourite-add-title")
            }

            TextField {
                id: labelField
                width: parent.width
                //% "Label"
                label: qsTrId("lautta-favourite-label")
                placeholderText: label
                focus: true
                EnterKey.iconSource: "image://theme/icon-m-enter-accept"
                EnterKey.onClicked: dialog.accept()
            }

            DetailItem {
                //% "Folder"
                label: qsTrId("lautta-favourite-folder")
                value: favourites.placeOf(dialog.uri)
            }

            SectionHeader {
                //% "Colour"
                text: qsTrId("lautta-favourite-colour")
            }
            ColourPicker {
                colour: dialog.colour
                onPicked: dialog.colour = colour
            }

            SectionHeader {
                visible: dialog.editing
                //% "Order"
                text: qsTrId("lautta-favourite-order")
            }
            Label {
                visible: dialog.editing
                x: Theme.horizontalPageMargin
                width: parent.width - 2 * x
                wrapMode: Text.Wrap
                horizontalAlignment: Text.AlignRight
                //% "Long-press a favourite in Browse to move it up or down."
                text: qsTrId("lautta-favourite-order-hint")
                font.pixelSize: Theme.fontSizeExtraSmall
                color: Theme.secondaryColor
            }
            Repeater {
                model: dialog.editing ? favourites : null
                delegate: Item {
                    width: column.width
                    height: Theme.itemSizeSmall
                    readonly property bool current: model.itemId === dialog.favouriteId

                    Rectangle {
                        anchors.fill: parent
                        visible: parent.current
                        color: Theme.highlightBackgroundColor
                        opacity: Theme.highlightBackgroundOpacity
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
                            text: model.label
                            color: parent.parent.current ? Theme.highlightColor : Theme.primaryColor
                            truncationMode: TruncationMode.Fade
                        }
                        Label {
                            width: parent.width
                            text: model.place
                            font.pixelSize: Theme.fontSizeExtraSmall
                            color: Theme.secondaryColor
                            truncationMode: TruncationMode.Fade
                        }
                    }
                }
            }
        }

        VerticalScrollDecorator { }
    }
}
