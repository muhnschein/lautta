// SPDX-License-Identifier: LGPL-2.1-or-later
import QtQuick 2.6
import Sailfish.Silica 1.0
import Lautta 1.0
import "../components/browse"

Page {
    id: page

    property string uri
    property bool isDir: true
    property string colour
    property string icon
    property string defaultIcon
    property bool added
    property bool filled

    allowedOrientations: Orientation.All

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

    // Save once typing pauses (each save refreshes Browse rows), and on page leave: at start of pop
    // or when no transition.
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
