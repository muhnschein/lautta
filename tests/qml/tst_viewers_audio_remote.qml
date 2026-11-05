// SPDX-License-Identifier: LGPL-2.1-or-later
import QtQuick 2.6
import Sailfish.Silica 1.0
import Lautta 1.0
import "../../qml/viewers"

ApplicationWindow {
    id: window
    property bool reachedEnd

    ViewerTools { id: tools }

    function waitFor(done, what) {
        for (var i = 0; i < 200 && !done(); ++i)
            tools.pump(50)
        if (!done())
            console.error("FAILED: timed out waiting for " + what)
        return done()
    }

    Component.onCompleted: {
        var page = pageStack.push(pageComponent, {}, PageStackAction.Immediate)
        waitFor(function () { return page.errorKind !== "" }, "an unknown server's error")
        if (page.statusText.length === 0 || page.playing)
            console.error("FAILED: the error is said, nothing plays: '" + page.statusText + "'")
        reachedEnd = true
    }

    Component {
        id: pageComponent
        MediaPlayer { uri: "lautta://nv-nas/audio/chapter.mp3"; video: false }
    }
}
