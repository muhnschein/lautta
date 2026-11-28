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
        if (!tools.installLink("/dev/zero", "lautta://user-documents/zero.mp3"))
            console.error("FAILED: link to a device")
        var page = pageStack.push(pageComponent, {}, PageStackAction.Immediate)
        waitFor(function () { return page.errorKind !== "" }, "an error for a link to a device")
        if (page.errorKind !== "NotAFile" || page.statusText.length === 0 || page.playing)
            console.error("FAILED: a link to a device is refused, not played: '" + page.errorKind + "' '"
                          + page.statusText + "'")
        reachedEnd = true
    }

    Component {
        id: pageComponent
        MediaPlayer { uri: "lautta://user-documents/zero.mp3" }
    }
}
