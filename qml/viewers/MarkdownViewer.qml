// SPDX-License-Identifier: LGPL-2.1-or-later
// Markdown viewer (PRV-4, board Markdown): the text rendered to the Qt rich
// text subset in the core; nothing from the file is trusted or fetched.
import QtQuick 2.6
import Sailfish.Silica 1.0
import Lautta 1.0
import "../components/viewers"
import "../components/viewers/ViewerText.js" as ViewerText
import "../components/ErrorText.js" as ErrorText

Page {
    id: page

    property string uri

    readonly property string fileName: App.nameOf(uri)

    allowedOrientations: Orientation.All

    MarkdownDocument {
        id: doc
        uri: page.uri
    }

    ExternalActions {
        id: external
        uri: page.uri
    }

    SilicaFlickable {
        anchors.fill: parent
        contentHeight: column.height + Theme.paddingLarge

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

        Column {
            id: column
            width: parent.width

            PageHeader {
                title: page.fileName
                description: ViewerText.where(App.locationName(page.uri), App.displayAddress(App.parentUri(page.uri)))
            }

            Label {
                x: Theme.horizontalPageMargin
                width: parent.width - 2 * Theme.horizontalPageMargin
                visible: doc.html.length > 0
                textFormat: Text.RichText
                wrapMode: Text.Wrap
                color: Theme.primaryColor
                linkColor: Theme.highlightColor
                font.pixelSize: Theme.fontSizeMedium
                text: doc.html
                onLinkActivated: Qt.openUrlExternally(link)
            }

            Label {
                x: Theme.horizontalPageMargin
                width: parent.width - 2 * Theme.horizontalPageMargin
                visible: doc.truncated
                wrapMode: Text.Wrap
                color: Theme.highlightColor
                font.pixelSize: Theme.fontSizeSmall
                //% "Showing the first 1 MiB"
                text: qsTrId("lautta-viewers-truncated-short")
            }
        }

        ViewPlaceholder {
            enabled: doc.errorKind !== ""
            text: ErrorText.message(doc.errorKind, { "item": page.fileName, "location": App.locationName(page.uri) })
        }

        BusyIndicator {
            anchors.centerIn: parent
            size: BusyIndicatorSize.Large
            running: doc.loading || external.busy
        }

        VerticalScrollDecorator { }
    }
}
