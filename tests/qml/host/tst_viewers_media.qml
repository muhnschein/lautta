// SPDX-License-Identifier: LGPL-2.1-or-later
// Host test of MediaSource (PRV-8, PRV-9). The host has no GStreamer
// backend, so playback itself cannot be verified here: what is checked is the
// wiring (the player object, state, errors) and the download fallback path.
import QtQuick 2.6
import Lautta 1.0

Item {
    id: root

    function check(cond, what) {
        if (!cond)
            console.error("FAILED: " + what)
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

        // Streaming: the player object exists at once for VideoOutput.
        streaming.uri = "lautta://user-documents/a.mp3"
        check(streaming.mediaObject !== null && streaming.mediaObject !== undefined, "mediaObject is the player")
        check(streaming.mode === "stream" && streaming.loading, "streaming starts")
        streaming.play()
        tools.pump(900)
        check(!streaming.loading, "source opened")
        // Without a multimedia backend the player reports a missing service.
        check(streaming.errorKind === "Unsupported", "no backend is reported: '" + streaming.errorKind + "'")
        check(!streaming.playing && streaming.state === "stopped", "not playing")
        streaming.uri = "lautta://user-documents/missing.mp3"
        tools.pump(500)
        check(streaming.errorKind === "NotFound" || streaming.errorKind === "Unsupported", "missing file: " + streaming.errorKind)

        // PRV-9: download-then-play copies the file first.
        copying.uri = "lautta://user-documents/a.mp3"
        check(copying.mode === "download" && copying.downloading, "download mode from the start")
        tools.pump(900)
        check(!copying.downloading && copying.downloadProgress === 1, "downloaded: " + copying.downloadProgress)
        check(copying.mode === "download", "stays in download mode")
    }
}
