// SPDX-License-Identifier: LGPL-2.1-or-later
// View options (board ViewOptions, SPEC BRW-3/BRW-4): sort, view, grid size
// and thumbnails, the same for every folder, with a live preview below. A
// settings page, not a dialog: every control applies at once, and folders
// already open follow. `model` is the DirectoryModel of the folder the page
// was opened from (none from Settings): it also takes a choice that only
// differed there, such as List over a suggested grid.
import QtQuick 2.6
import Sailfish.Silica 1.0
import Lautta 1.0
import "../components/directory"

Page {
    id: page

    property var model
    readonly property var sortKeys: ["name", "size", "modified", "type"]
    readonly property alias preview: preview

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
                //% "View options"
                title: qsTrId("lautta-dir-opts-title")
                //% "Used in every folder"
                description: qsTrId("lautta-dir-opts-every-folder")
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

            TextSwitch {
                objectName: "thumbnailsSwitch"
                automaticCheck: false
                checked: page.setting("thumbnails_local") === true
                //% "Thumbnails"
                text: qsTrId("lautta-dir-opts-thumbnails")
                onClicked: page.choose("thumbnails_local", "thumbnails", !checked)
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
        }

        VerticalScrollDecorator { }
    }
}
