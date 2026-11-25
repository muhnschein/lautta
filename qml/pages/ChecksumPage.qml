// SPDX-License-Identifier: LGPL-2.1-or-later
import QtQuick 2.6
import Sailfish.Silica 1.0
import Lautta 1.0
import "../components"
import "../components/ErrorText.js" as ErrorText

Page {
    id: page

    property string uri
    property string algorithm
    readonly property alias model: sum

    allowedOrientations: Orientation.All

    ChecksumModel {
        id: sum

        uri: page.uri
        algorithm: page.algorithm
        Component.onCompleted: start()
    }

    SilicaFlickable {
        anchors.fill: parent
        contentHeight: column.height + Theme.paddingLarge

        Column {
            id: column

            width: parent.width

            HeaderBand {
                PageHeader {
                    title: App.nameOf(page.uri)
                }
            }

            BackgroundItem {
                id: valueItem

                objectName: "checksumValue"
                width: parent.width
                height: valueColumn.height + 2 * Theme.paddingMedium
                enabled: sum.hex.length > 0
                onClicked: {
                    Clipboard.text = sum.hex
                    //% "Copied"
                    notice.text = qsTrId("lautta-checksum-copied")
                    notice.show()
                }

                Column {
                    id: valueColumn

                    x: Theme.horizontalPageMargin
                    y: Theme.paddingMedium
                    width: parent.width - 2 * Theme.horizontalPageMargin
                    spacing: Theme.paddingSmall

                    Label {
                        text: sum.label
                        font.pixelSize: Theme.fontSizeSmall
                        color: valueItem.highlighted ? Theme.secondaryHighlightColor : Theme.secondaryColor
                    }

                    ProgressBar {
                        objectName: "checksumProgress"
                        visible: sum.running
                        width: parent.width + 2 * Theme.horizontalPageMargin
                        x: -Theme.horizontalPageMargin
                        leftMargin: Theme.horizontalPageMargin
                        rightMargin: Theme.horizontalPageMargin
                        indeterminate: sum.progress < 0
                        value: Math.max(0, sum.progress)
                    }

                    Label {
                        objectName: "checksumHex"
                        visible: text.length > 0
                        width: parent.width
                        text: sum.hex
                        font.family: "monospace"
                        font.pixelSize: Theme.fontSizeSmall
                        wrapMode: Text.WrapAnywhere
                        color: valueItem.highlighted ? Theme.highlightColor : Theme.primaryColor
                    }

                    Row {
                        objectName: "checksumVerdict"
                        visible: sum.verdict.length > 0
                        spacing: Theme.paddingSmall

                        HighlightImage {
                            anchors.verticalCenter: parent.verticalCenter
                            source: sum.verdict === "matches" ? "image://theme/icon-s-accept" : "image://theme/icon-s-warning"
                            color: verdictLabel.color
                        }

                        Label {
                            id: verdictLabel

                            text: sum.verdict === "matches"
                                  //% "Matches %1"
                                  ? qsTrId("lautta-checksum-matches").arg(sum.verdictList)
                                  //% "Doesn't match %1"
                                  : qsTrId("lautta-checksum-differs").arg(sum.verdictList)
                            font.pixelSize: Theme.fontSizeSmall
                            color: sum.verdict === "matches" ? Theme.highlightColor : Theme.errorColor
                        }
                    }

                    Label {
                        objectName: "checksumError"
                        visible: sum.errorKind.length > 0 && sum.errorKind !== "Canceled"
                        width: parent.width
                        text: visible ? ErrorText.message(sum.errorKind, { "item": App.nameOf(page.uri), "location": App.locationName(page.uri), "open": true }) : ""
                        font.pixelSize: Theme.fontSizeSmall
                        wrapMode: Text.Wrap
                        color: Theme.errorColor
                    }
                }
            }
        }

        VerticalScrollDecorator { }
    }

    Notice {
        id: notice
    }
}
