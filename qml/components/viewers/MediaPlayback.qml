// SPDX-License-Identifier: LGPL-2.1-or-later
import QtQuick 2.6
import Lautta 1.0
import "../ErrorText.js" as ErrorText

Item {
    id: playback

    property string uri
    property Item output

    readonly property bool local: player.local
    readonly property QtObject videoSource: player
    readonly property bool playing: player.playing
    readonly property real position: player.position
    readonly property real duration: player.duration
    readonly property string errorKind: player.errorKind
    readonly property bool failed: ErrorText.stopsPlayback(errorKind)
    readonly property bool loading: player.loading
    readonly property bool downloading: player.downloading
    readonly property real downloadProgress: player.downloadProgress
    readonly property string statusText: {
        if (errorKind !== "")
            return ErrorText.message(errorKind, { "item": App.nameOf(uri), "location": App.locationName(uri), "open": true })
        if (player.downloading)
            //% "Downloading from %1"
            return qsTrId("lautta-viewers-media-downloading").arg(App.locationName(uri))
        if (!local)
            //% "Streaming from %1"
            return qsTrId("lautta-viewers-media-streaming").arg(App.locationName(uri))
        return ""
    }

    function toggle() {
        if (playing)
            player.pause()
        else
            player.play()
    }

    function seek(ms) {
        player.seek(Math.max(0, Math.min(duration > 0 ? duration : ms, ms)))
    }

    function keepPipelineIfStuck() {
        if (output)
            tools.keepWhileLoading(player.mediaObject, output)
    }

    MediaSource {
        id: player
        uri: playback.uri
        Component.onCompleted: play()
    }

    ViewerTools { id: tools }

    Component.onDestruction: {
        player.stop()
        keepPipelineIfStuck()
    }
}
