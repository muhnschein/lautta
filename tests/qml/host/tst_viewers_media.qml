// SPDX-License-Identifier: LGPL-2.1-or-later
import QtQuick 2.6
import Lautta 1.0
import "../../../qml/components/ErrorText.js" as ErrorText
import "../../../qml/components/viewers"

Item {
    id: root
    property bool reachedEnd

    function check(cond, what) {
        if (!cond)
            console.error("FAILED: " + what)
    }

    function until(done) {
        for (var i = 0; i < 1200 && !done(); ++i)
            tools.pump(25)
    }

    ViewerTools { id: tools }
    MediaSource { id: idle }
    MediaSource { id: streaming }
    MediaSource { id: copying; preferDownload: true }
    MediaSource { id: deepCopy; preferDownload: true }
    MediaPlayback { id: deep }

    Component.onCompleted: {
        check(tools.installFixture(Qt.resolvedUrl("fixtures/data.bin"), "lautta://user-documents/a.mp3"), "fixture")
        check(tools.installFixture(Qt.resolvedUrl("fixtures/nested-boxes.mp4"), "lautta://user-documents/deep.mp4"),
              "nested fixture")

        check(idle.state === "" && !idle.playing && idle.uri === "" && !idle.local, "idle source")
        idle.play()
        idle.seek(1000)
        idle.pause()
        idle.stop()
        check(!idle.playing, "controls on an idle source are harmless")
        check(idle.playerErrorKind(0, 6) === "", "no error, no kind")
        check(idle.playerErrorKind(2, 8) === "NotPlayable", "unplayable file: " + idle.playerErrorKind(2, 8))
        check(idle.playerErrorKind(2, 6) === "NoSound",
              "missing decoder while the rest plays: " + idle.playerErrorKind(2, 6))
        check(idle.playerErrorKind(4, 8) === "PermissionDenied", "access denied: " + idle.playerErrorKind(4, 8))
        check(!ErrorText.stopsPlayback("") && !ErrorText.stopsPlayback("NoSound"),
              "a video without its sound keeps playing its picture")
        check(ErrorText.stopsPlayback("NotPlayable") && ErrorText.stopsPlayback("ConnectionLost"),
              "other errors stop the video")

        streaming.uri = "lautta://user-documents/a.mp3"
        check(streaming.mediaObject !== null && streaming.mediaObject !== undefined, "mediaObject is the player")
        check(streaming.mode === "stream" && streaming.loading, "streaming starts")
        streaming.play()
        until(function() { return !streaming.loading && streaming.errorKind !== "" })
        check(!streaming.loading, "source opened")
        check(streaming.errorKind === "NotPlayable", "no backend is reported: '" + streaming.errorKind + "'")
        check(!streaming.playing && streaming.state === "stopped", "not playing")
        check(streaming.local && streaming.mode === "stream" && !streaming.downloading,
              "a device file streams: " + streaming.local + " " + streaming.mode)
        streaming.uri = "lautta://user-documents/missing.mp3"
        until(function() { return !streaming.loading && streaming.errorKind !== "" })
        check(streaming.errorKind === "NotFound", "missing file: " + streaming.errorKind)

        copying.uri = "lautta://user-documents/a.mp3"
        check(copying.mode === "download" && copying.downloading, "download mode from the start")
        until(function() { return !copying.downloading })
        check(!copying.downloading && copying.downloadProgress === 1, "downloaded: " + copying.downloadProgress)
        check(copying.mode === "download", "stays in download mode")

        deep.uri = "lautta://user-documents/deep.mp4"
        check(deep.videoSource.uri === deep.uri, "device files play through the checked source")
        until(function() { return !deep.loading && deep.errorKind !== "" })
        check(deep.errorKind === "NotPlayable" && deep.failed, "nested boxes are refused: " + deep.errorKind)
        check(deep.videoSource.errorMessage === "media nested too deeply to play",
              "refused before the player sees it: " + deep.videoSource.errorMessage)
        deepCopy.uri = "lautta://user-documents/deep.mp4"
        until(function() { return !deepCopy.downloading && deepCopy.errorKind !== "" })
        check(deepCopy.errorMessage === "media nested too deeply to play",
              "a downloaded copy is checked too: " + deepCopy.errorMessage)
        reachedEnd = true
    }
}
