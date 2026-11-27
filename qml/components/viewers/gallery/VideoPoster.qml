// SPDX-FileCopyrightText: 2013-2023 Jolla Ltd.
// SPDX-FileCopyrightText: 2024-2025 Jolla Mobile Ltd
//
// SPDX-License-Identifier: BSD-3-Clause
//
// sailfish-components-gallery's src/VideoPoster.qml, adapted for lautta
// (PRV-4, see LAUTTA.md): the poster is the system thumbnailer's frame of a
// device video, asked with its type as the Gallery does (none for server
// videos), and the slot owns its player:
// lautta's MediaPlayback with a VideoOutput, made at the first tap on play
// and gone once the slot is no longer current, so nothing plays or streams
// off screen. Server videos show the bridge's download and streaming states.
import QtQuick 2.6
import Sailfish.Silica 1.0
import QtMultimedia 5.6 as MM
import Nemo.KeepAlive 1.2
import Nemo.Thumbnailer 1.0
import Lautta 1.0
import ".."

Item {
    id: root

    property string uri
    property string mimeType
    property bool loadAllowed
    property bool active
    property bool overlayMode
    property bool started

    readonly property QtObject player: playerLoader.item ? playerLoader.item.playback : null
    readonly property bool playing: player !== null && player.playing
    readonly property bool loaded: player !== null && (player.playing || player.position > 0)
    readonly property bool busy: player !== null && (player.loading || (player.downloading && player.downloadProgress < 0))
    readonly property bool error: player !== null && player.failed
    readonly property bool settled: poster.status !== Thumbnail.Loading
    readonly property bool zoomed: false

    signal clicked

    function togglePlay() {
        if (!started)
            started = true
        else if (player)
            player.toggle()
    }

    onActiveChanged: if (!active) started = false

    MouseArea {
        id: tapArea

        objectName: "videoTapArea"
        anchors {
            fill: parent
            margins: Theme.paddingLarge
        }
        onClicked: root.clicked()
    }

    Thumbnail {
        id: poster

        objectName: "videoPoster"
        anchors.fill: parent
        source: root.loadAllowed && App.isLocal(root.uri) ? App.localUrl(root.uri) : ""
        mimeType: root.mimeType
        sourceSize.width: Screen.height
        sourceSize.height: Screen.height
        priority: Thumbnail.HighPriority
        fillMode: Thumbnail.PreserveAspectFit
        opacity: !root.loaded ? 1.0 : 0.0
        Behavior on opacity { FadeAnimator {} }
        visible: !root.loaded
    }

    Loader {
        id: playerLoader

        objectName: "videoPlayer"
        anchors.fill: parent
        active: root.started

        sourceComponent: Item {
            property alias playback: playback

            MM.VideoOutput {
                id: output
                anchors.fill: parent
                source: playback.videoSource
                fillMode: MM.VideoOutput.PreserveAspectFit
            }

            MediaPlayback {
                id: playback
                uri: root.uri
                output: output
            }
        }
    }

    DisplayBlanking {
        preventBlanking: root.playing
    }

    BusyIndicator {
        anchors.centerIn: parent
        size: BusyIndicatorSize.Large
        running: root.busy
    }

    IconButton {
        id: playButton

        objectName: "playButton"
        anchors.centerIn: parent
        width: icon.width
        height: icon.height
        enabled: !root.busy && (root.overlayMode || !root.playing) && !root.error
                 && !(root.player && root.player.downloading)
        opacity: enabled ? 1.0 : 0.0
        Behavior on opacity { FadeAnimator {} }

        icon.color: Theme.lightPrimaryColor
        onClicked: root.togglePlay()

        Binding {
            target: playButton.icon
            when: root.overlayMode || !root.playing
            property: "source"
            value: "image://theme/icon-video-overlay-" + (root.playing ? "pause" : "play")
        }
    }

    ProgressBar {
        objectName: "videoDownload"
        width: parent.width
        anchors.centerIn: parent
        visible: root.player !== null && root.player.downloading && root.player.downloadProgress >= 0
        minimumValue: 0
        maximumValue: 1
        value: root.player ? Math.max(0, root.player.downloadProgress) : 0
        //% "Getting the file before playing"
        label: qsTrId("lautta-viewers-media-getting")
    }

    Label {
        objectName: "videoStatus"
        x: Theme.horizontalPageMargin
        y: playButton.y + playButton.height + Theme.paddingLarge
        width: parent.width - 2 * x
        horizontalAlignment: Text.AlignHCenter
        wrapMode: Text.Wrap
        text: root.player && !root.error ? root.player.statusText : ""
        font.pixelSize: Theme.fontSizeSmall
        color: Theme.lightPrimaryColor
        opacity: text !== "" && root.overlayMode ? 1.0 : 0.0
        visible: opacity > 0
        Behavior on opacity { FadeAnimator {} }
    }

    InfoLabel {
        objectName: "videoError"
        anchors.verticalCenter: parent.verticalCenter
        text: root.error ? root.player.statusText : ""
        visible: root.error
    }
}
