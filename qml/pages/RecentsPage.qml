// SPDX-License-Identifier: LGPL-2.1-or-later
import QtQuick 2.6
import Sailfish.Silica 1.0
import Lautta 1.0
import "../components"
import "../components/Open.js" as Open
import "../components/browse"

Page {
    id: page

    property var listModel: recents
    readonly property bool recentsOn: JSON.parse(App.settingsJson).recents_enabled !== false

    allowedOrientations: Orientation.All

    Connections {
        target: App
        onSettingsJsonChanged: recents.reload()
    }

    function openRecent(row) {
        if (row.kind === "transferred")
            pageStack.push(Qt.resolvedUrl("DirectoryPage.qml"), { "uri": App.parentUri(row.uri) })
        else
            Open.open(pageStack, App, Operations, row.uri, false, "", "")
    }

    RecentsModel {
        id: recents
        Component.onCompleted: reload()
    }

    SilicaListView {
        id: list

        anchors.fill: parent
        model: page.listModel

        header: PageHeader {
            //% "Recents"
            title: qsTrId("lautta-recents-title")
        }

        section.property: "dateGroup"
        section.criteria: ViewSection.FullString
        section.delegate: DateSectionHeader {
            group: section
        }

        PullDownMenu {
            visible: recents.count > 0

            MenuItem {
                //% "Clear recents"
                text: qsTrId("lautta-recents-clear")
                onClicked: {
                    //% "Clearing recents"
                    remorsePopup.execute(qsTrId("lautta-recents-clearing"), function() {
                        recents.clear()
                    }, App.setting("remorse_seconds") * 1000)
                }
            }
        }

        delegate: ListItem {
            id: row

            contentHeight: Theme.itemSizeMedium
            menu: Component {
                ContextMenu {
                    MenuItem {
                        //% "Remove from recents"
                        text: qsTrId("lautta-recents-remove")
                        onClicked: recents.remove(model.itemId)
                    }
                }
            }
            onClicked: page.openRecent(model)

            ItemIcon {
                anchors {
                    left: parent.left
                    leftMargin: Theme.horizontalPageMargin
                    verticalCenter: parent.verticalCenter
                }
                name: model.name
                highlightedItem: row.highlighted
            }
            Column {
                anchors {
                    left: parent.left
                    leftMargin: Theme.horizontalPageMargin + Theme.iconSizeMedium + Theme.paddingLarge
                    right: parent.right
                    rightMargin: Theme.horizontalPageMargin
                    verticalCenter: parent.verticalCenter
                }

                Label {
                    width: parent.width
                    text: model.name
                    color: row.highlighted ? Theme.highlightColor : Theme.primaryColor
                    truncationMode: TruncationMode.Fade
                }
                PlaceLabel {
                    width: parent.width
                    text: model.place
                    remote: App.isRemote(model.uri)
                    color: row.highlighted ? Theme.secondaryHighlightColor : Theme.secondaryColor
                }
            }
        }

        ViewPlaceholder {
            enabled: recents.loaded && recents.count === 0
            text: page.recentsOn
                  //% "No recents yet"
                  ? qsTrId("lautta-recents-empty")
                  //% "Recents are off"
                  : qsTrId("lautta-recents-off")
            hintText: page.recentsOn
                      //% "Files you open, edit or transfer show up here"
                      ? qsTrId("lautta-recents-empty-hint")
                      //% "Turn them on in Settings"
                      : qsTrId("lautta-recents-off-settings-hint")
        }

        VerticalScrollDecorator { }
    }

    RemorsePopup {
        id: remorsePopup
    }
}
