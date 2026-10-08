// SPDX-License-Identifier: LGPL-2.1-or-later
// Opening a remote file in another app (PRV-5, PRV-6, board OpenRemote): the
// file is copied to ~/Downloads/Lautta/Opened so the sandboxed app can read
// it, then handed to the system. The first use explains the copy in one
// line (SEC-4).
import QtQuick 2.6
import Sailfish.Silica 1.0
import Nemo.Configuration 1.0
import Lautta 1.0
import "../components/ErrorText.js" as ErrorText

Page {
    id: page

    property string uri

    readonly property string fileName: App.nameOf(uri)
    property bool working: true
    property string errorKind
    property string errorMessage

    function start() {
        working = true
        errorKind = ""
        App.prepareExternal(uri)
    }

    function finish(fileUrl) {
        working = false
        tools.noteOpened(uri)
        hint.value = true
        Qt.openUrlExternally(fileUrl)
        pageStack.pop()
    }

    allowedOrientations: Orientation.All
    Component.onCompleted: start()

    ViewerTools { id: tools }

    // Remembers that the explanation has been shown.
    ConfigurationValue {
        id: hint
        key: "/apps/harbour-lautta/hints/open-remote-seen"
        defaultValue: false
    }

    Connections {
        target: App
        onExternalReady: {
            if (uri === page.uri && page.working)
                page.finish(fileUrl)
        }
        onExternalFailed: {
            if (uri === page.uri) {
                page.working = false
                page.errorKind = kind
                page.errorMessage = message
            }
        }
    }

    Column {
        anchors.centerIn: parent
        width: parent.width
        spacing: Theme.paddingLarge

        PageHeader {
            title: page.fileName
            description: App.locationName(page.uri)
        }

        BusyIndicator {
            anchors.horizontalCenter: parent.horizontalCenter
            size: BusyIndicatorSize.Large
            running: page.working
        }

        Label {
            x: Theme.horizontalPageMargin
            width: parent.width - 2 * Theme.horizontalPageMargin
            horizontalAlignment: Text.AlignHCenter
            wrapMode: Text.Wrap
            color: page.errorKind !== "" ? Theme.highlightColor : Theme.secondaryHighlightColor
            text: page.errorKind !== ""
                  ? ErrorText.message(page.errorKind, { "item": page.fileName, "location": App.locationName(page.uri) })
                  //% "Getting %1"
                  : qsTrId("lautta-viewers-getting").arg(page.fileName)
        }

        Button {
            anchors.horizontalCenter: parent.horizontalCenter
            visible: page.errorKind !== ""
            //% "Try again"
            text: qsTrId("lautta-viewers-retry")
            onClicked: page.start()
        }
    }

    InteractionHintLabel {
        anchors.bottom: parent.bottom
        opacity: hint.value ? 0 : 1
        visible: opacity > 0
        //% "A copy goes to Downloads/Lautta/Opened so other apps can read it. It's removed after 24 hours unless pinned."
        text: qsTrId("lautta-viewers-open-remote-hint")
        Behavior on opacity { FadeAnimation { } }
    }
}
