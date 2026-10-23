// SPDX-License-Identifier: LGPL-2.1-or-later
// Settings › Search (SPEC §18, SRC-3): how deep a search on a server goes;
// searches on this device have no limit.
import QtQuick 2.6
import Sailfish.Silica 1.0
import Lautta 1.0
import "../components/search"

Page {
    id: page

    objectName: "searchSettingsPage"
    allowedOrientations: Orientation.All

    SilicaFlickable {
        anchors.fill: parent
        contentHeight: column.height + Theme.paddingLarge

        Column {
            id: column

            width: parent.width

            PageHeader {
                title: qsTrId("lautta-settings-search")
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
            Label {
                x: Theme.horizontalPageMargin
                width: parent.width - 2 * Theme.horizontalPageMargin
                wrapMode: Text.Wrap
                color: Theme.secondaryHighlightColor
                font.pixelSize: Theme.fontSizeSmall
                //% "Searches on this device go through every folder"
                text: qsTrId("lautta-settings-search-depth-hint")
            }
        }

        VerticalScrollDecorator { }
    }
}
