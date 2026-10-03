// SPDX-License-Identifier: LGPL-2.1-or-later
// Search filters (SRC-2, SRC-3, SRC-4; design: SearchFilters): name, match
// mode, type, size, modified, depth on servers and recent searches. Accepting
// applies them to the SearchModel and starts the search.
import QtQuick 2.6
import Sailfish.Silica 1.0
import Lautta 1.0
import "../components"
import "../components/search"

Dialog {
    id: dialog

    // The SearchModel of the search page.
    property var searchModel

    readonly property var typeIds: ["", "folder", "image", "video", "audio", "document", "archive", "text"]
    readonly property var sizeIds: ["any", "gt1m", "gt10m", "gt100m", "lt100k", "lt1m"]
    readonly property var dateIds: ["any", "today", "week", "month", "year"]
    readonly property bool remote: searchModel ? !App.isLocal(searchModel.rootUri) : false
    property var recent: []

    function load() {
        if (!searchModel)
            return
        nameField.text = searchModel.query
        matchBox.currentIndex = searchModel.matchMode === "glob" ? 1 : 0
        var types = []
        try {
            types = JSON.parse(searchModel.types || "[]")
        } catch (e) {
            types = []
        }
        typeBox.currentIndex = Math.max(0, typeIds.indexOf(types.length > 0 ? types[0] : ""))
        sizeBox.currentIndex = Math.max(0, sizeIds.indexOf(searchModel.sizePreset || "any"))
        dateBox.currentIndex = Math.max(0, dateIds.indexOf(searchModel.datePreset || "any"))
        hiddenSwitch.checked = searchModel.includeHidden
        try {
            recent = JSON.parse(searchModel.recentSearchesJson || "[]")
        } catch (e2) {
            recent = []
        }
    }

    function apply() {
        if (!searchModel)
            return
        searchModel.query = nameField.text
        searchModel.matchMode = matchBox.currentIndex === 1 ? "glob" : "substring"
        searchModel.types = JSON.stringify(typeBox.currentIndex > 0 ? [typeIds[typeBox.currentIndex]] : [])
        searchModel.sizePreset = sizeIds[sizeBox.currentIndex]
        searchModel.datePreset = dateIds[dateBox.currentIndex]
        searchModel.includeHidden = hiddenSwitch.checked
        searchModel.start()
    }

    allowedOrientations: Orientation.All
    canAccept: nameField.text.length > 0 || typeBox.currentIndex > 0 || sizeBox.currentIndex > 0
               || dateBox.currentIndex > 0
    onAccepted: apply()
    Component.onCompleted: load()

    SilicaFlickable {
        anchors.fill: parent
        contentHeight: column.height

        Column {
            id: column

            width: parent.width

            DialogHeader {
                //% "Search"
                acceptText: qsTrId("lautta-filters-accept")
                //% "Cancel"
                cancelText: qsTrId("lautta-filters-cancel")
            }

            Label {
                x: Theme.horizontalPageMargin
                width: parent.width - 2 * Theme.horizontalPageMargin
                horizontalAlignment: Text.AlignRight
                color: Theme.highlightColor
                font.pixelSize: Theme.fontSizeSmall
                truncationMode: TruncationMode.Fade
                //% "Search in %1"
                text: searchModel ? qsTrId("lautta-filters-in").arg(App.displayAddress(searchModel.rootUri)) : ""
            }

            TextField {
                id: nameField

                width: parent.width
                //% "Name"
                label: qsTrId("lautta-filters-name")
                placeholderText: label
                inputMethodHints: Qt.ImhNoPredictiveText | Qt.ImhNoAutoUppercase
                EnterKey.iconSource: "image://theme/icon-m-enter-next"
                EnterKey.onClicked: focus = false
            }

            ComboBox {
                id: matchBox

                //% "Match"
                label: qsTrId("lautta-filters-match")
                menu: ContextMenu {
                    MenuItem {
                        //% "Contains"
                        text: qsTrId("lautta-filters-match-substring")
                    }
                    MenuItem {
                        //% "Glob"
                        text: qsTrId("lautta-filters-match-glob")
                    }
                }
            }

            ComboBox {
                id: typeBox

                //% "Type"
                label: qsTrId("lautta-filters-type")
                menu: ContextMenu {
                    MenuItem {
                        //% "Any"
                        text: qsTrId("lautta-filters-type-any")
                    }
                    MenuItem {
                        //% "Folders"
                        text: qsTrId("lautta-filters-type-folder")
                    }
                    MenuItem {
                        //% "Images"
                        text: qsTrId("lautta-filters-type-image")
                    }
                    MenuItem {
                        //% "Videos"
                        text: qsTrId("lautta-filters-type-video")
                    }
                    MenuItem {
                        //% "Audio"
                        text: qsTrId("lautta-filters-type-audio")
                    }
                    MenuItem {
                        //% "Documents"
                        text: qsTrId("lautta-filters-type-document")
                    }
                    MenuItem {
                        //% "Archives"
                        text: qsTrId("lautta-filters-type-archive")
                    }
                    MenuItem {
                        //% "Text files"
                        text: qsTrId("lautta-filters-type-text")
                    }
                }
            }

            ComboBox {
                id: sizeBox

                //% "Size"
                label: qsTrId("lautta-filters-size")
                menu: ContextMenu {
                    MenuItem {
                        //% "Any size"
                        text: qsTrId("lautta-filters-size-any")
                    }
                    MenuItem {
                        //% "Larger than 1 MB"
                        text: qsTrId("lautta-filters-size-gt1m")
                    }
                    MenuItem {
                        //% "Larger than 10 MB"
                        text: qsTrId("lautta-filters-size-gt10m")
                    }
                    MenuItem {
                        //% "Larger than 100 MB"
                        text: qsTrId("lautta-filters-size-gt100m")
                    }
                    MenuItem {
                        //% "Smaller than 100 kB"
                        text: qsTrId("lautta-filters-size-lt100k")
                    }
                    MenuItem {
                        //% "Smaller than 1 MB"
                        text: qsTrId("lautta-filters-size-lt1m")
                    }
                }
            }

            ComboBox {
                id: dateBox

                //% "Modified"
                label: qsTrId("lautta-filters-modified")
                menu: ContextMenu {
                    MenuItem {
                        //% "Any time"
                        text: qsTrId("lautta-filters-date-any")
                    }
                    MenuItem {
                        //% "Today"
                        text: qsTrId("lautta-filters-date-today")
                    }
                    MenuItem {
                        //% "Past week"
                        text: qsTrId("lautta-filters-date-week")
                    }
                    MenuItem {
                        //% "Past month"
                        text: qsTrId("lautta-filters-date-month")
                    }
                    MenuItem {
                        //% "This year"
                        text: qsTrId("lautta-filters-date-year")
                    }
                }
            }

            TextSwitch {
                id: hiddenSwitch

                //% "Include hidden files"
                text: qsTrId("lautta-filters-hidden")
            }

            SettingSlider {
                visible: dialog.remote
                settingKey: "search_remote_depth"
                minimumValue: 1
                maximumValue: 16
                //% "%n folders"
                valueFormat: function (n) { return qsTrId("lautta-filters-depth-value", n) }
                //% "Search depth on servers"
                label: qsTrId("lautta-filters-depth")
            }

            SectionHeader {
                visible: dialog.recent.length > 0
                //% "Recent searches"
                text: qsTrId("lautta-filters-recent")
            }

            Repeater {
                model: dialog.recent

                BackgroundItem {
                    width: column.width
                    height: Theme.itemSizeSmall
                    onClicked: {
                        nameField.text = modelData
                        dialog.accept()
                    }

                    AppIcon {
                        id: recentIcon

                        x: Theme.horizontalPageMargin
                        anchors.verticalCenter: parent.verticalCenter
                        icon: "image://theme/icon-m-search"
                        small: true
                        highlighted: parent.highlighted
                    }
                    Label {
                        anchors {
                            left: recentIcon.right
                            leftMargin: Theme.paddingLarge
                            right: parent.right
                            rightMargin: Theme.horizontalPageMargin
                            verticalCenter: parent.verticalCenter
                        }
                        text: modelData
                        truncationMode: TruncationMode.Fade
                        color: parent.highlighted ? Theme.highlightColor : Theme.primaryColor
                    }
                }
            }
        }

        VerticalScrollDecorator { }
    }
}
