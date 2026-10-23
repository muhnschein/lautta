// SPDX-License-Identifier: LGPL-2.1-or-later
// A favourite (ORG-1): label, colour and icon of a folder or file, each
// saved as it changes, so there is nothing to accept. Opened on `uri`: an
// item that is no favourite yet becomes one at once ("Add to favourites"),
// with its name, no colour and the default icon, and the page then edits
// it. Removing stays in Browse's long-press menu.
import QtQuick 2.6
import Sailfish.Silica 1.0
import Lautta 1.0
import "../components/browse"

Page {
    id: page

    property string uri
    // A folder unless the caller says the item is a file; a favourite that
    // exists already says what it is.
    property bool isDir: true
    property string colour
    // A name of FavouritesModel.iconChoices(), "" for the default icon.
    property string icon
    property string defaultIcon
    // The page made the item a favourite: it was none.
    property bool added
    // The fields show the stored favourite.
    property bool filled

    allowedOrientations: Orientation.All

    // An emptied field gives the favourite the item's name back, as when
    // it was added: a favourite always has a label.
    function label() {
        var text = labelField.text.trim()
        return text.length > 0 ? text : App.nameOf(uri)
    }

    function save() {
        labelTimer.stop()
        favourites.save(uri, label(), colour, icon, isDir)
    }

    function fill() {
        if (filled || !favourites.loaded)
            return
        var id = favourites.idOf(uri)
        if (id === -1) {
            // Added once; the model reloads with it and calls back.
            if (!added) {
                added = true
                favourites.save(uri, App.nameOf(uri), "", "", isDir)
            }
            return
        }
        var f = JSON.parse(favourites.get(id))
        labelField.text = f.label
        colour = f.colour
        icon = f.icon
        isDir = f.isDir !== false
        defaultIcon = f.defaultIcon
        filled = true
    }

    // Typing is saved once it pauses, not for every key (each save
    // refreshes the Browse rows), and at the latest when the page is left:
    // as it starts to go, or when it goes without a transition.
    function flush() {
        if (labelTimer.running)
            save()
    }

    onStatusChanged: if (status === PageStatus.Deactivating) flush()
    Component.onDestruction: flush()

    Timer {
        id: labelTimer
        interval: 500
        onTriggered: page.save()
    }

    FavouritesModel {
        id: favourites
        onCountChanged: page.fill()
        onLoadedChanged: page.fill()
        Component.onCompleted: reload()
    }

    SilicaFlickable {
        anchors.fill: parent
        contentHeight: column.height + Theme.paddingLarge

        Column {
            id: column
            width: parent.width
            enabled: page.filled

            PageHeader {
                //% "Edit favourite"
                title: qsTrId("lautta-favourite-edit-title")
                //% "Added to favourites"
                description: page.added ? qsTrId("lautta-favourite-added") : ""
            }

            TextField {
                id: labelField
                width: parent.width
                //% "Label"
                label: qsTrId("lautta-favourite-label")
                placeholderText: App.nameOf(page.uri)
                onTextChanged: if (page.filled) labelTimer.restart()
                EnterKey.iconSource: "image://theme/icon-m-enter-close"
                EnterKey.onClicked: {
                    labelField.focus = false
                    page.save()
                }
            }

            // The folder itself, or the folder a file is in.
            DetailItem {
                //% "Folder"
                label: qsTrId("lautta-favourite-folder")
                value: favourites.placeOf(page.isDir ? page.uri : App.parentUri(page.uri))
            }

            SectionHeader {
                //% "Colour"
                text: qsTrId("lautta-favourite-colour")
            }
            ColourPicker {
                colour: page.colour
                onPicked: {
                    page.colour = colour
                    page.save()
                }
            }

            SectionHeader {
                //% "Icon"
                text: qsTrId("lautta-favourite-icon")
            }
            // A folder's default is the star, the first choice; a file's is
            // its file's icon, offered before the others.
            IconPicker {
                icons: page.isDir ? favourites.iconChoices() : [""].concat(favourites.iconChoices())
                icon: page.icon.length > 0 ? page.icon : (page.isDir ? "favorite" : "")
                defaultIcon: page.defaultIcon
                colour: page.colour
                onPicked: {
                    page.icon = icon
                    page.save()
                }
            }
        }

        VerticalScrollDecorator { }
    }
}
