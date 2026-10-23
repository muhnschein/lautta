// SPDX-License-Identifier: LGPL-2.1-or-later
// Add or edit a favourite (ORG-1): a label and a colour for a folder or a
// file. Opened on `uri`; it edits the item's favourite when it is one
// already and adds one otherwise, so callers never pass an id.
import QtQuick 2.6
import Sailfish.Silica 1.0
import Lautta 1.0
import "../components"
import "../components/browse"

Dialog {
    id: dialog

    property string uri
    // A folder unless the caller says the item is a file; a favourite that
    // exists already says what it is.
    property bool isDir: true
    property string colour
    // Known once the favourites are loaded (`filled`).
    property bool editing
    property bool filled

    allowedOrientations: Orientation.All
    canAccept: filled && labelField.text.trim().length > 0

    function fill() {
        if (filled || !favourites.loaded)
            return
        var f = JSON.parse(favourites.get(favourites.idOf(uri)))
        editing = f.uri !== undefined
        if (editing) {
            labelField.text = f.label
            colour = f.colour
            isDir = f.isDir !== false
        }
        filled = true
    }

    onAccepted: favourites.save(uri, labelField.text, colour, isDir)

    FavouritesModel {
        id: favourites
        onLoadedChanged: dialog.fill()
        Component.onCompleted: reload()
    }

    Component.onCompleted: labelField.text = App.nameOf(uri)

    SilicaFlickable {
        anchors.fill: parent
        contentHeight: column.height + Theme.paddingLarge

        PullDownMenu {
            visible: dialog.editing
            MenuItem {
                //% "Remove from favourites"
                text: qsTrId("lautta-favourite-remove")
                onClicked: {
                    favourites.removeUri(dialog.uri)
                    pageStack.pop()
                }
            }
        }

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

            // The folder itself, or the folder a file is in.
            DetailItem {
                //% "Folder"
                label: qsTrId("lautta-favourite-folder")
                value: favourites.placeOf(dialog.isDir ? dialog.uri : App.parentUri(dialog.uri))
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
                    readonly property bool current: model.uri === dialog.uri

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
                        PlaceLabel {
                            width: parent.width
                            text: model.place
                            remote: App.isRemote(model.uri)
                        }
                    }
                }
            }
        }

        VerticalScrollDecorator { }
    }
}
