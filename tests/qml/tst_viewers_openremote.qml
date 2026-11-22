// SPDX-License-Identifier: LGPL-2.1-or-later
import QtQuick 2.6
import Sailfish.Silica 1.0
import Lautta 1.0
import "../../qml/dialogs"

ApplicationWindow {
    id: window
    property bool reachedEnd
    property string fetchedUrl
    readonly property string localFile: "lautta://user-documents/clip.md"

    ViewerTools { id: tools }

    function waitFor(done, what) {
        for (var i = 0; i < 200 && !done(); ++i)
            tools.pump(50)
        if (!done())
            console.error("FAILED: timed out waiting for " + what)
        return done()
    }

    Component.onCompleted: {
        var page = pageStack.push(pageComponent, { "uri": "lautta://nv-nas/docs/notes.md" }, PageStackAction.Immediate)
        if (!waitFor(function () { return !page.working && page.errorKind !== "" }, "an unknown server's error"))
            return

        // Share: fetched file goes to the opener, then the page goes.
        if (!tools.installFixture(Qt.resolvedUrl("host/fixtures/doc.md"), localFile)) {
            console.error("FAILED: fixture")
            return
        }
        page = pageStack.push(pageComponent, { "uri": localFile, "share": true }, PageStackAction.Immediate)
        page.fetched.connect(function (fileUrl) { window.fetchedUrl = fileUrl })
        if (!waitFor(function () { return window.fetchedUrl !== "" }, "the shared file"))
            return
        if (fetchedUrl.indexOf("file://") !== 0 || fetchedUrl.slice(-"/Documents/clip.md".length) !== "/Documents/clip.md") {
            console.error("FAILED: shared file url " + fetchedUrl)
            return
        }
        if (!waitFor(function () { return pageStack.depth === 1 && !pageStack.busy }, "Getting page to close after share"))
            return
        reachedEnd = true
    }

    Component {
        id: pageComponent
        OpenRemoteDialog { }
    }
}
