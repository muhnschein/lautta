// SPDX-License-Identifier: LGPL-2.1-or-later
// Settings of one location: display name, start folder and, for servers,
// bulk lanes, caches and the hand-offs to netvfs' account settings (NVB-5);
// for volumes the hand-off to the platform's Storage settings (LOC-3).
// Changes are saved when the page is left.
import QtQuick 2.6
import Sailfish.Silica 1.0
import Lautta 1.0

Page {
    id: page

    property string locationId
    property bool dirty

    readonly property bool remote: prefs.kind === "account" || prefs.kind === "adhoc"

    allowedOrientations: Orientation.All

    onStatusChanged: {
        if (status === PageStatus.Deactivating && dirty) {
            dirty = false
            prefs.save()
        }
    }

    LocationPrefsModel {
        id: prefs
        locationId: page.locationId
    }

    SilicaFlickable {
        anchors.fill: parent
        contentHeight: column.height + Theme.paddingLarge

        Column {
            id: column
            width: parent.width

            PageHeader {
                title: prefs.locationName
                //% "Location settings"
                description: qsTrId("lautta-locset-title")
            }

            TextField {
                id: nameField
                width: parent.width
                //% "Display name"
                label: qsTrId("lautta-locset-name")
                placeholderText: prefs.locationName
                text: prefs.displayName
                onTextChanged: {
                    if (activeFocus) {
                        prefs.displayName = text
                        page.dirty = true
                    }
                }
                EnterKey.iconSource: "image://theme/icon-m-enter-close"
                EnterKey.onClicked: focus = false
            }

            ListItem {
                id: startRow
                contentHeight: Theme.itemSizeMedium
                menu: Component {
                    ContextMenu {
                        MenuItem {
                            visible: prefs.startFolder.length > 0
                            //% "Use the default start folder"
                            text: qsTrId("lautta-locset-start-default")
                            onClicked: {
                                prefs.startFolder = ""
                                page.dirty = true
                            }
                        }
                    }
                }
                onClicked: {
                    var dialog = pageStack.push(Qt.resolvedUrl("../dialogs/FolderPickerDialog.qml"), {
                                                    //% "Start folder"
                                                    "title": qsTrId("lautta-locset-start"),
                                                    //% "Select"
                                                    "acceptText": qsTrId("lautta-locset-start-select"),
                                                    "startUri": prefs.startFolder.length > 0
                                                                ? prefs.startFolder : "lautta://" + page.locationId + "/"
                                                })
                    dialog.accepted.connect(function() {
                        prefs.startFolder = dialog.selectedUri
                        page.dirty = true
                    })
                }

                Column {
                    anchors {
                        left: parent.left
                        right: parent.right
                        margins: Theme.horizontalPageMargin
                        verticalCenter: parent.verticalCenter
                    }
                    Label {
                        width: parent.width
                        //% "Start folder"
                        text: qsTrId("lautta-locset-start")
                        color: startRow.highlighted ? Theme.highlightColor : Theme.primaryColor
                    }
                    Label {
                        width: parent.width
                        text: prefs.startFolder.length > 0
                              ? App.displayAddress(prefs.startFolder)
                              //% "Default"
                              : qsTrId("lautta-locset-start-none")
                        font.pixelSize: Theme.fontSizeExtraSmall
                        color: Theme.secondaryColor
                        truncationMode: TruncationMode.Fade
                    }
                }
            }

            Slider {
                visible: page.remote
                width: parent.width
                minimumValue: 1
                maximumValue: 6
                stepSize: 1
                value: prefs.bulkLanes > 0 ? prefs.bulkLanes : (App.setting("remote_lanes") || 2)
                valueText: value
                //% "Bulk lane"
                label: qsTrId("lautta-locset-lanes")
                onReleased: {
                    prefs.bulkLanes = value
                    page.dirty = true
                }
            }

            TextSwitch {
                visible: page.remote
                //% "Cache folder listings"
                text: qsTrId("lautta-locset-cache-listings")
                checked: !prefs.noListingCache
                onClicked: {
                    prefs.noListingCache = !checked
                    page.dirty = true
                }
            }
            TextSwitch {
                visible: page.remote
                //% "Cache thumbnails"
                text: qsTrId("lautta-locset-cache-thumbs")
                checked: !prefs.noThumbCache
                onClicked: {
                    prefs.noThumbCache = !checked
                    page.dirty = true
                }
            }

            SectionHeader {
                visible: prefs.kind === "account"
                //% "Account"
                text: qsTrId("lautta-locset-account")
            }
            Repeater {
                model: prefs.kind === "account" ? 3 : 0
                delegate: ListItem {
                    contentHeight: Theme.itemSizeMedium
                    onClicked: Bridge.editAccount("lautta://" + page.locationId + "/")

                    Column {
                        anchors {
                            left: parent.left
                            right: parent.right
                            margins: Theme.horizontalPageMargin
                            verticalCenter: parent.verticalCenter
                        }
                        Label {
                            width: parent.width
                            text: index === 0
                                  //% "Edit account"
                                  ? qsTrId("lautta-locset-edit-account")
                                  : (index === 1
                                     //% "Update sign-in"
                                     ? qsTrId("lautta-locset-update-sign-in")
                                     //% "Review server identity"
                                     : qsTrId("lautta-locset-review-identity"))
                            color: highlighted ? Theme.highlightColor : Theme.primaryColor
                        }
                        Label {
                            width: parent.width
                            //% "Opens Settings"
                            text: qsTrId("lautta-locset-opens-settings")
                            font.pixelSize: Theme.fontSizeExtraSmall
                            color: Theme.secondaryColor
                        }
                    }
                }
            }

            SectionHeader {
                visible: prefs.kind === "volume"
                //% "Storage"
                text: qsTrId("lautta-locset-storage")
            }
            ListItem {
                visible: prefs.kind === "volume"
                contentHeight: Theme.itemSizeMedium
                // The hand-off to the platform's Storage page is verified on device (SPEC §24.1).
                onClicked: Qt.openUrlExternally("settings://system/storage")

                Column {
                    anchors {
                        left: parent.left
                        right: parent.right
                        margins: Theme.horizontalPageMargin
                        verticalCenter: parent.verticalCenter
                    }
                    Label {
                        width: parent.width
                        //% "Open Storage settings"
                        text: qsTrId("lautta-browse-open-storage-settings")
                        color: highlighted ? Theme.highlightColor : Theme.primaryColor
                    }
                    Label {
                        width: parent.width
                        //% "Mount, unmount and format are in Settings"
                        text: qsTrId("lautta-locset-storage-hint")
                        font.pixelSize: Theme.fontSizeExtraSmall
                        color: Theme.secondaryColor
                        wrapMode: Text.Wrap
                    }
                }
            }

            Button {
                visible: page.remote
                anchors.horizontalCenter: parent.horizontalCenter
                //% "Clear this location's cache"
                text: qsTrId("lautta-locset-clear-cache")
                onClicked: prefs.clearCache()
            }
        }

        VerticalScrollDecorator { }
    }
}
