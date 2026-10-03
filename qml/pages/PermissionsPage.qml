// SPDX-License-Identifier: LGPL-2.1-or-later
// Permissions editor (OPS-12): rwx grid, octal, and for folders a recursive
// apply with separate masks for files and folders.
import QtQuick 2.6
import Sailfish.Silica 1.0
import Lautta 1.0
import "../components/operations"

Dialog {
    id: page

    property string uri

    allowedOrientations: Orientation.All
    canAccept: perms.canApply
    onAccepted: perms.apply()

    function unavailableText() {
        if (perms.fsType.length > 0)
            //% "Not available on %1 (%2)."
            return qsTrId("lautta-perm-unavailable-fs").arg(perms.locationName).arg(perms.fsType)
        //% "Not available on %1."
        return qsTrId("lautta-perm-unavailable").arg(perms.locationName)
    }

    PermissionsModel {
        id: perms
        uri: page.uri
    }

    SilicaFlickable {
        anchors.fill: parent
        contentHeight: column.height + Theme.paddingLarge

        Column {
            id: column
            width: parent.width

            DialogHeader {
                //% "Save"
                acceptText: qsTrId("lautta-perm-save")
            }
            DialogSubtitle {
                //% "Permissions · %1"
                text: qsTrId("lautta-perm-subtitle").arg(App.nameOf(page.uri))
            }

            Item {
                width: parent.width
                height: Theme.itemSizeSmall
                visible: perms.loaded

                Row {
                    anchors {
                        right: parent.right
                        rightMargin: Theme.horizontalPageMargin
                        verticalCenter: parent.verticalCenter
                    }
                    Repeater {
                        model: [
                            //% "Read"
                            qsTrId("lautta-perm-read"),
                            //% "Write"
                            qsTrId("lautta-perm-write"),
                            //% "Execute"
                            qsTrId("lautta-perm-execute")
                        ]
                        Label {
                            width: gridCell
                            horizontalAlignment: Text.AlignHCenter
                            color: Theme.secondaryColor
                            font.pixelSize: Theme.fontSizeExtraSmall
                            text: modelData
                            truncationMode: TruncationMode.Fade
                        }
                    }
                }
            }

            Repeater {
                model: [
                    //% "Owner"
                    qsTrId("lautta-perm-owner"),
                    //% "Group"
                    qsTrId("lautta-perm-group"),
                    //% "Others"
                    qsTrId("lautta-perm-others")
                ]

                Item {
                    property int who: index

                    width: column.width
                    height: Theme.itemSizeSmall
                    visible: perms.loaded
                    opacity: perms.supported ? 1.0 : Theme.opacityLow

                    Label {
                        anchors {
                            left: parent.left
                            leftMargin: Theme.horizontalPageMargin
                            right: switches.left
                            verticalCenter: parent.verticalCenter
                        }
                        horizontalAlignment: Text.AlignRight
                        text: modelData
                        truncationMode: TruncationMode.Fade
                    }
                    Row {
                        id: switches
                        anchors {
                            right: parent.right
                            rightMargin: Theme.horizontalPageMargin
                            verticalCenter: parent.verticalCenter
                        }
                        Repeater {
                            model: 3

                            Item {
                                width: gridCell
                                height: Theme.itemSizeSmall

                                Switch {
                                    anchors.centerIn: parent
                                    enabled: perms.supported
                                    automaticCheck: false
                                    checked: perms.mode >= 0 && perms.bit(who, index)
                                    onClicked: perms.setBit(who, index, !checked)
                                }
                            }
                        }
                    }
                }
            }

            TextField {
                width: parent.width
                visible: perms.loaded
                enabled: perms.supported
                //% "Octal"
                label: qsTrId("lautta-perm-octal")
                placeholderText: label
                font.family: "monospace"
                text: perms.octal
                inputMethodHints: Qt.ImhDigitsOnly
                validator: RegExpValidator { regExp: /^[0-7]{0,4}$/ }
                onTextChanged: if (activeFocus) perms.octal = text
                EnterKey.iconSource: "image://theme/icon-m-enter-close"
                EnterKey.onClicked: focus = false
            }

            SectionHeader {
                visible: perms.isDir && perms.supported
                //% "Apply to contents"
                text: qsTrId("lautta-perm-contents")
            }
            TextSwitch {
                visible: perms.isDir && perms.supported
                checked: perms.recursive
                //% "Apply recursively"
                text: qsTrId("lautta-perm-recursive")
                onCheckedChanged: perms.recursive = checked
            }
            TextField {
                width: parent.width
                visible: perms.isDir && perms.supported && perms.recursive
                //% "Files"
                label: qsTrId("lautta-perm-files")
                placeholderText: label
                font.family: "monospace"
                text: perms.filesOctal
                inputMethodHints: Qt.ImhDigitsOnly
                validator: RegExpValidator { regExp: /^[0-7]{0,4}$/ }
                onTextChanged: if (activeFocus) perms.filesOctal = text
                EnterKey.iconSource: "image://theme/icon-m-enter-close"
                EnterKey.onClicked: focus = false
            }
            TextField {
                width: parent.width
                visible: perms.isDir && perms.supported && perms.recursive
                //% "Folders"
                label: qsTrId("lautta-perm-folders")
                placeholderText: label
                font.family: "monospace"
                text: perms.foldersOctal
                inputMethodHints: Qt.ImhDigitsOnly
                validator: RegExpValidator { regExp: /^[0-7]{0,4}$/ }
                onTextChanged: if (activeFocus) perms.foldersOctal = text
                EnterKey.iconSource: "image://theme/icon-m-enter-close"
                EnterKey.onClicked: focus = false
            }
            Label {
                x: Theme.horizontalPageMargin
                width: parent.width - 2 * Theme.horizontalPageMargin
                visible: perms.loaded && !perms.supported
                horizontalAlignment: Text.AlignRight
                wrapMode: Text.Wrap
                color: Theme.secondaryColor
                font.pixelSize: Theme.fontSizeSmall
                text: page.unavailableText()
            }
        }
        VerticalScrollDecorator { }
    }

    readonly property real gridCell: Theme.itemSizeSmall * 1.3
}
