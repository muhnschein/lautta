// SPDX-License-Identifier: LGPL-2.1-or-later
// View options of a folder (board ViewOptions, SPEC BRW-3/BRW-4): sort,
// view, thumbnails, and whether to remember them for this
// folder or for all folders. `model` is the folder's DirectoryModel.
import QtQuick 2.6
import Sailfish.Silica 1.0
import Lautta 1.0

Dialog {
    id: dialog

    property var model
    readonly property var sortKeys: ["name", "size", "modified", "type"]

    function _apply() {
        var everywhere = scopeBox.currentIndex === 1
        var prefs = {
            "sortKey": sortKeys[sortBox.currentIndex],
            "descending": descendingSwitch.checked,
            "foldersFirst": foldersFirstSwitch.checked,
            "showHidden": hiddenSwitch.checked,
            "viewMode": viewBox.currentIndex === 1 ? "grid" : "list",
            "thumbnails": thumbnailsSwitch.checked
        }
        if (everywhere) {
            App.setSetting("sort_key", JSON.stringify(prefs.sortKey))
            App.setSetting("sort_descending", JSON.stringify(prefs.descending))
            App.setSetting("folders_first", JSON.stringify(prefs.foldersFirst))
            App.setSetting("show_hidden", JSON.stringify(prefs.showHidden))
            App.setSetting("view_mode", JSON.stringify(prefs.viewMode))
            App.setSetting("thumbnails_local", JSON.stringify(prefs.thumbnails))
            prefs = { "scope": "all" }
        }
        model.setViewPrefs(JSON.stringify(prefs))
    }

    allowedOrientations: Orientation.All
    onAccepted: _apply()

    SilicaFlickable {
        anchors.fill: parent
        contentHeight: column.height + Theme.paddingLarge

        Column {
            id: column

            width: parent.width

            DialogHeader {
                //% "Apply"
                acceptText: qsTrId("lautta-dir-opts-apply")
                //% "View options · %1"
                title: qsTrId("lautta-dir-opts-title").arg(dialog.model ? dialog.model.title : "")
            }

            ComboBox {
                id: sortBox

                //% "Sort by"
                label: qsTrId("lautta-dir-opts-sort")
                currentIndex: Math.max(0, dialog.sortKeys.indexOf(dialog.model ? dialog.model.sortKey : "name"))
                menu: ContextMenu {
                    MenuItem {
                        //% "Name"
                        text: qsTrId("lautta-dir-opts-sort-name")
                    }
                    MenuItem {
                        //% "Size"
                        text: qsTrId("lautta-dir-opts-sort-size")
                    }
                    MenuItem {
                        //% "Modified"
                        text: qsTrId("lautta-dir-opts-sort-modified")
                    }
                    MenuItem {
                        //% "Type"
                        text: qsTrId("lautta-dir-opts-sort-type")
                    }
                }
            }

            TextSwitch {
                id: descendingSwitch

                checked: dialog.model ? dialog.model.descending : false
                //% "Descending"
                text: qsTrId("lautta-dir-opts-descending")
            }

            TextSwitch {
                id: foldersFirstSwitch

                checked: dialog.model ? dialog.model.foldersFirst : true
                //% "Folders first"
                text: qsTrId("lautta-dir-opts-folders-first")
            }

            TextSwitch {
                id: hiddenSwitch

                checked: dialog.model ? dialog.model.showHidden : false
                //% "Show hidden files"
                text: qsTrId("lautta-dir-opts-hidden")
                //% "Names starting with a dot and hidden files"
                description: qsTrId("lautta-dir-opts-hidden-hint")
            }

            ComboBox {
                id: viewBox

                //% "View"
                label: qsTrId("lautta-dir-opts-view")
                currentIndex: dialog.model && dialog.model.viewMode === "grid" ? 1 : 0
                menu: ContextMenu {
                    MenuItem {
                        //% "List"
                        text: qsTrId("lautta-dir-opts-view-list")
                    }
                    MenuItem {
                        //% "Grid"
                        text: qsTrId("lautta-dir-opts-view-grid")
                    }
                }
            }

            TextSwitch {
                id: thumbnailsSwitch

                checked: dialog.model ? dialog.model.thumbnails : true
                //% "Thumbnails"
                text: qsTrId("lautta-dir-opts-thumbnails")
            }

            SectionHeader {
                //% "Apply to"
                text: qsTrId("lautta-dir-opts-apply-to")
            }

            ComboBox {
                id: scopeBox

                //% "Remember for"
                label: qsTrId("lautta-dir-opts-remember")
                menu: ContextMenu {
                    MenuItem {
                        //% "This folder"
                        text: qsTrId("lautta-dir-opts-remember-folder")
                    }
                    MenuItem {
                        //% "All folders"
                        text: qsTrId("lautta-dir-opts-remember-all")
                    }
                }
            }
        }
    }
}
