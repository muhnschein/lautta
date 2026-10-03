// SPDX-License-Identifier: LGPL-2.1-or-later
// Connect to server (NVB-6, bridge only): an address, a user name and a
// secret. The secret goes to the bridge once and is cleared from the field
// at once; Lautta stores nothing. `url` prefills the address (a nearby or
// recent server, LOC-6).
import QtQuick 2.6
import Sailfish.Silica 1.0
import Lautta 1.0

Dialog {
    id: dialog

    property string url

    allowedOrientations: Orientation.All
    canAccept: urlField.text.trim().length > 0

    onAccepted: {
        Bridge.connectAdHoc(urlField.text.trim(), passwordField.text,
                            JSON.stringify({ "user": userField.text.trim() }))
        passwordField.text = ""
    }
    Component.onDestruction: passwordField.text = ""

    LocationsModel {
        id: locations
        Component.onCompleted: refresh()
    }

    SilicaFlickable {
        anchors.fill: parent
        contentHeight: column.height + Theme.paddingLarge

        Column {
            id: column
            width: parent.width

            DialogHeader {
                //% "Connect"
                acceptText: qsTrId("lautta-connect-accept")
                //% "Connect to server"
                title: qsTrId("lautta-connect-title")
            }

            TextField {
                id: urlField
                width: parent.width
                //% "Server address"
                label: qsTrId("lautta-connect-address")
                placeholderText: label
                text: dialog.url
                inputMethodHints: Qt.ImhUrlCharactersOnly | Qt.ImhNoPredictiveText | Qt.ImhNoAutoUppercase
                focus: dialog.url.length === 0
                EnterKey.iconSource: "image://theme/icon-m-enter-next"
                EnterKey.onClicked: userField.focus = true
            }
            Label {
                x: Theme.horizontalPageMargin
                width: parent.width - 2 * x
                wrapMode: Text.Wrap
                //% "sftp://, smb://, davs://, ftps:// …"
                text: qsTrId("lautta-connect-schemes")
                font.pixelSize: Theme.fontSizeExtraSmall
                color: Theme.secondaryColor
            }
            TextField {
                id: userField
                width: parent.width
                //% "User name"
                label: qsTrId("lautta-connect-user")
                placeholderText: label
                inputMethodHints: Qt.ImhNoPredictiveText | Qt.ImhNoAutoUppercase
                EnterKey.iconSource: "image://theme/icon-m-enter-next"
                EnterKey.onClicked: passwordField.focus = true
            }
            PasswordField {
                id: passwordField
                width: parent.width
                //% "Password (optional)"
                label: qsTrId("lautta-connect-password")
                placeholderText: label
                EnterKey.iconSource: "image://theme/icon-m-enter-accept"
                EnterKey.onClicked: dialog.accept()
            }
            Label {
                x: Theme.horizontalPageMargin
                width: parent.width - 2 * x
                wrapMode: Text.Wrap
                //% "The password is passed to netvfs once and not stored by Lautta."
                text: qsTrId("lautta-connect-secret-note")
                font.pixelSize: Theme.fontSizeExtraSmall
                color: Theme.secondaryColor
            }

            SectionHeader {
                visible: locations.recentCount > 0
                //% "Recent"
                text: qsTrId("lautta-connect-recent")
            }
            Repeater {
                model: locations
                delegate: BackgroundItem {
                    visible: model.kind === "adhocRecent"
                    height: visible ? Theme.itemSizeMedium : 0
                    onClicked: urlField.text = model.uri

                    Column {
                        anchors {
                            left: parent.left
                            right: parent.right
                            margins: Theme.horizontalPageMargin
                            verticalCenter: parent.verticalCenter
                        }
                        Label {
                            width: parent.width
                            text: model.name
                            truncationMode: TruncationMode.Fade
                            color: highlighted ? Theme.highlightColor : Theme.primaryColor
                        }
                        Label {
                            width: parent.width
                            text: model.provider
                            font.pixelSize: Theme.fontSizeExtraSmall
                            color: Theme.secondaryColor
                            truncationMode: TruncationMode.Fade
                        }
                    }
                }
            }

            SectionHeader {
                visible: locations.nearbyCount > 0
                //% "Nearby"
                text: qsTrId("lautta-connect-nearby")
            }
            Repeater {
                model: locations
                delegate: BackgroundItem {
                    visible: model.kind === "nearby"
                    height: visible ? Theme.itemSizeMedium : 0
                    highlighted: down || urlField.text === model.uri
                    onClicked: urlField.text = model.uri

                    Column {
                        anchors {
                            left: parent.left
                            right: parent.right
                            margins: Theme.horizontalPageMargin
                            verticalCenter: parent.verticalCenter
                        }
                        Label {
                            width: parent.width
                            text: model.name
                            truncationMode: TruncationMode.Fade
                            color: highlighted ? Theme.highlightColor : Theme.primaryColor
                        }
                        Label {
                            width: parent.width
                            text: model.provider + " · " + model.host
                            font.pixelSize: Theme.fontSizeExtraSmall
                            color: Theme.secondaryColor
                            truncationMode: TruncationMode.Fade
                        }
                    }
                }
            }
        }

        VerticalScrollDecorator { }
    }
}
