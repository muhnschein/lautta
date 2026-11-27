// SPDX-License-Identifier: LGPL-2.1-or-later
import QtQuick 2.6
import Sailfish.Silica 1.0
import Lautta 1.0
import "../components/search"

Page {
    id: page

    objectName: "recentsSettingsPage"
    allowedOrientations: Orientation.All

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
                title: qsTrId("lautta-settings-recents")
            }

            SettingSwitch {
                //% "Remember recent files"
                text: qsTrId("lautta-settings-recents-on")
                settingKey: "recents_enabled"
            }
            Item {
                width: 1
                height: Theme.paddingLarge
            }
            Button {
                objectName: "clearRecentsButton"
                anchors.horizontalCenter: parent.horizontalCenter
                //% "Clear recents"
                text: qsTrId("lautta-settings-clear-recents")
                onClicked: Remorse.popupAction(page,
                                               //% "Clearing recents"
                                               qsTrId("lautta-settings-clearing-recents"),
                                               function () { cache.clearRecents() },
                                               Number(App.setting("remorse_seconds")) * 1000)
            }
        }

        VerticalScrollDecorator { }
    }
}
