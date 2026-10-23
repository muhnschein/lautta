// SPDX-License-Identifier: LGPL-2.1-or-later
// View options (board ViewOptions, SPEC BRW-3/BRW-4): sort, view and grid
// size, the same for every folder, with a live preview below them, then the
// thumbnails: the switch for all of them, and those from servers with their
// limits and cache (PRV-1, PRV-2). A settings page, not a dialog: every
// control applies at once, and folders already open follow. `model` is the
// DirectoryModel of the folder the page was opened from (none from
// Settings): it also takes a choice that only differed there, such as List
// over a suggested grid.
import QtQuick 2.6
import Sailfish.Silica 1.0
import Lautta 1.0
import "../components/directory"
import "../components/search"
import "../components/search/SettingsText.js" as SettingsText

Page {
    id: page

    property var model
    readonly property var sortKeys: ["name", "size", "modified", "type"]
    readonly property alias preview: preview
    readonly property bool thumbnails: setting("thumbnails_local") === true

    function setting(key) {
        var dependency = App.settingsJson
        return App.setting(key)
    }

    // Stores one view setting for every folder; `pref` is its name for
    // DirectoryModel.setViewPrefs, empty when the model has none.
    function choose(key, pref, value) {
        App.setSetting(key, JSON.stringify(value))
        if (model && pref.length > 0) {
            var prefs = {}
            prefs[pref] = value
            model.setViewPrefs(JSON.stringify(prefs))
        }
    }

    objectName: "viewOptionsPage"
    allowedOrientations: Orientation.All

    SilicaFlickable {
        anchors.fill: parent
        contentHeight: column.height + Theme.paddingLarge

        Column {
            id: column

            width: parent.width

            PageHeader {
                objectName: "viewOptionsHeader"
                //% "View options"
                title: qsTrId("lautta-dir-opts-title")
            }

            ComboBox {
                objectName: "sortBox"
                //% "Sort by"
                label: qsTrId("lautta-dir-opts-sort")
                currentIndex: Math.max(0, page.sortKeys.indexOf(page.setting("sort_key")))
                menu: ContextMenu {
                    MenuItem {
                        objectName: "name"
                        //% "Name"
                        text: qsTrId("lautta-dir-opts-sort-name")
                        onClicked: page.choose("sort_key", "sortKey", objectName)
                    }
                    MenuItem {
                        objectName: "size"
                        //% "Size"
                        text: qsTrId("lautta-dir-opts-sort-size")
                        onClicked: page.choose("sort_key", "sortKey", objectName)
                    }
                    MenuItem {
                        objectName: "modified"
                        //% "Modified"
                        text: qsTrId("lautta-dir-opts-sort-modified")
                        onClicked: page.choose("sort_key", "sortKey", objectName)
                    }
                    MenuItem {
                        objectName: "type"
                        //% "Type"
                        text: qsTrId("lautta-dir-opts-sort-type")
                        onClicked: page.choose("sort_key", "sortKey", objectName)
                    }
                }
            }

            TextSwitch {
                objectName: "descendingSwitch"
                automaticCheck: false
                checked: page.setting("sort_descending") === true
                //% "Descending"
                text: qsTrId("lautta-dir-opts-descending")
                onClicked: page.choose("sort_descending", "descending", !checked)
            }

            TextSwitch {
                objectName: "foldersFirstSwitch"
                automaticCheck: false
                checked: page.setting("folders_first") === true
                //% "Folders first"
                text: qsTrId("lautta-dir-opts-folders-first")
                onClicked: page.choose("folders_first", "foldersFirst", !checked)
            }

            TextSwitch {
                objectName: "hiddenSwitch"
                automaticCheck: false
                checked: page.setting("show_hidden") === true
                //% "Show hidden files"
                text: qsTrId("lautta-dir-opts-hidden")
                //% "Names starting with a dot and hidden files"
                description: qsTrId("lautta-dir-opts-hidden-hint")
                onClicked: page.choose("show_hidden", "showHidden", !checked)
            }

            ComboBox {
                objectName: "viewBox"
                //% "View"
                label: qsTrId("lautta-dir-opts-view")
                currentIndex: page.setting("view_mode") === "grid" ? 1 : 0
                menu: ContextMenu {
                    MenuItem {
                        objectName: "list"
                        //% "List"
                        text: qsTrId("lautta-dir-opts-view-list")
                        onClicked: page.choose("view_mode", "viewMode", objectName)
                    }
                    MenuItem {
                        objectName: "grid"
                        //% "Grid"
                        text: qsTrId("lautta-dir-opts-view-grid")
                        onClicked: page.choose("view_mode", "viewMode", objectName)
                    }
                }
            }

            ComboBox {
                objectName: "gridBox"
                visible: page.setting("view_mode") === "grid"
                //% "Grid size"
                label: qsTrId("lautta-dir-opts-grid-size")
                currentIndex: page.setting("grid_columns") === 3 ? 1 : 0
                menu: ContextMenu {
                    MenuItem {
                        objectName: "4"
                        //% "4 per row"
                        text: qsTrId("lautta-dir-opts-grid-4")
                        onClicked: page.choose("grid_columns", "", 4)
                    }
                    MenuItem {
                        objectName: "3"
                        //% "3 per row, with names"
                        text: qsTrId("lautta-dir-opts-grid-3")
                        onClicked: page.choose("grid_columns", "", 3)
                    }
                }
            }

            SectionHeader {
                //% "Preview"
                text: qsTrId("lautta-dir-opts-preview")
            }
            Item {
                width: 1
                height: Theme.paddingLarge
            }
            ViewPreview {
                id: preview

                width: parent.width
                screenWidth: page.width
                screenHeight: page.height
            }

            // Global like everything on this page (BRW-4), whichever folder
            // it was opened from; the server ones are not view settings but
            // belong with the switch that turns every thumbnail off (PRV-1,
            // PRV-2).
            SectionHeader {
                objectName: "thumbnailsSection"
                //% "Thumbnails"
                text: qsTrId("lautta-dir-opts-thumbnails-section")
            }
            TextSwitch {
                objectName: "thumbnailsSwitch"
                automaticCheck: false
                checked: page.thumbnails
                //% "Show thumbnails"
                text: qsTrId("lautta-dir-opts-thumbnails")
                //% "Making thumbnails decodes files as soon as they show in a folder, including files you may not trust. When off, a file is decoded only when you open it."
                description: qsTrId("lautta-dir-opts-thumbnails-hint")
                onClicked: page.choose("thumbnails_local", "thumbnails", !checked)
            }
            SettingSwitch {
                id: remoteThumbnails

                objectName: "remoteThumbnailsSwitch"
                enabled: page.thumbnails
                //% "Thumbnails from servers"
                text: qsTrId("lautta-settings-thumbs-remote")
                //% "Downloads pictures from servers to make them, which uses more data and battery and can slow down browsing"
                description: qsTrId("lautta-settings-thumbs-remote-hint")
                settingKey: "thumbnails_remote"
            }
            SettingCombo {
                objectName: "remoteThumbnailLimitBox"
                enabled: page.thumbnails && remoteThumbnails.current
                //% "Largest server file with a thumbnail"
                label: qsTrId("lautta-settings-thumbs-limit")
                settingKey: "thumbnail_max_remote_mb"
                values: [5, 10, 20, 50, 100]
                labels: SettingsText.megabytes(values)
            }
            SettingCombo {
                objectName: "meteredThumbnailLimitBox"
                enabled: page.thumbnails && remoteThumbnails.current
                //% "On a metered connection"
                label: qsTrId("lautta-settings-thumbs-metered")
                settingKey: "thumbnail_max_metered_mb"
                values: [0, 1, 5, 10, 20]
                labels: SettingsText.megabytes(values)
            }
            SettingSlider {
                objectName: "thumbnailCacheSlider"
                //% "Thumbnail cache"
                label: qsTrId("lautta-settings-thumb-cache")
                settingKey: "thumbnail_cache_mb"
                minimumValue: 50
                maximumValue: 1000
                stepSize: 50
                //% "%1 MB"
                valueFormat: function (n) { return qsTrId("lautta-settings-mb").arg(n) }
            }
        }

        VerticalScrollDecorator { }
    }
}
