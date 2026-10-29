// SPDX-License-Identifier: LGPL-2.1-or-later
// Settings › Deleting (SPEC §18, OPS-8): whether deletes go to Trash, how
// long Trash keeps them and the undo time of every remorse timer.
import QtQuick 2.6
import Sailfish.Silica 1.0
import Lautta 1.0
import "../components/search"
import "../components/search/SettingsText.js" as SettingsText

Page {
    id: page

    objectName: "deletingSettingsPage"
    allowedOrientations: Orientation.All

    SilicaFlickable {
        anchors.fill: parent
        contentHeight: column.height + Theme.paddingLarge

        Column {
            id: column

            width: parent.width

            PageHeader {
                title: qsTrId("lautta-settings-deleting")
            }

            SettingSwitch {
                id: trash

                //% "Move deleted items to Trash"
                text: qsTrId("lautta-settings-trash")
                //% "Files deleted from this device's folders can be restored"
                description: qsTrId("lautta-settings-trash-hint")
                settingKey: "recently_deleted"
            }
            SettingCombo {
                //% "Keep items in Trash"
                label: qsTrId("lautta-settings-trash-days")
                enabled: trash.current
                settingKey: "recently_deleted_retention_days"
                values: [7, 14, 30, 60, 90]
                labels: SettingsText.days(values)
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
        }

        VerticalScrollDecorator { }
    }
}
