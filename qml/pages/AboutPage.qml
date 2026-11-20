// SPDX-License-Identifier: LGPL-2.1-or-later
import QtQuick 2.6
import Sailfish.Silica 1.0
import Lautta 1.0

Page {
    id: page

    readonly property string bridgeStatus: typeof Bridge !== "undefined" ? String(Bridge.status) : "absent"
    readonly property bool hasBridge: bridgeStatus !== "absent" && bridgeStatus !== "undefined"

    function statusText() {
        switch (bridgeStatus) {
        case "ready":
            //% "Connected to netvfs bridge"
            return qsTrId("lautta-about-status-ready")
        case "connecting":
            //% "Connecting to netvfs…"
            return qsTrId("lautta-about-status-connecting")
        case "reconnecting":
            //% "Reconnecting to netvfs…"
            return qsTrId("lautta-about-status-reconnecting")
        case "consentUnknown":
            //% "Waiting for your permission"
            return qsTrId("lautta-about-status-consent-unknown")
        default:
            return ""
        }
    }

    function accessText() {
        switch (bridgeStatus) {
        case "consentUnknown":
            //% "Not asked yet"
            return qsTrId("lautta-about-access-unknown")
        case "connecting":
            return ""
        default:
            //% "Set per server"
            return qsTrId("lautta-about-access-per-server")
        }
    }

    allowedOrientations: Orientation.All

    SilicaFlickable {
        anchors.fill: parent
        contentHeight: column.height + Theme.paddingLarge

        Column {
            id: column

            width: parent.width

            PageHeader {
                //% "About"
                title: qsTrId("lautta-about-title")
            }

            Item {
                width: parent.width
                height: logo.height + Theme.paddingLarge * 2

                HighlightImage {
                    id: logo

                    anchors.centerIn: parent
                    source: "image://theme/icon-l-storage"
                    color: Theme.highlightColor
                }
            }

            Label {
                anchors.horizontalCenter: parent.horizontalCenter
                //% "Lautta"
                text: qsTrId("lautta-app-name")
                font.pixelSize: Theme.fontSizeExtraLarge
            }

            Label {
                anchors.horizontalCenter: parent.horizontalCenter
                //% "Version %1"
                text: qsTrId("lautta-about-version").arg(App.version)
                color: Theme.secondaryColor
                font.pixelSize: Theme.fontSizeSmall
            }

            Label {
                x: Theme.horizontalPageMargin * 2
                width: parent.width - 4 * Theme.horizontalPageMargin
                topPadding: Theme.paddingLarge
                bottomPadding: Theme.paddingLarge
                horizontalAlignment: Text.AlignHCenter
                wrapMode: Text.Wrap
                //% "A file manager for Sailfish OS."
                text: qsTrId("lautta-about-tagline")
            }

            SectionHeader {
                visible: page.hasBridge
                //% "Network locations"
                text: qsTrId("lautta-about-network")
            }
            DetailItem {
                visible: page.hasBridge
                //% "Status"
                label: qsTrId("lautta-about-status")
                value: page.statusText()
            }
            DetailItem {
                visible: page.hasBridge && value.length > 0
                //% "Access"
                label: qsTrId("lautta-about-access")
                value: page.accessText()
            }
            Label {
                visible: page.hasBridge
                x: Theme.horizontalPageMargin
                width: parent.width - 2 * Theme.horizontalPageMargin
                horizontalAlignment: Text.AlignRight
                wrapMode: Text.Wrap
                color: Theme.secondaryColor
                font.pixelSize: Theme.fontSizeSmall
                //% "Change in Settings › Accounts › Apps using network locations › Lautta. Servers you add in Lautta always have full access."
                text: qsTrId("lautta-about-access-hint")
            }

            SectionHeader {
                //% "Permissions"
                text: qsTrId("lautta-about-permissions")
            }
            Label {
                x: Theme.horizontalPageMargin
                width: parent.width - 2 * Theme.horizontalPageMargin
                horizontalAlignment: Text.AlignRight
                wrapMode: Text.Wrap
                //% "Your folders, Removable media, Audio"
                text: qsTrId("lautta-about-permission-list")
            }
            Label {
                x: Theme.horizontalPageMargin
                width: parent.width - 2 * Theme.horizontalPageMargin
                horizontalAlignment: Text.AlignRight
                wrapMode: Text.Wrap
                color: Theme.secondaryColor
                font.pixelSize: Theme.fontSizeSmall
                //% "Lautta never opens network connections itself."
                text: qsTrId("lautta-about-no-network")
            }

            SectionHeader {
                //% "Licence"
                text: qsTrId("lautta-about-licence")
            }
            Label {
                x: Theme.horizontalPageMargin
                width: parent.width - 2 * Theme.horizontalPageMargin
                horizontalAlignment: Text.AlignRight
                font.pixelSize: Theme.fontSizeSmall
                //% "LGPL-2.1-or-later"
                text: qsTrId("lautta-about-licence-name")
            }

            BackgroundItem {
                width: column.width
                height: Theme.itemSizeSmall
                onClicked: Qt.openUrlExternally("https://github.com/muhnschein/lautta")

                Label {
                    anchors {
                        left: parent.left
                        right: parent.right
                        leftMargin: Theme.horizontalPageMargin
                        rightMargin: Theme.horizontalPageMargin
                        verticalCenter: parent.verticalCenter
                    }
                    horizontalAlignment: Text.AlignRight
                    //% "Source code"
                    text: qsTrId("lautta-about-source")
                    color: Theme.highlightColor
                }
            }
        }

        VerticalScrollDecorator { }
    }
}
