// SPDX-License-Identifier: LGPL-2.1-or-later
import QtQuick 2.6
import Sailfish.Silica 1.0
import Lautta 1.0
import "../components"
import "../components/search"
import "../components/search/IndexText.js" as IndexText

Page {
    id: page

    readonly property var indexRows: IndexText.rows(JSON.parse(indexInfo.statusJson || "[]"))

    objectName: "searchSettingsPage"
    allowedOrientations: Orientation.All

    IndexInfo {
        id: indexInfo

        Component.onCompleted: refresh()
    }

    SilicaFlickable {
        anchors.fill: parent
        contentHeight: column.height + Theme.paddingLarge

        Column {
            id: column

            width: parent.width

            HeaderBand {
                PageHeader {
                    title: qsTrId("lautta-settings-search")
                }
            }

            Label {
                objectName: "indexIntro"
                x: Theme.horizontalPageMargin
                width: parent.width - 2 * Theme.horizontalPageMargin
                wrapMode: Text.Wrap
                color: Theme.secondaryHighlightColor
                font.pixelSize: Theme.fontSizeSmall
                //% "Lautta keeps a list of file names, so search answers as you type and finds files on servers that are offline. It never reads what files contain."
                text: qsTrId("lautta-index-explain")
            }

            Item {
                width: 1
                height: Theme.paddingLarge
            }

            Repeater {
                model: page.indexRows

                Item {
                    objectName: "indexRow"
                    readonly property bool server: modelData.kind === "server"
                    width: column.width
                    height: server ? serverSwitch.height : line.height + Theme.paddingMedium

                    Column {
                        id: line

                        visible: !parent.server
                        x: Theme.horizontalPageMargin
                        width: parent.width - 2 * x
                        y: Theme.paddingSmall

                        Label {
                            width: parent.width
                            text: IndexText.name(modelData)
                            truncationMode: TruncationMode.Fade
                        }
                        Label {
                            width: parent.width
                            text: IndexText.status(modelData)
                            color: Theme.secondaryColor
                            font.pixelSize: Theme.fontSizeExtraSmall
                            truncationMode: TruncationMode.Fade
                        }
                    }

                    TextSwitch {
                        id: serverSwitch

                        objectName: "indexServerSwitch"
                        visible: parent.server
                        automaticCheck: false
                        checked: modelData.wanted
                        text: IndexText.name(modelData)
                        description: IndexText.status(modelData)
                        onClicked: indexInfo.setServerIndexed(modelData.location, !modelData.wanted)
                    }
                }
            }

            SettingSwitch {
                objectName: "indexOnCharger"
                settingKey: "index_on_charger"
                //% "Update while charging"
                text: qsTrId("lautta-index-charger")
                //% "With the charger in and the battery above half, Lautta also updates the index while it runs in the background."
                description: qsTrId("lautta-index-charger-hint")
            }
        }

        VerticalScrollDecorator { }
    }
}
