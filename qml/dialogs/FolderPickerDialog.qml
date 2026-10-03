// SPDX-License-Identifier: LGPL-2.1-or-later
// Folder picker across all locations (SPEC OPS-10, boards FolderPicker and
// FolderPickerPulley): the current folder on top, the locations, then the
// sub-folders of the current folder. After accept, `selectedUri` is the
// chosen folder. `startUri` is where to begin (a location root by default).
import QtQuick 2.6
import Sailfish.Silica 1.0
import Lautta 1.0
import "../components"
import "../components/ErrorText.js" as ErrorText

Dialog {
    id: dialog

    property string title
    property string acceptText
    property string startUri
    // Without write access in the current folder, "accept" stays off.
    property bool requireWrite: true
    property string currentUri: startUri
    readonly property string selectedUri: currentUri

    allowedOrientations: Orientation.All
    canAccept: currentUri.length > 0 && !folders.loading && folders.errorKind === ""
               && (!requireWrite || folders.writable)

    PickerRootsModel {
        id: roots
    }

    DirectoryModel {
        id: folders

        uri: dialog.currentUri
        chips: JSON.stringify(["folders"])
    }

    PathModel {
        id: where

        uri: dialog.currentUri
    }

    SilicaListView {
        id: list

        anchors.fill: parent
        model: folders

        PullDownMenu {
            MenuItem {
                visible: dialog.currentUri.length > 0 && folders.writable
                //% "New folder"
                text: qsTrId("lautta-dir-picker-new-folder")
                onClicked: {
                    var d = pageStack.push(Qt.resolvedUrl("NewItemDialog.qml"),
                                           { "parentUri": dialog.currentUri, "foldersOnly": true })
                    d.accepted.connect(function () { folders.createFolder(d.itemName) })
                }
            }
        }

        header: Column {
            width: list.width

            DialogHeader {
                acceptText: dialog.acceptText
                title: dialog.title
            }

            // The current folder; tapping it goes one level up.
            BackgroundItem {
                width: parent.width
                height: Theme.itemSizeSmall
                enabled: dialog.currentUri.length > 0 && where.count > 1
                highlightedColor: Theme.rgba(Theme.highlightBackgroundColor, Theme.highlightBackgroundOpacity)
                onClicked: dialog.currentUri = App.parentUri(dialog.currentUri)

                Rectangle {
                    anchors.fill: parent
                    color: Theme.rgba(Theme.highlightBackgroundColor, Theme.highlightBackgroundOpacity / 2)
                }
                Label {
                    anchors {
                        left: parent.left
                        right: parent.right
                        margins: Theme.horizontalPageMargin
                        verticalCenter: parent.verticalCenter
                    }
                    text: dialog.currentUri.length > 0 ? where.fullPath : ""
                    color: Theme.highlightColor
                    truncationMode: TruncationMode.Fade
                    horizontalAlignment: Text.AlignRight
                }
            }

            SectionHeader {
                //% "Locations"
                text: qsTrId("lautta-dir-picker-locations")
            }

            Repeater {
                model: roots

                BackgroundItem {
                    width: list.width
                    height: Theme.itemSizeSmall
                    enabled: model.selectable
                    opacity: enabled ? 1 : Theme.opacityLow
                    highlighted: down || dialog.currentUri.indexOf(model.uri) === 0
                    onClicked: dialog.currentUri = model.uri

                    Row {
                        anchors {
                            left: parent.left
                            right: parent.right
                            margins: Theme.horizontalPageMargin
                            verticalCenter: parent.verticalCenter
                        }
                        spacing: Theme.paddingMedium

                        Column {
                            width: parent.width
                            anchors.verticalCenter: parent.verticalCenter

                            Label {
                                width: parent.width
                                text: model.name
                                truncationMode: TruncationMode.Fade
                                color: highlighted ? Theme.highlightColor : Theme.primaryColor
                            }
                            Label {
                                visible: model.status !== "ready"
                                width: parent.width
                                text: model.attention.length > 0 ? model.attention : ErrorText.message("NetworkUnreachable", { "location": model.name })
                                font.pixelSize: Theme.fontSizeExtraSmall
                                color: Theme.secondaryColor
                                truncationMode: TruncationMode.Fade
                            }
                        }
                    }
                }
            }

            SectionHeader {
                visible: dialog.currentUri.length > 0
                //% "In %1"
                text: qsTrId("lautta-dir-picker-in").arg(folders.title)
            }
        }

        delegate: BackgroundItem {
            width: list.width
            height: Theme.itemSizeSmall
            onClicked: dialog.currentUri = model.uri

            Row {
                anchors {
                    left: parent.left
                    right: parent.right
                    margins: Theme.horizontalPageMargin
                    verticalCenter: parent.verticalCenter
                }
                spacing: Theme.paddingMedium

                FileIcon {
                    anchors.verticalCenter: parent.verticalCenter
                    isDir: true
                    category: "folder"
                    isSymlink: model.isSymlink
                }
                Label {
                    anchors.verticalCenter: parent.verticalCenter
                    width: parent.width - Theme.iconSizeMedium - Theme.paddingMedium
                    text: model.name
                    truncationMode: TruncationMode.Fade
                    color: parent.parent.highlighted ? Theme.highlightColor : Theme.primaryColor
                }
            }
        }

        footer: Item {
            width: list.width
            height: Theme.paddingLarge
        }

        ViewPlaceholder {
            enabled: dialog.currentUri.length > 0 && !folders.loading && folders.count === 0
                     && folders.errorKind === ""
            //% "No folders"
            text: qsTrId("lautta-dir-picker-empty")
            //% "You can use this folder or pull down to make one"
            hintText: qsTrId("lautta-dir-picker-empty-hint")
        }

        VerticalScrollDecorator { }
    }
}
