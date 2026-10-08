// SPDX-License-Identifier: LGPL-2.1-or-later
// Settings (SPEC §18; design: Settings): every global preference, recents,
// per-location settings and storage. Values are
// persisted through App.setSetting; QML keeps no copy.
import QtQuick 2.6
import Sailfish.Silica 1.0
import Lautta 1.0
import "../components/search"

Page {
    id: page

    // Servers, ad-hoc servers and volumes: [{ id, name, kind }].
    property var locations: []
    readonly property var largeOps: [[100, 100000000], [1000, 1000000000], [10000, 10000000000]]

    function days(list) {
        return list.map(function (n) {
            //% "%n days"
            return qsTrId("lautta-settings-days", n)
        })
    }

    function megabytes(list) {
        return list.map(function (n) {
            //% "Never"
            var never = qsTrId("lautta-settings-never")
            //% "Up to %1 MB"
            return n === 0 ? never : qsTrId("lautta-settings-up-to-mb").arg(n)
        })
    }

    function setting(key) {
        var dependency = App.settingsJson
        return App.setting(key)
    }

    function clearAppData() {
        cache.clearAppData()
    }

    allowedOrientations: Orientation.All
    Component.onCompleted: {
        cache.refresh()
        try {
            locations = JSON.parse(cache.locationsJson())
        } catch (e) {
            locations = []
        }
    }

    CacheInfo {
        id: cache

        onCacheCleared: {
            //% "Cache cleared"
            notice.text = qsTrId("lautta-settings-cache-cleared")
            notice.show()
        }
        onAppDataCleared: {
            App.loadSettings("{}")
            pageStack.pop(null)
        }
        onFailed: {
            notice.text = kind === "Locked"
                          //% "Finish or cancel transfers first"
                          ? qsTrId("lautta-settings-busy")
                          //% "Couldn't clear the data"
                          : qsTrId("lautta-settings-clear-failed")
            notice.show()
        }
    }

    Notice {
        id: notice
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
            SettingCombo {
                //% "Default view"
                label: qsTrId("lautta-settings-view")
                settingKey: "view_mode"
                values: ["list", "grid"]
                //% "List"
                labels: [qsTrId("lautta-settings-view-list"),
                         //% "Grid"
                         qsTrId("lautta-settings-view-grid")]
            }
            SettingCombo {
                //% "Sort by"
                label: qsTrId("lautta-settings-sort")
                settingKey: "sort_key"
                values: ["name", "size", "modified", "type"]
                //% "Name"
                labels: [qsTrId("lautta-settings-sort-name"),
                         //% "Size"
                         qsTrId("lautta-settings-sort-size"),
                         //% "Modified"
                         qsTrId("lautta-settings-sort-modified"),
                         //% "Type"
                         qsTrId("lautta-settings-sort-type")]
            }
            SettingSwitch {
                //% "Sort descending"
                text: qsTrId("lautta-settings-descending")
                settingKey: "sort_descending"
            }
            SettingSwitch {
                //% "Folders first"
                text: qsTrId("lautta-settings-folders-first")
                settingKey: "folders_first"
            }
            SettingSwitch {
                //% "Show hidden files"
                text: qsTrId("lautta-settings-hidden")
                settingKey: "show_hidden"
            }
            SettingCombo {
                //% "Date format"
                label: qsTrId("lautta-settings-date-format")
                settingKey: "date_format"
                values: ["relative", "iso", "locale"]
                //% "Relative (Today 09:14)"
                labels: [qsTrId("lautta-settings-date-relative"),
                         //% "ISO (2026-10-02 09:14)"
                         qsTrId("lautta-settings-date-iso"),
                         //% "System format"
                         qsTrId("lautta-settings-date-locale")]
            }
            SettingSwitch {
                //% "Thumbnails on this device"
                text: qsTrId("lautta-settings-thumbs-local")
                settingKey: "thumbnails_local"
            }
            SettingSwitch {
                //% "Thumbnails from servers"
                text: qsTrId("lautta-settings-thumbs-remote")
                settingKey: "thumbnails_remote"
            }
            SettingCombo {
                //% "Largest server file with a thumbnail"
                label: qsTrId("lautta-settings-thumbs-limit")
                settingKey: "thumbnail_max_remote_mb"
                values: [5, 10, 20, 50, 100]
                labels: page.megabytes(values)
            }
            SettingCombo {
                //% "On a metered connection"
                label: qsTrId("lautta-settings-thumbs-metered")
                settingKey: "thumbnail_max_metered_mb"
                values: [0, 1, 5, 10, 20]
                labels: page.megabytes(values)
            }
            SettingSlider {
                //% "Thumbnail cache"
                label: qsTrId("lautta-settings-thumb-cache")
                settingKey: "thumbnail_cache_mb"
                minimumValue: 50
                maximumValue: 1000
                stepSize: 50
                //% "%1 MB"
                valueFormat: function (n) { return qsTrId("lautta-settings-mb").arg(n) }
            }
            SettingCombo {
                //% "Keep stored listings"
                label: qsTrId("lautta-settings-listing-days")
                settingKey: "listing_cache_days"
                values: [1, 3, 7, 14, 30]
                labels: page.days(values)
            }

            SectionHeader {
                //% "Deleting"
                text: qsTrId("lautta-settings-deleting")
            }
            SettingSwitch {
                //% "Recently deleted"
                text: qsTrId("lautta-settings-trash")
                //% "Local deletes can be restored"
                description: qsTrId("lautta-settings-trash-hint")
                settingKey: "recently_deleted"
            }
            SettingCombo {
                //% "Keep deleted items"
                label: qsTrId("lautta-settings-trash-days")
                enabled: page.setting("recently_deleted") === true
                settingKey: "recently_deleted_retention_days"
                values: [7, 14, 30, 60, 90]
                labels: page.days(values)
            }
            SettingSlider {
                //% "Undo time (remorse)"
                label: qsTrId("lautta-settings-remorse")
                settingKey: "remorse_seconds"
                minimumValue: 3
                maximumValue: 10
                //% "%1 s"
                valueFormat: function (n) { return qsTrId("lautta-settings-seconds").arg(n) }
            }

            SectionHeader {
                //% "Transfers"
                text: qsTrId("lautta-settings-transfers")
            }
            SettingSlider {
                //% "Transfers at once on this device"
                label: qsTrId("lautta-settings-concurrency")
                settingKey: "local_concurrency"
                minimumValue: 1
                maximumValue: 4
            }
            SettingSlider {
                //% "Connections per server"
                label: qsTrId("lautta-settings-lanes")
                settingKey: "remote_lanes"
                minimumValue: 1
                maximumValue: 6
            }
            SettingSwitch {
                //% "Keep modification times"
                text: qsTrId("lautta-settings-mtimes")
                settingKey: "preserve_mtimes"
            }
            SettingSwitch {
                //% "Keep permissions"
                text: qsTrId("lautta-settings-permissions")
                settingKey: "preserve_permissions"
            }
            SettingSwitch {
                //% "Verify with checksums"
                text: qsTrId("lautta-settings-verify")
                //% "Always on for moves"
                description: qsTrId("lautta-settings-verify-hint")
                settingKey: "verify_checksums"
            }
            SettingSwitch {
                //% "Resume unfinished transfers at start"
                text: qsTrId("lautta-settings-resume")
                settingKey: "auto_resume"
            }
            SettingCombo {
                //% "Keep history"
                label: qsTrId("lautta-settings-history")
                settingKey: "history_retention_days"
                values: [7, 14, 30, 90, 365]
                labels: page.days(values)
            }
            ComboBox {
                id: largeBox

                readonly property real items: Number(page.setting("large_op_items"))
                readonly property real bytes: Number(page.setting("large_op_bytes"))

                //% "Ask before large operations"
                label: qsTrId("lautta-settings-large")
                //% "Over %1 items or %2"
                value: qsTrId("lautta-settings-large-value").arg(items).arg(Format.formatFileSize(bytes))
                menu: ContextMenu {
                    Repeater {
                        model: page.largeOps

                        MenuItem {
                            text: qsTrId("lautta-settings-large-value").arg(modelData[0])
                                  .arg(Format.formatFileSize(modelData[1]))
                            onClicked: {
                                App.setSetting("large_op_items", JSON.stringify(modelData[0]))
                                App.setSetting("large_op_bytes", JSON.stringify(modelData[1]))
                            }
                        }
                    }
                }
            }

            SectionHeader {
                //% "Search"
                text: qsTrId("lautta-settings-search")
            }
            SettingSlider {
                //% "Search depth on servers"
                label: qsTrId("lautta-settings-search-depth")
                settingKey: "search_remote_depth"
                minimumValue: 1
                maximumValue: 16
                //% "%n folders"
                valueFormat: function (n) { return qsTrId("lautta-settings-folders", n) }
            }

            SectionHeader {
                //% "Recents"
                text: qsTrId("lautta-settings-recents")
            }
            SettingSwitch {
                //% "Remember recent files"
                text: qsTrId("lautta-settings-recents-on")
                settingKey: "recents_enabled"
            }
            Button {
                anchors.horizontalCenter: parent.horizontalCenter
                //% "Clear recents"
                text: qsTrId("lautta-settings-clear-recents")
                onClicked: Remorse.popupAction(page,
                                               //% "Clearing recents"
                                               qsTrId("lautta-settings-clearing-recents"),
                                               function () { cache.clearRecents() },
                                               Number(App.setting("remorse_seconds")) * 1000)
            }

            SectionHeader {
                visible: page.locations.length > 0
                //% "Locations"
                text: qsTrId("lautta-settings-locations")
            }
            Repeater {
                model: page.locations

                BackgroundItem {
                    width: column.width
                    height: Theme.itemSizeSmall
                    onClicked: pageStack.push(Qt.resolvedUrl("LocationSettingsPage.qml"),
                                              { "locationId": modelData.id })

                    Label {
                        anchors {
                            left: parent.left
                            right: parent.right
                            leftMargin: Theme.horizontalPageMargin
                            rightMargin: Theme.horizontalPageMargin
                            verticalCenter: parent.verticalCenter
                        }
                        text: modelData.name
                        truncationMode: TruncationMode.Fade
                        color: parent.highlighted ? Theme.highlightColor : Theme.primaryColor
                    }
                }
            }

            SectionHeader {
                //% "Storage"
                text: qsTrId("lautta-settings-storage")
            }
            DetailItem {
                //% "Cache"
                label: qsTrId("lautta-settings-cache")
                value: Format.formatFileSize(cache.cacheBytes)
            }
            Button {
                anchors.horizontalCenter: parent.horizontalCenter
                enabled: !cache.busy
                //% "Clear cache"
                text: qsTrId("lautta-settings-clear-cache")
                onClicked: Remorse.popupAction(page,
                                               //% "Clearing cache"
                                               qsTrId("lautta-settings-clearing-cache"),
                                               function () { cache.clearCache() },
                                               Number(App.setting("remorse_seconds")) * 1000)
            }
            Item {
                width: 1
                height: Theme.paddingLarge
            }
            Button {
                anchors.horizontalCenter: parent.horizontalCenter
                enabled: !cache.busy
                //% "Clear app data"
                text: qsTrId("lautta-settings-clear-data")
                onClicked: Remorse.popupAction(page,
                                               //% "Clearing app data"
                                               qsTrId("lautta-settings-clearing-data"),
                                               function () { page.clearAppData() },
                                               Number(App.setting("remorse_seconds")) * 1000)
            }
            Label {
                x: Theme.horizontalPageMargin
                width: parent.width - 2 * Theme.horizontalPageMargin
                horizontalAlignment: Text.AlignHCenter
                wrapMode: Text.Wrap
                color: Theme.secondaryColor
                font.pixelSize: Theme.fontSizeSmall
                //% "Removes queue, history and favourites"
                text: qsTrId("lautta-settings-clear-data-hint")
            }

            Item {
                width: 1
                height: Theme.paddingLarge
            }
            BackgroundItem {
                width: column.width
                height: Theme.itemSizeMedium
                onClicked: pageStack.push(Qt.resolvedUrl("AboutPage.qml"))

                Label {
                    anchors {
                        left: parent.left
                        right: parent.right
                        leftMargin: Theme.horizontalPageMargin
                        rightMargin: Theme.horizontalPageMargin
                        verticalCenter: parent.verticalCenter
                    }
                    //% "About Lautta"
                    text: qsTrId("lautta-settings-about")
                    color: parent.highlighted ? Theme.highlightColor : Theme.primaryColor
                }
            }
        }

        VerticalScrollDecorator { }
    }
}
