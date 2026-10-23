// SPDX-License-Identifier: LGPL-2.1-or-later
// Settings (SPEC §18; design: Settings), laid out as Salama's settings: a
// list of entries, each opening a page of settings and telling under its
// name what is chosen there. Values are persisted through App.setSetting;
// QML keeps no copy.
import QtQuick 2.6
import Sailfish.Silica 1.0
import Lautta 1.0
import "../components/search"
import "../components/search/SettingsText.js" as SettingsText

Page {
    id: page

    function setting(key) {
        var dependency = App.settingsJson
        return App.setting(key)
    }

    function open(file) {
        pageStack.push(Qt.resolvedUrl(file))
    }

    objectName: "settingsPage"
    allowedOrientations: Orientation.All
    // Back from Storage, where the cache may have been cleared.
    onStatusChanged: if (status === PageStatus.Activating) cache.refresh()

    CacheInfo {
        id: cache
    }

    SilicaFlickable {
        anchors.fill: parent
        contentHeight: column.height + Theme.paddingLarge

        Column {
            id: column

            width: parent.width

            PageHeader {
                //% "Settings"
                title: qsTrId("lautta-settings-title")
            }

            SectionHeader {
                //% "Browsing"
                text: qsTrId("lautta-settings-browsing")
            }
            SettingsEntry {
                objectName: "browsePageEntry"
                iconSource: "image://theme/icon-m-home"
                //% "Browse page"
                text: qsTrId("lautta-settings-browse-page")
                onClicked: page.open("BrowseSettingsPage.qml")
            }
            SettingsEntry {
                // The view settings live on their own page, the one folders
                // open from their pulley (BRW-4), with the thumbnails.
                objectName: "viewOptionsRow"
                iconSource: "image://theme/icon-m-month-view"
                //% "View options"
                text: qsTrId("lautta-settings-view-options")
                value: SettingsText.viewOptions(page.setting("view_mode"), page.setting("grid_columns"),
                                                page.setting("sort_key"))
                onClicked: page.open("ViewOptionsPage.qml")
            }
            SettingsEntry {
                objectName: "searchEntry"
                iconSource: "image://theme/icon-m-search"
                //% "Search"
                text: qsTrId("lautta-settings-search")
                value: SettingsText.search(Number(page.setting("search_remote_depth")))
                onClicked: page.open("SearchSettingsPage.qml")
            }
            SettingsEntry {
                objectName: "recentsEntry"
                iconSource: "image://theme/icon-m-history"
                //% "Recents"
                text: qsTrId("lautta-settings-recents")
                value: SettingsText.recents(page.setting("recents_enabled") === true)
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
                value: SettingsText.deleting(page.setting("recently_deleted") === true,
                                             Number(page.setting("recently_deleted_retention_days")))
                onClicked: page.open("DeletingSettingsPage.qml")
            }
            SettingsEntry {
                objectName: "transfersEntry"
                iconSource: "image://theme/icon-m-transfer"
                //% "Transfers"
                text: qsTrId("lautta-settings-transfers")
                value: SettingsText.transfers(Number(page.setting("local_concurrency")))
                onClicked: page.open("TransfersSettingsPage.qml")
            }
            SettingsEntry {
                objectName: "storageEntry"
                iconSource: "image://theme/icon-m-storage"
                //% "Storage"
                text: qsTrId("lautta-settings-storage")
                value: SettingsText.storage(Format.formatFileSize(cache.cacheBytes))
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
