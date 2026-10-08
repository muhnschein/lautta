// SPDX-License-Identifier: LGPL-2.1-or-later
// Text and code viewer (PRV-4, boards TextViewer, TextViewerPulley): read-only
// up to 1 MiB with line numbers.
import QtQuick 2.6
import Sailfish.Silica 1.0
import Lautta 1.0
import "../components"
import "../components/viewers"
import "../components/ErrorText.js" as ErrorText

Page {
    id: page

    property string uri
    property bool wrap: false

    readonly property string fileName: App.nameOf(uri)
    readonly property var lines: doc.loaded ? doc.text.split("\n") : []
    readonly property int maxChars: {
        var most = 0
        for (var i = 0; i < lines.length; ++i)
            most = Math.max(most, lines[i].length)
        return most
    }
    readonly property real charWidth: metrics.advanceWidth
    readonly property real gutter: Math.max(2, String(lines.length).length) * charWidth + Theme.paddingLarge

    allowedOrientations: Orientation.All

    function say(kind, message) {
        notice.text = ErrorText.message(kind, { "item": fileName, "location": App.locationName(uri) })
        notice.show()
    }

    TextDocument {
        id: doc
        uri: page.uri
    }

    ExternalActions {
        id: external
        uri: page.uri
        onFailed: page.say(kind, message)
    }

    Notice { id: notice }

    TextMetrics {
        id: metrics
        font.family: "monospace"
        font.pixelSize: Theme.fontSizeExtraSmall
        text: "M"
    }

    SilicaListView {
        id: view

        anchors.fill: parent
        model: page.lines
        clip: true
        flickableDirection: page.wrap ? Flickable.VerticalFlick : Flickable.HorizontalAndVerticalFlick
        contentWidth: page.wrap ? width
                                : Math.max(width, page.gutter + page.maxChars * page.charWidth + 2 * Theme.horizontalPageMargin)

        PullDownMenu {
            MenuItem {
                //% "Open with"
                text: qsTrId("lautta-viewers-open-with")
                onClicked: external.openWith()
            }
            MenuItem {
                //% "Share"
                text: qsTrId("lautta-viewers-share")
                onClicked: external.share()
            }
            MenuItem {
                text: page.wrap
                      //% "Don't wrap lines"
                      ? qsTrId("lautta-viewers-unwrap")
                      //% "Wrap lines"
                      : qsTrId("lautta-viewers-wrap")
                onClicked: page.wrap = !page.wrap
            }
        }

        header: PageHeader {
            width: view.width
            title: page.fileName
            description: doc.loaded
                         //% "Read-only · %1"
                         ? qsTrId("lautta-viewers-read-only").arg(Format.formatFileSize(doc.size))
                         : ""
        }

        delegate: Item {
            width: page.wrap ? view.width : view.contentWidth
            height: Math.max(line.implicitHeight, Theme.fontSizeExtraSmall * 1.4)

            Label {
                id: number
                x: Theme.paddingSmall
                width: page.gutter - Theme.paddingLarge
                horizontalAlignment: Text.AlignRight
                text: index + 1
                font.family: "monospace"
                font.pixelSize: Theme.fontSizeExtraSmall
                color: Theme.secondaryColor
                opacity: Theme.opacityHigh
            }
            Label {
                id: line
                x: page.gutter
                width: page.wrap ? view.width - page.gutter - Theme.horizontalPageMargin : implicitWidth
                text: modelData
                font.family: "monospace"
                font.pixelSize: Theme.fontSizeExtraSmall
                wrapMode: page.wrap ? Text.WrapAnywhere : Text.NoWrap
                textFormat: Text.PlainText
            }
        }

        footer: Column {
            width: view.width
            visible: doc.loaded && (doc.truncated || !doc.validUtf8)
            spacing: Theme.paddingMedium

            Label {
                x: Theme.horizontalPageMargin
                width: parent.width - 2 * Theme.horizontalPageMargin
                visible: doc.truncated
                wrapMode: Text.Wrap
                color: Theme.highlightColor
                font.pixelSize: Theme.fontSizeSmall
                //% "Showing the first 1 MiB of %1"
                text: qsTrId("lautta-viewers-truncated").arg(Format.formatFileSize(doc.size))
            }
            Label {
                x: Theme.horizontalPageMargin
                width: parent.width - 2 * Theme.horizontalPageMargin
                visible: !doc.validUtf8
                wrapMode: Text.Wrap
                color: Theme.secondaryHighlightColor
                font.pixelSize: Theme.fontSizeSmall
                //% "This file isn't valid UTF-8. Some characters are replaced."
                text: qsTrId("lautta-viewers-not-utf8")
            }
            Button {
                anchors.horizontalCenter: parent.horizontalCenter
                //% "Open with"
                text: qsTrId("lautta-viewers-open-with")
                onClicked: external.openWith()
            }
        }

        ViewPlaceholder {
            enabled: doc.errorKind !== ""
            text: ErrorText.message(doc.errorKind, { "item": page.fileName, "location": App.locationName(page.uri) })
        }

        VerticalScrollDecorator { }
        HorizontalScrollDecorator { }
    }

    BusyIndicator {
        anchors.centerIn: parent
        size: BusyIndicatorSize.Large
        running: doc.loading || external.busy
    }
}
