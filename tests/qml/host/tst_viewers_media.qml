// SPDX-License-Identifier: LGPL-2.1-or-later Host has no GStreamer backend: wiring and download
// fallback only, not playback.
import QtQuick 2.6
import Lautta 1.0

Item {
    id: root

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

    Component.onCompleted: {
        check(tools.installFixture(Qt.resolvedUrl("fixtures/data.bin"), "lautta://user-documents/a.mp3"), "fixture")

        check(idle.state === "" && !idle.playing && idle.uri === "", "idle source")
        idle.play()
        idle.seek(1000)
        idle.pause()
        idle.stop()
        check(!idle.playing, "controls on an idle source are harmless")

        streaming.uri = "lautta://user-documents/a.mp3"
        check(streaming.mediaObject !== null && streaming.mediaObject !== undefined, "mediaObject is the player")
        check(streaming.mode === "stream" && streaming.loading, "streaming starts")
        streaming.play()
        until(function() { return !streaming.loading && streaming.errorKind !== "" })
        check(!streaming.loading, "source opened")
        check(streaming.errorKind === "Unsupported", "no backend is reported: '" + streaming.errorKind + "'")
        check(!streaming.playing && streaming.state === "stopped", "not playing")
        streaming.uri = "lautta://user-documents/missing.mp3"
        until(function() { return !streaming.loading && streaming.errorKind !== "" })
        check(streaming.errorKind === "NotFound" || streaming.errorKind === "Unsupported", "missing file: " + streaming.errorKind)

        copying.uri = "lautta://user-documents/a.mp3"
        check(copying.mode === "download" && copying.downloading, "download mode from the start")
        until(function() { return !copying.downloading })
        check(!copying.downloading && copying.downloadProgress === 1, "downloaded: " + copying.downloadProgress)
        check(copying.mode === "download", "stays in download mode")
    }
}
