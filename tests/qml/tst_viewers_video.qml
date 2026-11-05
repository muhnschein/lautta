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
        if (!tools.installFixture(Qt.resolvedUrl("host/fixtures/data.bin"), "lautta://user-documents/clip.mp4"))
            console.error("FAILED: fixture")
        var page = pageStack.push(pageComponent, {}, PageStackAction.Immediate)
        waitFor(function () { return page.errorKind !== "" }, "no video in the file")
        if ((page.statusText !== "lautta-err-not-playable" && page.statusText !== "Can't play clip.mp4.") || page.playing)
            console.error("FAILED: the file is named as unplayable, nothing plays: '" + page.statusText + "'")
        reachedEnd = true
    }

    Component {
        id: pageComponent
        MediaPlayer { uri: "lautta://user-documents/clip.mp4"; video: true }
    }
}
