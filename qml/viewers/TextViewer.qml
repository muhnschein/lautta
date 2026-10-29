// SPDX-License-Identifier: LGPL-2.1-or-later
// Text and code viewer (PRV-4, boards TextViewer, TextViewerPulley): read-only
// up to 1 MiB with line numbers, the lines wrapped to the page's width,
// coloured by the file's language (PRV-13), on a backdrop that darkens a dark
// ambience and lightens a light one.
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
    // The text in rows (TextRows.js): the list's model.
    readonly property var layout: doc.loaded ? TextRows.rows(doc.text) : TextRows.empty
    readonly property real charWidth: metrics.advanceWidth
    readonly property real gutter: Math.max(2, String(layout.lines).length) * charWidth + Theme.paddingLarge
    // The ambience's colours for the syntax classes (PRV-13).
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

    // Colour Page.highContrast dims a page with, stronger: black on a dark
    // ambience, light grey on a light one. Opaque enough for text and syntax
    // colours to read over any wallpaper (PRV-4).
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
        // Never sideways: a list that can flick sideways keeps the swipe
        // back from the page stack (PRV-4).
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
            // The list is put at its top (above the header) when the text
            // arrives, and a ListView keeps contentY when its header grows:
            // a description line that came only with the text would push
            // the top of the header out of view. So the line is there,
            // blank, from the start.
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
                // A line's number on its first row only.
                text: modelData.number > 0 ? modelData.number : ""
                font.family: "monospace"
                font.pixelSize: Theme.fontSizeExtraSmall
                color: Theme.secondaryColor
                opacity: Theme.opacityHigh
            }
            Label {
                id: line
                x: page.gutter
                // Never implicitWidth: wrapping makes that depend on the
                // width again (a binding loop).
                width: parent.width - page.gutter - Theme.horizontalPageMargin
                // The row's characters come back escaped from the core, with
                // the colours of its part of the line, so nothing in the file
                // is read as markup (PRV-13).
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
