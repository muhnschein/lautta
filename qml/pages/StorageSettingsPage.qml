// SPDX-License-Identifier: LGPL-2.1-or-later
import QtQuick 2.6
import Sailfish.Silica 1.0
import Lautta 1.0
import "../components/search"
import "../components/search/SettingsText.js" as SettingsText

Page {
    id: page

    readonly property int remorseMs: {
        var dependency = App.settingsJson
        return Number(App.setting("remorse_seconds")) * 1000
    }

    objectName: "storageSettingsPage"
    allowedOrientations: Orientation.All
    Component.onCompleted: cache.refresh()

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
                          //% "Finish or cancel running tasks first"
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
                title: qsTrId("lautta-settings-storage")
            }

            SettingCombo {
                //% "Keep stored listings"
                label: qsTrId("lautta-settings-listing-days")
                settingKey: "listing_cache_days"
                values: [1, 3, 7, 14, 30]
                labels: SettingsText.days(values)
            }
            DetailItem {
                //% "Cache"
                label: qsTrId("lautta-settings-cache")
                value: Format.formatFileSize(cache.cacheBytes)
            }
            Button {
                objectName: "clearCacheButton"
                anchors.horizontalCenter: parent.horizontalCenter
                enabled: !cache.busy
                //% "Clear cache"
                text: qsTrId("lautta-settings-clear-cache")
                onClicked: Remorse.popupAction(page,
                                               //% "Clearing cache"
                                               qsTrId("lautta-settings-clearing-cache"),
                                               function () { cache.clearCache() },
                                               page.remorseMs)
            }
            Item {
                width: 1
                height: Theme.paddingLarge
            }
            Button {
                objectName: "clearAppDataButton"
                anchors.horizontalCenter: parent.horizontalCenter
                enabled: !cache.busy
                //% "Clear app data"
                text: qsTrId("lautta-settings-clear-data")
                onClicked: Remorse.popupAction(page,
                                               //% "Clearing app data"
                                               qsTrId("lautta-settings-clearing-data"),
                                               function () { cache.clearAppData() },
                                               page.remorseMs)
            }
            Label {
                x: Theme.horizontalPageMargin
                width: parent.width - 2 * Theme.horizontalPageMargin
                horizontalAlignment: Text.AlignHCenter
                wrapMode: Text.Wrap
                color: Theme.secondaryColor
                font.pixelSize: Theme.fontSizeSmall
                //% "Removes queue, history, favourites and everything in Trash"
                text: qsTrId("lautta-settings-clear-data-hint")
            }
        }

        VerticalScrollDecorator { }
    }
}
