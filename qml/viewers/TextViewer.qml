// SPDX-License-Identifier: LGPL-2.1-or-later
import QtQuick 2.6
import Sailfish.Silica 1.0
import Lautta 1.0
import "../components"
import "../components/viewers"
import "../components/ErrorText.js" as ErrorText
import "../components/viewers/SyntaxColours.js" as SyntaxColours

Page {
    id: page

    property string uri

    readonly property string fileName: App.nameOf(uri)
    readonly property real charWidth: metrics.advanceWidth
    readonly property real gutter: Math.max(2, String(doc.lineCount).length) * charWidth + Theme.paddingLarge
    readonly property string colours: SyntaxColours.palette(Theme)

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

    Rectangle {
        objectName: "textBackdrop"
        anchors.fill: parent
        color: Theme.rgba(palette.overlayBackgroundColor, Theme.opacityHigh)
    }

    TextMetrics {
        id: metrics
        font.family: App.monoFont
        font.pixelSize: Theme.fontSizeExtraSmall
        text: "M"
    }

    SilicaListView {
        id: view

        anchors.fill: parent
        model: doc.rowCount
        clip: true
        flickableDirection: Flickable.VerticalFlick

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
        }

        header: PageHeader {
            width: view.width
            title: page.fileName
            description: doc.loaded
                         //% "Read-only · %1"
                         ? qsTrId("lautta-viewers-read-only").arg(Format.formatFileSize(doc.size))
                         : " "
        }

        delegate: Item {
            width: view.width
            height: Math.max(line.implicitHeight, Theme.fontSizeExtraSmall * 1.4)

            Label {
                x: Theme.paddingSmall
                width: page.gutter - Theme.paddingLarge
                horizontalAlignment: Text.AlignRight
                text: doc.rowNumber(index) || ""
                font.family: App.monoFont
                font.pixelSize: Theme.fontSizeExtraSmall
                color: Theme.secondaryColor
                opacity: Theme.opacityHigh
            }
            Label {
                id: line
                x: page.gutter
                width: parent.width - page.gutter - Theme.horizontalPageMargin
                text: doc.styledRow(index, doc.highlighted ? page.colours : "")
                font.family: App.monoFont
                font.pixelSize: Theme.fontSizeExtraSmall
                wrapMode: Text.WrapAtWordBoundaryOrAnywhere
                textFormat: Text.StyledText
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
                //% "Showing the first 1 MB of %1"
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
            text: ErrorText.message(doc.errorKind, { "item": page.fileName, "location": App.locationName(page.uri), "open": true })
        }

        VerticalScrollDecorator { }
    }

    BusyIndicator {
        anchors.centerIn: parent
        size: BusyIndicatorSize.Large
        running: doc.loading || external.busy
    }
}
