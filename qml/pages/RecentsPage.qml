// SPDX-License-Identifier: LGPL-2.1-or-later
// Recents (ORG-2): files opened, previewed or transferred, newest
// first. Filterable by kind, clearable, and can be switched off.
import QtQuick 2.6
import Sailfish.Silica 1.0
import Lautta 1.0
import "../components/Open.js" as Open
import "../components/browse"
import "../components/browse/BrowseText.js" as BrowseText

Page {
    id: page

    // The rows; tests give the page a ListModel.
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

        header: Column {
            width: list.width

            PageHeader {
                //% "Recents"
                title: qsTrId("lautta-recents-title")
                //% "%n items"
                description: qsTrId("lautta-recents-count", recents.count)
            }
            ComboBox {
                id: filterBox
                visible: page.recentsOn
                //% "Show"
                label: qsTrId("lautta-recents-show")
                currentIndex: kinds.indexOf(recents.kindFilter)
                readonly property var kinds: ["", "opened", "previewed", "transferred"]

                menu: ContextMenu {
                    Repeater {
                        model: filterBox.kinds
                        delegate: MenuItem {
                            text: BrowseText.recentFilterTitle(modelData)
                            onClicked: recents.kindFilter = modelData
                        }
                    }
                }
            }
        }

        section.property: "day"
        section.criteria: ViewSection.FullString
        section.delegate: SectionHeader {
            text: BrowseText.recentDayTitle(section)
        }

        PullDownMenu {
            MenuItem {
                text: page.recentsOn
                      //% "Turn off recents"
                      ? qsTrId("lautta-recents-turn-off")
                      //% "Turn on recents"
                      : qsTrId("lautta-recents-turn-on")
                onClicked: App.setSetting("recents_enabled", JSON.stringify(!page.recentsOn))
            }
            MenuItem {
                visible: recents.count > 0
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
                Label {
                    width: parent.width
                    text: BrowseText.recentKindLine(model.kind, model.place)
                    font.pixelSize: Theme.fontSizeExtraSmall
                    color: Theme.secondaryColor
                    truncationMode: TruncationMode.Fade
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
                      //% "Pull down to turn them on"
                      : qsTrId("lautta-recents-off-hint")
        }

        VerticalScrollDecorator { }
    }

    RemorsePopup {
        id: remorsePopup
    }
}
