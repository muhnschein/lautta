// SPDX-License-Identifier: LGPL-2.1-or-later
import QtQuick 2.6
import Sailfish.Silica 1.0
import Lautta 1.0
import "../../qml/viewers"

ApplicationWindow {
    id: window

    ViewerTools { id: tools }

    Component.onCompleted: {
        tools.installFixture(Qt.resolvedUrl("host/fixtures/data.bin"), "lautta://user-documents/clip.mp4")
        var page = pageStack.push(pageComponent, {}, PageStackAction.Immediate)
        for (var i = 0; i < 400 && page.statusText === ""; ++i)
            tools.pump(25)
        if (page.statusText !== "lautta-err-not-playable" && page.statusText !== "Can't play clip.mp4.")
            console.error("FAILED: a file that isn't a video: '" + page.statusText + "'")
    }

    Component {
        id: pageComponent
        MediaPlayer { uri: "lautta://user-documents/clip.mp4"; video: true }
    }
}
