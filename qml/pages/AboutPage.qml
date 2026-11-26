// SPDX-License-Identifier: LGPL-2.1-or-later
import QtQuick 2.6
import Sailfish.Silica 1.0
import Lautta 1.0
import "../components/FileIcons.js" as FileIcons

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

                    objectName: "aboutIcon"
                    anchors.centerIn: parent
                    width: Theme.iconSizeExtraLarge
                    height: width
                    sourceSize.width: width
                    sourceSize.height: height
                    source: FileIcons.source("lautta", Theme.pixelRatio)
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
                //% "A better file manager for Sailfish OS."
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

            SectionHeader {
                //% "Licence and source"
                text: qsTrId("lautta-about-licence-source")
            }
            DetailItem {
                //% "Licence"
                label: qsTrId("lautta-about-licence")
                //% "LGPL-2.1-or-later"
                value: qsTrId("lautta-about-licence-name")
            }
            BackgroundItem {
                objectName: "sourceCode"
                width: column.width
                height: source.height
                onClicked: Qt.openUrlExternally("https://github.com/muhnschein/lautta")

                DetailItem {
                    id: source

                    //% "Source code"
                    label: qsTrId("lautta-about-source")
                    //% "On GitHub"
                    value: qsTrId("lautta-about-source-github")
                }
            }
        }

        VerticalScrollDecorator { }
    }
}
