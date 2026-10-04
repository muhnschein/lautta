// SPDX-License-Identifier: LGPL-2.1-or-later
// Audio and video player (PRV-4, PRV-8, PRV-9, boards VideoPlayer and
// AudioPlayer). Local files use QtMultimedia's MediaPlayer directly; remote
// files play through MediaSource (a QIODevice over the bridge read handle,
// or download-then-play when the device's backend cannot stream).
import QtQuick 2.6
import Sailfish.Silica 1.0
// This file is itself called MediaPlayer, so QtMultimedia's types are
// reached through a prefix.
import QtMultimedia 5.6 as MM
import Nemo.KeepAlive 1.2
import Lautta 1.0
import "../components/viewers"
import "../components/viewers/ViewerText.js" as ViewerText
import "../components/ErrorText.js" as ErrorText

Page {
    id: page

    property string uri
    property bool video

    readonly property bool local: App.isLocal(uri)
    readonly property string fileName: App.nameOf(uri)
    readonly property bool playing: local ? localPlayer.playbackState === MM.MediaPlayer.PlayingState : remote.playing
    readonly property real position: local ? localPlayer.position : remote.position
    readonly property real duration: local ? localPlayer.duration : remote.duration
    readonly property string errorKind: local ? (localPlayer.error !== MM.MediaPlayer.NoError ? "Unsupported" : "") : remote.errorKind
    property bool chrome: true

    function toggle() {
        if (local) {
            if (playing)
                localPlayer.pause()
            else
                localPlayer.play()
        } else if (playing) {
            remote.pause()
        } else {
            remote.play()
        }
    }

    function seek(ms) {
        var target = Math.max(0, Math.min(duration > 0 ? duration : ms, ms))
        if (local)
            localPlayer.seek(target)
        else
            remote.seek(target)
    }

    allowedOrientations: Orientation.All

    MM.MediaPlayer {
        id: localPlayer
        source: page.local ? App.localUrl(page.uri) : ""
        autoPlay: true
    }

    MediaSource {
        id: remote
        uri: page.local ? "" : page.uri
        Component.onCompleted: play()
    }

    ViewerTools { id: tools }

    Component.onCompleted: if (local) tools.noteViewed(uri)
    Component.onDestruction: {
        if (local)
            localPlayer.stop()
        else
            remote.stop()
    }

    DisplayBlanking {
        preventBlanking: page.video && page.playing
    }

    Rectangle {
        anchors.fill: parent
        color: page.video ? "black" : "transparent"
    }

    MM.VideoOutput {
        anchors.fill: parent
        visible: page.video
        source: page.local ? localPlayer : remote
        fillMode: MM.VideoOutput.PreserveAspectFit
    }

    MouseArea {
        anchors.fill: parent
        enabled: page.video
        onClicked: page.chrome = !page.chrome
    }

    // What the player is doing: where from, download progress, errors.
    readonly property string statusText: {
        if (errorKind !== "")
            return ErrorText.message(errorKind, { "item": fileName, "location": App.locationName(uri) })
        if (remote.downloading)
            //% "Downloading from %1"
            return qsTrId("lautta-viewers-media-downloading").arg(App.locationName(uri))
        if (!local)
            //% "Streaming from %1"
            return qsTrId("lautta-viewers-media-streaming").arg(App.locationName(uri))
        return ViewerText.where(App.locationName(uri), App.displayAddress(App.parentUri(uri)))
    }

    PageHeader {
        id: header
        opacity: page.chrome || !page.video ? 1 : 0
        visible: opacity > 0
        title: page.fileName
        description: page.statusText
        Behavior on opacity { FadeAnimation { } }
    }

    // Audio: a cover placeholder in the middle.
    Rectangle {
        anchors { horizontalCenter: parent.horizontalCenter; top: header.bottom; topMargin: Theme.paddingLarge * 2 }
        visible: !page.video && page.height > page.width
        width: Math.min(parent.width - 2 * Theme.horizontalPageMargin, Theme.itemSizeHuge * 2.4)
        height: width
        color: Theme.rgba(Theme.highlightBackgroundColor, Theme.highlightBackgroundOpacity)

        Image {
            anchors.centerIn: parent
            source: "image://theme/icon-l-music?" + Theme.highlightColor
        }
    }

    BusyIndicator {
        anchors.centerIn: parent
        size: BusyIndicatorSize.Large
        running: remote.loading || (remote.downloading && remote.downloadProgress < 0)
    }

    // Download progress while the fallback copies the file (PRV-9).
    ProgressBar {
        anchors { left: parent.left; right: parent.right; verticalCenter: parent.verticalCenter }
        visible: remote.downloading && remote.downloadProgress >= 0
        minimumValue: 0
        maximumValue: 1
        value: Math.max(0, remote.downloadProgress)
        //% "Getting the file before playing"
        label: qsTrId("lautta-viewers-media-getting")
    }

    Column {
        id: controls

        anchors { left: parent.left; right: parent.right; bottom: parent.bottom }
        opacity: page.chrome || !page.video ? 1 : 0
        visible: opacity > 0 && !remote.downloading
        Behavior on opacity { FadeAnimation { } }

        Slider {
            id: slider
            width: parent.width
            minimumValue: 0
            maximumValue: Math.max(1, page.duration)
            enabled: page.duration > 0
            handleVisible: page.duration > 0
            valueText: ViewerText.clock(value)
            label: page.duration > 0 ? ViewerText.clock(page.duration) : ""
            onReleased: page.seek(value)

            Binding {
                target: slider
                property: "value"
                value: page.position
                when: !slider.down
            }
        }

        Row {
            anchors.horizontalCenter: parent.horizontalCenter
            spacing: Theme.paddingLarge * 2

            IconButton {
                icon.source: "image://theme/icon-m-previous"
                onClicked: page.seek(0)
            }
            IconButton {
                icon.source: page.playing ? "image://theme/icon-l-pause" : "image://theme/icon-l-play"
                onClicked: page.toggle()
            }
            IconButton {
                icon.source: "image://theme/icon-m-next"
                onClicked: page.seek(page.position + 30000)
            }
        }

        Item { width: 1; height: Theme.paddingLarge }
    }
}
