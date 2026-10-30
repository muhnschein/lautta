// SPDX-License-Identifier: LGPL-2.1-or-later
import QtQuick 2.6
import Sailfish.Silica 1.0
import Lautta 1.0
import "../components/search"
import "../components/search/SettingsText.js" as SettingsText

Page {
    id: page

    readonly property var largeOps: [[100, 100000000], [1000, 1000000000], [10000, 10000000000]]

    function setting(key) {
        var dependency = App.settingsJson
        return App.setting(key)
    }

    objectName: "transfersSettingsPage"
    allowedOrientations: Orientation.All

    SilicaFlickable {
        anchors.fill: parent
        contentHeight: column.height + Theme.paddingLarge

        Column {
            id: column

            width: parent.width

            PageHeader {
                title: qsTrId("lautta-settings-transfers")
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
                //% "Resume unfinished tasks at start"
                text: qsTrId("lautta-settings-resume")
                settingKey: "auto_resume"
            }
            SettingCombo {
                //% "Keep history"
                label: qsTrId("lautta-settings-history")
                settingKey: "history_retention_days"
                values: [7, 14, 30, 90, 365]
                labels: SettingsText.days(values)
            }
            ComboBox {
                // Two keys at once: no SettingCombo.
                objectName: "largeOpsBox"

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
        }

        VerticalScrollDecorator { }
    }
}
