// SPDX-License-Identifier: LGPL-2.1-or-later
import QtQuick 2.6
import QtMultimedia 5.6 as MM
import Lautta 1.0
import "../ErrorText.js" as ErrorText

Item {
    id: playback

    property string uri
    property Item output

    readonly property bool local: App.isLocal(uri)
    readonly property QtObject videoSource: local ? localPlayer : remote
    readonly property bool playing: local ? localPlayer.playbackState === MM.MediaPlayer.PlayingState : remote.playing
    readonly property real position: local ? localPlayer.position : remote.position
    readonly property real duration: local ? localPlayer.duration : remote.duration
    readonly property string errorKind: local ? remote.playerErrorKind(localPlayer.error, localPlayer.status) : remote.errorKind
    readonly property bool failed: ErrorText.stopsPlayback(errorKind)
    readonly property bool loading: remote.loading
    readonly property bool downloading: remote.downloading
    readonly property real downloadProgress: remote.downloadProgress
    readonly property string statusText: {
        if (errorKind !== "")
            return ErrorText.message(errorKind, { "item": App.nameOf(uri), "location": App.locationName(uri), "open": true })
        if (remote.downloading)
            //% "Downloading from %1"
            return qsTrId("lautta-viewers-media-downloading").arg(App.locationName(uri))
        if (!local)
            //% "Streaming from %1"
            return qsTrId("lautta-viewers-media-streaming").arg(App.locationName(uri))
        return ""
    }

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

    function keepPipelineIfStuck() {
        var media = local ? localPlayer : remote.mediaObject
        if (output)
            tools.keepWhileLoading(media, output)
        if (local)
            tools.keepWhileLoading(media, localPlayer)
    }

    MM.MediaPlayer {
        id: localPlayer
        source: playback.local ? App.localUrl(playback.uri) : ""
        autoPlay: true
    }

    MediaSource {
        id: remote
        uri: playback.local ? "" : playback.uri
        Component.onCompleted: play()
    }

    ViewerTools { id: tools }

    Component.onDestruction: {
        if (local)
            localPlayer.stop()
        else
            remote.stop()
        keepPipelineIfStuck()
    }
}
