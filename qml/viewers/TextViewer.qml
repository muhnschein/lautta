// SPDX-License-Identifier: LGPL-2.1-or-later
import QtQuick 2.6
import Sailfish.Silica 1.0
import Lautta 1.0
import "../components"
import "../components/viewers"
import "../components/ErrorText.js" as ErrorText
import "../components/viewers/SyntaxColours.js" as SyntaxColours
import "../components/viewers/TextRows.js" as TextRows

Page {
    id: page

    property string uri

    readonly property string fileName: App.nameOf(uri)
    readonly property var layout: doc.loaded ? TextRows.rows(doc.text) : TextRows.empty
    readonly property real charWidth: metrics.advanceWidth
    readonly property real gutter: Math.max(2, String(layout.lines).length) * charWidth + Theme.paddingLarge
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

    // Stronger than Page.highContrast; opaque enough for text over any wallpaper.
    Rectangle {
        objectName: "textBackdrop"
        anchors.fill: parent
        color: Theme.rgba(palette.overlayBackgroundColor, Theme.opacityHigh)
    }

    TextMetrics {
        id: metrics
        font.family: "monospace"
        font.pixelSize: Theme.fontSizeExtraSmall
        text: "M"
    }

    SilicaListView {
        id: view

        anchors.fill: parent
        model: page.layout.rows
        clip: true
        // Never sideways: sideways-flickable list keeps swipe-back from page stack.
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
            // List at top when text arrives; ListView keeps contentY when header grows, so
            // description line is there, blank, from the start.
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
                text: modelData.number > 0 ? modelData.number : ""
                font.family: "monospace"
                font.pixelSize: Theme.fontSizeExtraSmall
                color: Theme.secondaryColor
                opacity: Theme.opacityHigh
            }
            Label {
                id: line
                x: page.gutter
                // Never implicitWidth: wrapping makes it depend on width again (binding loop).
                width: parent.width - page.gutter - Theme.horizontalPageMargin
                // Row characters come back escaped from core with colours: file never read as
                // markup.
                text: doc.styledRow(modelData.text, modelData.line, modelData.start, doc.highlighted ? page.colours : "")
                font.family: "monospace"
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
    }

    BusyIndicator {
        anchors.centerIn: parent
        size: BusyIndicatorSize.Large
        running: doc.loading || external.busy
    }
}
