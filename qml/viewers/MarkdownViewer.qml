// SPDX-License-Identifier: LGPL-2.1-or-later
import QtQuick 2.6
import Sailfish.Silica 1.0
import Lautta 1.0
import "../components/viewers"
import "../components/ErrorText.js" as ErrorText
import "../components/viewers/ReadingStyle.js" as ReadingStyle

Page {
    id: page

    property string uri
    readonly property string coverKind: "file"
    readonly property string coverUri: uri

    readonly property string fileName: App.nameOf(uri)

    allowedOrientations: Orientation.All

    MarkdownDocument {
        id: doc
        colours: ReadingStyle.colours(Theme)
        uri: page.uri
    }

    ExternalActions {
        id: external
        uri: page.uri
    }

    // Same backdrop as text viewer: long text readable over any wallpaper.
    Rectangle {
        objectName: "readingBackdrop"
        anchors.fill: parent
        color: Theme.rgba(palette.overlayBackgroundColor, Theme.opacityHigh)
    }

    // One Label per block, made only near screen: one rich text of a big file blocks GUI thread.
    SilicaListView {
        id: view

        objectName: "readingView"
        anchors.fill: parent
        model: doc.blockCount
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
            MenuItem {
                //% "View source"
                text: qsTrId("lautta-viewers-view-source")
                onClicked: pageStack.push(Qt.resolvedUrl("TextViewer.qml"), { "uri": page.uri })
            }
        }

        header: PageHeader {
            width: view.width
            title: page.fileName
        }

        // Viewer draws quote bar and code panel: Qt rich text paints cell backgrounds only behind text.
        delegate: Item {
            id: block

            readonly property string kind: doc.blockKind(index)
            readonly property bool joined: doc.blockJoined(index)
            readonly property bool continued: index > 0 && doc.blockJoined(index - 1)
            readonly property bool code: kind === "code"
            readonly property bool quote: kind === "quote"
            readonly property real inset: code ? Theme.paddingMedium : 0
            readonly property real above: (code && !continued ? Theme.paddingMedium : 0)
                                        + (kind === "heading" && index > 0 ? Theme.paddingLarge : 0)
            readonly property real below: code && !joined ? Theme.paddingMedium : 0

            width: view.width
            height: above + label.height + below + (joined ? 0 : Theme.paddingLarge)

            Rectangle {
                objectName: "codePanel"
                visible: block.code
                x: Theme.horizontalPageMargin
                y: block.continued ? 0 : block.above - Theme.paddingMedium
                width: parent.width - 2 * Theme.horizontalPageMargin
                height: label.y + label.height + block.below - y
                color: Theme.rgba(Theme.highlightBackgroundColor, Theme.opacityFaint)
            }

            Rectangle {
                objectName: "quoteBar"
                visible: block.quote
                x: Theme.horizontalPageMargin
                width: Math.max(2, Math.round(Theme.paddingLarge / 8))
                height: label.height
                color: Theme.highlightColor
            }

            Label {
                id: label

                x: Theme.horizontalPageMargin + block.inset + (block.quote ? Theme.paddingLarge : 0)
                y: block.above
                width: block.width - x - Theme.horizontalPageMargin - block.inset
                textFormat: Text.RichText
                wrapMode: Text.Wrap
                color: block.quote ? Theme.secondaryColor : Theme.primaryColor
                linkColor: Theme.highlightColor
                font.pixelSize: Theme.fontSizeMedium
                text: doc.block(index)
                onLinkActivated: Qt.openUrlExternally(link)
            }
        }

        footer: Item {
            width: view.width
            height: truncatedLabel.height + Theme.paddingLarge

            Label {
                id: truncatedLabel
                x: Theme.horizontalPageMargin
                width: parent.width - 2 * Theme.horizontalPageMargin
                visible: doc.truncated
                height: visible ? implicitHeight : 0
                wrapMode: Text.Wrap
                color: Theme.highlightColor
                font.pixelSize: Theme.fontSizeSmall
                //% "Showing the first 1 MB"
                text: qsTrId("lautta-viewers-truncated-short")
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
