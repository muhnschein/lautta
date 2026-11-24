// SPDX-License-Identifier: LGPL-2.1-or-later
import QtQuick 2.6
import Sailfish.Silica 1.0
import "../components"
import "../components/search"
import "../components/FileIcons.js" as FileIcons

Page {
    id: page

    function open(file) {
        pageStack.push(Qt.resolvedUrl(file))
    }

    objectName: "settingsPage"
    allowedOrientations: Orientation.All

    SilicaFlickable {
        anchors.fill: parent
        contentHeight: column.height + Theme.paddingLarge

        Column {
            id: column

            width: parent.width

            HeaderBand {
                PageHeader {
                    //% "Settings"
                    title: qsTrId("lautta-settings-title")
                }
            }

            SectionHeader {
                //% "Browsing"
                text: qsTrId("lautta-settings-browsing")
            }
            SettingsEntry {
                objectName: "homePageEntry"
                iconSource: FileIcons.source("home", Theme.pixelRatio)
                //% "Home page"
                text: qsTrId("lautta-settings-home-page")
                onClicked: page.open("HomeSettingsPage.qml")
            }
            SettingsEntry {
                objectName: "viewOptionsRow"
                iconSource: FileIcons.source("view-options", Theme.pixelRatio)
                //% "View options"
                text: qsTrId("lautta-settings-view-options")
                onClicked: page.open("ViewOptionsPage.qml")
            }
            SettingsEntry {
                objectName: "searchEntry"
                iconSource: "image://theme/icon-m-search"
                //% "Search"
                text: qsTrId("lautta-settings-search")
                onClicked: page.open("SearchSettingsPage.qml")
            }
            SettingsEntry {
                objectName: "recentsEntry"
                iconSource: "image://theme/icon-m-history"
                //% "Recents"
                text: qsTrId("lautta-settings-recents")
                onClicked: page.open("RecentsSettingsPage.qml")
            }

            SectionHeader {
                //% "Files"
                text: qsTrId("lautta-settings-files")
            }
            SettingsEntry {
                objectName: "deletingEntry"
                iconSource: "image://theme/icon-m-delete"
                //% "Deleting"
                text: qsTrId("lautta-settings-deleting")
                onClicked: page.open("DeletingSettingsPage.qml")
            }
            SettingsEntry {
                objectName: "transfersEntry"
                iconSource: "image://theme/icon-m-transfer"
                //% "Transfers"
                text: qsTrId("lautta-settings-transfers")
                onClicked: page.open("TransfersSettingsPage.qml")
            }
            SettingsEntry {
                objectName: "storageEntry"
                iconSource: "image://theme/icon-m-storage"
                //% "Storage"
                text: qsTrId("lautta-settings-storage")
                onClicked: page.open("StorageSettingsPage.qml")
            }

            Item {
                width: 1
                height: Theme.paddingLarge
            }
            SettingsEntry {
                objectName: "aboutEntry"
                iconSource: "image://theme/icon-m-about"
                //% "About Lautta"
                text: qsTrId("lautta-settings-about")
                onClicked: page.open("AboutPage.qml")
            }
        }

        VerticalScrollDecorator { }
    }
}
